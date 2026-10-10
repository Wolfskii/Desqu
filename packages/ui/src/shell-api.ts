import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

// Mirrors desqu-shell's serde types (crates/desqu-shell/src/{settings,plugin}.rs).
export interface UpdateSettings {
    checkAutomatically: boolean;
    installAutomatically: boolean;
}

export interface ShellSettings {
    version: number;
    startWithSystem: boolean;
    startMinimized: boolean;
    closeToTray: boolean;
    updates: UpdateSettings;
}

export interface AppInfo {
    name: string;
    version: string;
    identifier: string;
    /** No installer footprint next to the exe: do not offer to install updates. */
    portable: boolean;
}

const PLUGIN = "plugin:desqu-shell";

export const shellApi = {
    getShellSettings: () => invoke<ShellSettings>(`${PLUGIN}|get_shell_settings`),
    setShellSettings: (settings: ShellSettings) => invoke<ShellSettings>(`${PLUGIN}|set_shell_settings`, { settings }),
    appInfo: () => invoke<AppInfo>(`${PLUGIN}|app_info`),
};

export interface UpdateInfo {
    version: string;
    notes: string | null;
    /** Publish date, unix seconds. */
    date: number | null;
}

export interface UpdateStatus {
    /** False when this build has no updater configuration. */
    enabled: boolean;
    available: UpdateInfo | null;
    /** Unix seconds of the last completed check. */
    lastCheck: number | null;
    lastError: string | null;
    releasesUrl: string;
}

export const updateApi = {
    status: () => invoke<UpdateStatus>(`${PLUGIN}|update_status`),
    check: () => invoke<UpdateInfo | null>(`${PLUGIN}|check_update`),
    install: () => invoke<void>(`${PLUGIN}|install_update`),
    openReleasesPage: () => invoke<void>(`${PLUGIN}|open_releases_page`),
    onAvailable: (cb: (info: UpdateInfo) => void): Promise<UnlistenFn> =>
        listen<UpdateInfo>("desqu://update-available", (e) => cb(e.payload)),
};
