import { browser } from "wxt/browser";
import { defineBackground } from "wxt/utils/define-background";
import { resolveMode, type FullscreenMode } from "../shared/rules";
import { getRules } from "../shared/storage";
import type { BackgroundToTab, ContentToBackground, ModeReply } from "../shared/messages";

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
  await Promise.all(
    tabs.map((tab) => (tab.id === undefined ? undefined : pushModeToTab(tab.id, tab.url))),
  );
}

async function handle(message: ContentToBackground, url: string | undefined): Promise<unknown> {
  switch (message.kind) {
    case "getMode":
      return { mode: await modeForUrl(url) } satisfies ModeReply;
    case "fsRequest":
      // The mode is resolved here, not taken from the page. Later steps (B8, C4) act on it.
      return { ok: true, mode: await modeForUrl(url) };
    case "fsExit":
      return { ok: true };
    default:
      return { ok: false };
  }
}

export default defineBackground(() => {
  browser.runtime.onMessage.addListener((message: ContentToBackground, sender) =>
    handle(message, sender.tab?.url),
  );

  // SPA navigation changes the URL without a new page load, so push the mode again.
  browser.tabs.onUpdated.addListener((tabId, info, tab) => {
    if (info.url !== undefined) void pushModeToTab(tabId, tab.url ?? info.url);
  });

  // Rule edits apply to open tabs without a reload.
  browser.storage.onChanged.addListener((changes, area) => {
    if (area === "local" && "siteRules" in changes) void pushModesToAllTabs();
  });
});
