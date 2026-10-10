import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type { ModuleStatus } from "@desqu/ui";
import type { DesktopEntry, DesktopStatus, ItemInfo, RestoreReport, WorkspacesConfig } from "./types";

const PLUGIN = "plugin:workspaces-module";

export const api = {
    getConfig: () => invoke<WorkspacesConfig>(`${PLUGIN}|get_config`),
    setConfig: (config: WorkspacesConfig) => invoke<WorkspacesConfig>(`${PLUGIN}|set_config`, { config }),
    listDesktops: () => invoke<DesktopEntry[]>(`${PLUGIN}|list_desktops`),
    listItems: () => invoke<ItemInfo[]>(`${PLUGIN}|list_items`),
    itemIcon: (id: string, size = 32) => invoke<string>(`${PLUGIN}|item_icon`, { id, size }),
    captureWorkspace: (desktopId: string) => invoke<WorkspacesConfig>(`${PLUGIN}|capture_workspace`, { desktopId }),
    restoreWorkspace: (desktopId: string) => invoke<RestoreReport>(`${PLUGIN}|restore_workspace`, { desktopId }),
    desktopStatus: () => invoke<DesktopStatus>(`${PLUGIN}|desktop_status`),
    moduleStart: () => invoke<void>(`${PLUGIN}|module_start`),
    moduleStop: () => invoke<void>(`${PLUGIN}|module_stop`),
    moduleStatus: () => invoke<ModuleStatus>(`${PLUGIN}|module_status`),
};

export const events = {
    /** The engine changed something (desktop switch, icons hidden or shown). */
    onChanged: (cb: () => void) => listen("workspaces://changed", () => cb()),
};

export type { UnlistenFn };
