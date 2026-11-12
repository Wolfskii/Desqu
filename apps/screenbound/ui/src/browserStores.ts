// Where each browser's extension lives. Keep in sync with apps/browser-extension/extension-ids.json
// (storeLinks). `null` means not published yet: the UI then shows the manual-install note instead.
export interface BrowserInstall {
    name: string;
    /** Which store or manual route this browser uses. */
    route: "chrome" | "edge" | "firefox" | "opera";
    /** Set once the store listing exists. */
    url: string | null;
}

export const BROWSER_INSTALLS: BrowserInstall[] = [
    { name: "Chrome", route: "chrome", url: null },
    { name: "Edge", route: "edge", url: null },
    { name: "Brave", route: "chrome", url: null },
    { name: "Opera", route: "opera", url: null },
    { name: "Vivaldi", route: "chrome", url: null },
    { name: "Firefox", route: "firefox", url: null },
    { name: "LibreWolf", route: "firefox", url: null },
];

/** Not offered at all: Desqu runs on Windows only. */
export const UNSUPPORTED_BROWSERS = ["Safari"];
