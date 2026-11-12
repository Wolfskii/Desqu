import { browser } from "wxt/browser";
import { defineBackground } from "wxt/utils/define-background";
import { resolveMode, type FullscreenMode, type SiteRules } from "../shared/rules";
import { getRules, RULES_KEY, saveRulesAsReceived } from "../shared/storage";
import type { BackgroundToTab, ContentToBackground, MeasureReply, ModeReply } from "../shared/messages";
import type { ClientMessage, ServerMessage } from "../shared/protocol";
import { PendingRequests, browserFromUserAgent, hostStateAfterDisconnect, nextBackoff } from "../shared/bridge-state";
import { setStatus, setZones } from "../shared/status";

/** Must match HOST_NAME in apps/screenbound/module/src/bridge/manifest.rs. */
const HOST_NAME = "io.github.wolfskii.desqu.browser";
const PROTOCOL = 1;

type HostPort = ReturnType<typeof browser.runtime.connectNative>;

let port: HostPort | null = null;
/** True after Desqu answered our hello on the current port. */
let connected = false;
let backoff: number | null = null;
let counter = 0;
const pending = new PendingRequests();
/** `updatedAt` of the rules we last sent or received. A local edit has a different one. */
let lastSyncedUpdatedAt: number | null = null;

// ---- tabs and page modes --------------------------------------------------------------------

/** Mode for a tab's top-level URL. Frames follow their tab, so an embed uses the host page's rule. */
async function modeForUrl(url: string | undefined): Promise<FullscreenMode> {
  if (!url) return "native";
  return resolveMode(await getRules(), url).mode;
}

async function pushModeToTab(tabId: number, url: string | undefined): Promise<void> {
  const mode = await modeForUrl(url);
  const message: BackgroundToTab = { kind: "setMode", mode };
  // Without frameId this reaches every frame of the tab. Pages without our script (chrome://) throw.
  await browser.tabs.sendMessage(tabId, message).catch(() => undefined);
}

async function pushModesToAllTabs(): Promise<void> {
  const tabs = await browser.tabs.query({});
  await Promise.all(tabs.map((tab) => (tab.id === undefined ? undefined : pushModeToTab(tab.id, tab.url))));
}

// ---- bridge to Desqu ------------------------------------------------------------------------

/** Sends a request and waits for its reply. Rejects when Desqu is not connected or does not answer. */
function request(build: (id: string) => ClientMessage): Promise<ServerMessage> {
  // Only the port is needed here: `hello` is the message that makes us "connected", so it must be sent before that.
  if (!port) return Promise.reject(new Error("Desqu is not connected"));
  const id = String(++counter);
  const reply = pending.add(id);
  port.postMessage(build(id));
  return reply;
}

function fireAndForget(build: (id: string) => ClientMessage): void {
  request(build).catch(() => undefined);
}

async function onHostMessage(message: ServerMessage): Promise<void> {
  if ("replyTo" in message) pending.deliver(message);
  switch (message.type) {
    case "bridgeStatus":
      if (message.connected) {
        backoff = null;
        await sayHello();
      } else {
        connected = false;
        await setStatus({ state: "notRunning" });
      }
      return;
    case "helloAck":
      connected = true;
      lastSyncedUpdatedAt = message.rules.updatedAt;
      await saveRulesAsReceived(message.rules);
      await setZones(message.zones);
      await setStatus({ state: "connected", product: message.product, appVersion: message.appVersion });
      return;
    case "rulesChanged":
      lastSyncedUpdatedAt = message.rules.updatedAt;
      await saveRulesAsReceived(message.rules);
      return;
    case "zonesChanged":
      await setZones(message.zones);
      return;
    case "error":
      if (message.code === "unsupportedProtocol") {
        connected = false;
        await setStatus({ state: "unknown" });
      }
      return;
    case "zoneReleased":
      forgetZone(message.tabId);
      return;
    case "requestViewport":
      await reportViewport(message.tabId);
      return;
    default:
      return;
  }
}

async function sayHello(): Promise<void> {
  const rules = await getRules();
  const ua = navigator.userAgent;
  const browserInfo = browserFromUserAgent(ua);
  try {
    await request((id) => ({
      type: "hello",
      id,
      protocol: PROTOCOL,
      extensionVersion: browser.runtime.getManifest().version,
      browser: { name: browserInfo.name, version: browserInfo.version },
      rules,
    }));
  } catch {
    // The reply (or its absence) is handled by onHostMessage; nothing else to do here.
  }
}

function connectHost(): void {
  let next: HostPort;
  try {
    next = browser.runtime.connectNative(HOST_NAME);
  } catch {
    void setStatus({ state: "notInstalled" });
    scheduleReconnect();
    return;
  }
  port = next;
  connected = false;
  next.onMessage.addListener((message) => {
    void onHostMessage(message as ServerMessage);
  });
  next.onDisconnect.addListener(() => {
    // Desqu released every zone when the host went away.
    zoneWindows.clear();
    const lastError = (browser.runtime as unknown as { lastError?: { message?: string } }).lastError;
    const reason = (next as unknown as { error?: { message?: string } }).error?.message ?? lastError?.message;
    port = null;
    connected = false;
    pending.failAll("the native host disconnected");
    void setStatus({ state: hostStateAfterDisconnect(reason) });
    scheduleReconnect();
  });
}

function scheduleReconnect(): void {
  backoff = nextBackoff(backoff);
  setTimeout(connectHost, backoff);
}

/** Local edits (popup, options) go to Desqu when it is connected. Received copies are not sent back. */
async function publishLocalRules(rules: SiteRules): Promise<void> {
  if (rules.updatedAt === lastSyncedUpdatedAt) return;
  lastSyncedUpdatedAt = rules.updatedAt;
  if (connected) fireAndForget((id) => ({ type: "setRules", id, rules }));
}

// ---- zone mode: the page is placed in a Desqu zone --------------------------------------------

/** Tabs placed in a zone, mapped to their window. A zone shows one tab per window. */
const zoneWindows = new Map<number, number>();

const NORMAL_TITLE = "Screenbound Companion";

async function setBadge(tabId: number, text: string, title: string): Promise<void> {
  await browser.action.setBadgeText({ tabId, text }).catch(() => undefined);
  await browser.action.setTitle({ tabId, title }).catch(() => undefined);
}

/** The page viewport from the top frame. Only Firefox answers; elsewhere Desqu measures the window. */
async function measureTab(tabId: number): Promise<MeasureReply["viewport"]> {
  const reply = (await browser.tabs
    .sendMessage(tabId, { kind: "measure" } satisfies BackgroundToTab, { frameId: 0 })
    .catch(() => undefined)) as MeasureReply | undefined;
  return reply?.viewport ?? null;
}

async function enterZone(tab: { id: number; windowId?: number; title?: string }, url: string): Promise<void> {
  const zoneId = resolveMode(await getRules(), url).zoneId;
  if (!connected) {
    await setBadge(tab.id, "!", "Desqu is not running: showing this video in the window");
    return;
  }
  const viewport = await measureTab(tab.id);
  let reply: ServerMessage | undefined;
  try {
    reply = await request((id) => ({
      type: "zoneEnter",
      id,
      tabId: tab.id,
      tabTitle: tab.title ?? "",
      zoneId,
      viewport,
    }));
  } catch {
    reply = undefined;
  }
  if (reply?.type === "zoneApplied" && reply.ok) {
    if (tab.windowId !== undefined) zoneWindows.set(tab.id, tab.windowId);
    await setBadge(tab.id, "", reply.zoneName ? `In the Desqu zone: ${reply.zoneName}` : "In the Desqu zone");
    return;
  }
  const why =
    reply?.type === "zoneApplied" ? (reply.error ?? "not placed")
    : reply?.type === "error" ? reply.message
    : "no answer from Desqu";
  await setBadge(tab.id, "!", `Showing in the window: ${why}`);
}

async function leaveZone(tabId: number): Promise<void> {
  if (!zoneWindows.delete(tabId)) return;
  if (connected) fireAndForget((id) => ({ type: "zoneExit", id, tabId }));
  await setBadge(tabId, "", NORMAL_TITLE);
}

/** Desqu gave the window back by itself; the page stays in window mode. */
function forgetZone(tabId: number): void {
  zoneWindows.delete(tabId);
  void setBadge(tabId, "", NORMAL_TITLE);
}

async function reportViewport(tabId: number): Promise<void> {
  if (!connected) return;
  const viewport = await measureTab(tabId);
  if (viewport) fireAndForget((id) => ({ type: "viewportReport", id, tabId, viewport }));
}

// ---- messages from content scripts ----------------------------------------------------------

async function handle(
  message: ContentToBackground,
  tab: { id?: number; windowId?: number; url?: string; title?: string } | undefined,
): Promise<unknown> {
  const url = tab?.url;
  switch (message.kind) {
    case "getMode":
      return { mode: await modeForUrl(url) } satisfies ModeReply;
    case "fsRequest": {
      // The mode is resolved here, not taken from the page. A page can send any request it likes.
      const mode = await modeForUrl(url);
      if (mode === "zone" && tab?.id !== undefined && url) {
        void enterZone({ id: tab.id, windowId: tab.windowId, title: tab.title }, url);
      }
      if (connected && tab?.id !== undefined) {
        if (mode === "fullscreen") {
          fireAndForget((id) => ({ type: "exemptNext", id, tabTitle: tab.title ?? "" }));
        } else if (mode === "native" && url) {
          const host = safeHost(url);
          if (host) port?.postMessage({ type: "fullscreenNative", tabId: tab.id, host } satisfies ClientMessage);
        }
      }
      return { ok: true, mode };
    }
    case "fsExit":
      if (tab?.id !== undefined) void leaveZone(tab.id);
      return { ok: true };
    default:
      return { ok: false };
  }
}

function safeHost(url: string): string | null {
  try {
    return new URL(url).hostname;
  } catch {
    return null;
  }
}

export default defineBackground(() => {
  connectHost();

  browser.runtime.onMessage.addListener((message: ContentToBackground, sender) =>
    handle(message, sender.tab),
  );

  // SPA navigation changes the URL without a new page load, so push the mode again. A zone tab that
  // navigates away is no longer the video, so it leaves the zone.
  browser.tabs.onUpdated.addListener((tabId, info, tab) => {
    if (info.url === undefined) return;
    void pushModeToTab(tabId, tab.url ?? info.url);
    if (zoneWindows.has(tabId)) void leaveZone(tabId);
  });

  // The zone shows the active tab of its window: switching tabs ends the zone for the other one.
  browser.tabs.onActivated.addListener(({ tabId, windowId }) => {
    for (const [zoneTab, zoneWindow] of [...zoneWindows]) {
      if (zoneWindow === windowId && zoneTab !== tabId) void leaveZone(zoneTab);
    }
  });
  browser.tabs.onRemoved.addListener((tabId) => {
    void leaveZone(tabId);
  });
  browser.windows.onRemoved.addListener((windowId) => {
    for (const [tabId, zoneWindow] of [...zoneWindows]) {
      if (zoneWindow === windowId) zoneWindows.delete(tabId);
    }
  });

  // Rule edits apply to open tabs without a reload, and go to Desqu when connected.
  browser.storage.onChanged.addListener((changes, area) => {
    if (area !== "local" || !(RULES_KEY in changes)) return;
    const next = changes[RULES_KEY]?.newValue as SiteRules | undefined;
    if (next) void publishLocalRules(next);
    void pushModesToAllTabs();
  });
});
