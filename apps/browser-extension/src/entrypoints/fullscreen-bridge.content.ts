// ISOLATED world: relays between the page-side patch (MAIN world, DOM events) and the background.
// The page can dispatch fake DOM events too, so this script never trusts event details: the
// background decides the mode from the rules for the tab's own URL.
import { defineContentScript } from "wxt/utils/define-content-script";
import { browser } from "wxt/browser";
import {
  EV_EXIT,
  EV_FORCE_EXIT,
  EV_MODE,
  EV_REQUEST,
  type BackgroundToTab,
  type ContentToBackground,
  type ModeReply,
} from "../shared/messages";
import { MODES, type FullscreenMode } from "../shared/rules";

function send<T = unknown>(message: ContentToBackground): Promise<T | undefined> {
  return browser.runtime.sendMessage(message).then(
    (reply: T) => reply,
    // The background may be restarting; the page keeps its last mode.
    () => undefined,
  );
}

function setPageMode(mode: FullscreenMode): void {
  document.dispatchEvent(new CustomEvent(EV_MODE, { detail: mode }));
}

export default defineContentScript({
  matches: ["<all_urls>"],
  runAt: "document_start",
  allFrames: true,
  matchAboutBlank: true,
  main() {
    void send<ModeReply>({ kind: "getMode" }).then((reply) => {
      if (reply && MODES.includes(reply.mode)) setPageMode(reply.mode);
    });

    document.addEventListener(EV_REQUEST, () => {
      void send({ kind: "fsRequest" });
    });
    document.addEventListener(EV_EXIT, () => {
      void send({ kind: "fsExit" });
    });

    browser.runtime.onMessage.addListener((message: BackgroundToTab) => {
      if (message?.kind === "setMode" && MODES.includes(message.mode)) {
        setPageMode(message.mode);
      } else if (message?.kind === "forceExit") {
        document.dispatchEvent(new CustomEvent(EV_FORCE_EXIT));
      }
      return undefined;
    });
  },
});
