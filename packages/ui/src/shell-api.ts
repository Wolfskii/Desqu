import { invoke } from "@tauri-apps/api/core";

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
