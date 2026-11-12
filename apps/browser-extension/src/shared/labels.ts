// Plain-language names for the modes. Shown in the popup and options. Keep wording in sync with PLAN.md §7.
import type { FullscreenMode } from "./rules";

export const MODE_LABELS: Record<FullscreenMode, { name: string; hint: string }> = {
  native: {
    name: "Browser default",
    hint: "The browser makes the page fullscreen. Desqu still limits it to your zone, with a short flash.",
  },
  fullscreen: {
    name: "Real fullscreen",
    hint: "The browser makes the page fullscreen. Desqu leaves it alone, so there is no flash.",
  },
  window: {
    name: "In window",
    hint: "The video fills the browser tab. The browser window does not change. Works without Desqu.",
  },
  zone: {
    name: "Desqu zone",
    hint: "The browser window moves into your Desqu zone and the video fills it. Needs Desqu running.",
  },
};

export const LIMITATIONS: string[] = [
  "Fullscreen from the browser itself (F11 or the browser menu) is not a website request, so the extension cannot change it. Desqu still limits it as usual.",
  "In \"In window\" and \"Desqu zone\" modes the page is not really fullscreen. A few sites style their player with fullscreen-only CSS and may look slightly different.",
  "Does not run on browser pages (settings, extension stores, the built-in PDF viewer). Private windows need \"Allow in private/incognito\" turned on.",
  "\"Desqu zone\" needs the Desqu (or Screenbound) app running on Windows. Without it, the extension shows the video in the window instead.",
  "Safari is not supported: Desqu runs on Windows only.",
];
