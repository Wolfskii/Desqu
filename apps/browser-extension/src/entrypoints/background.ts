import { browser } from "wxt/browser";
import { defineBackground } from "wxt/utils/define-background";
import { resolveMode, type FullscreenMode, type SiteRules } from "../shared/rules";
import { getRules, RULES_KEY, saveRulesAsReceived } from "../shared/storage";
import type { BackgroundToTab, ContentToBackground, ModeReply } from "../shared/messages";
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
  if (!port || !connected) return Promise.reject(new Error("Desqu is not connected"));
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
    // Zone mode (phase C) will act on these; until then they are ignored.
    case "zoneReleased":
    case "requestViewport":
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

// ---- messages from content scripts ----------------------------------------------------------

async function handle(message: ContentToBackground, tab: { id?: number; url?: string; title?: string } | undefined): Promise<unknown> {
  const url = tab?.url;
  switch (message.kind) {
    case "getMode":
      return { mode: await modeForUrl(url) } satisfies ModeReply;
    case "fsRequest": {
      // The mode is resolved here, not taken from the page. A page can send any request it likes.
      const mode = await modeForUrl(url);
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

  // SPA navigation changes the URL without a new page load, so push the mode again.
  browser.tabs.onUpdated.addListener((tabId, info, tab) => {
    if (info.url !== undefined) void pushModeToTab(tabId, tab.url ?? info.url);
  });

  // Rule edits apply to open tabs without a reload, and go to Desqu when connected.
  browser.storage.onChanged.addListener((changes, area) => {
    if (area !== "local" || !(RULES_KEY in changes)) return;
    const next = changes[RULES_KEY]?.newValue as SiteRules | undefined;
    if (next) void publishLocalRules(next);
    void pushModesToAllTabs();
  });
});
