// Messages between the extension pieces. Page-level DOM events (used between the MAIN-world script
// and the isolated script) and runtime messages (isolated script <-> background).
import type { FullscreenMode } from "./rules";

// DOM events on `document`. Details are strings only.
export const EV_MODE = "desqu-fs-mode"; // isolated -> MAIN, detail: FullscreenMode
export const EV_REQUEST = "desqu-fs-request"; // MAIN -> isolated, page asked for fullscreen
export const EV_EXIT = "desqu-fs-exit"; // MAIN -> isolated, fake fullscreen ended
export const EV_FORCE_EXIT = "desqu-fs-force-exit"; // isolated -> MAIN

/** Sent by content scripts to the background. The background resolves the mode itself. */
export type ContentToBackground =
  | { kind: "getMode" }
  | { kind: "fsRequest" }
  | { kind: "fsExit" };

/** Sent by the background to every frame of a tab. */
export type BackgroundToTab =
  | { kind: "setMode"; mode: FullscreenMode }
  | { kind: "forceExit" };

export interface ModeReply {
  mode: FullscreenMode;
}
