import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type { ActivityEntry, AppConfig, EngineStatus, ModuleStatus, MonitorInfo, WindowInfo } from "./types";

export const api = {
    getMonitors: () => invoke<MonitorInfo[]>("plugin:screenbound-module|get_monitors"),
    getConfig: () => invoke<AppConfig>("plugin:screenbound-module|get_config"),
    setConfig: (config: AppConfig) => invoke<AppConfig>("plugin:screenbound-module|set_config", { config }),
    setEnabled: (enabled: boolean) => invoke<AppConfig>("plugin:screenbound-module|set_enabled", { enabled }),
    getStatus: () => invoke<EngineStatus>("plugin:screenbound-module|get_status"),
    listWindows: () => invoke<WindowInfo[]>("plugin:screenbound-module|list_windows"),
    getActivity: () => invoke<ActivityEntry[]>("plugin:screenbound-module|get_activity"),
    releaseWindow: (id: number) => invoke<void>("plugin:screenbound-module|release_window", { id }),
    releaseAll: () => invoke<void>("plugin:screenbound-module|release_all"),
    moduleStart: () => invoke<void>("plugin:screenbound-module|module_start"),
    moduleStop: () => invoke<void>("plugin:screenbound-module|module_stop"),
    moduleStatus: () => invoke<ModuleStatus>("plugin:screenbound-module|module_status"),
};

export const events = {
    onActivity: (cb: (e: ActivityEntry) => void) => listen<ActivityEntry>("sb://activity", (e) => cb(e.payload)),
    onManagedChanged: (cb: () => void) => listen("sb://managed-changed", () => cb()),
    onMonitorsChanged: (cb: (m: MonitorInfo[]) => void) =>
        listen<MonitorInfo[]>("sb://monitors-changed", (e) => cb(e.payload)),
    onConfigChanged: (cb: (c: AppConfig) => void) => listen<AppConfig>("sb://config-changed", (e) => cb(e.payload)),
};

export type { UnlistenFn };
