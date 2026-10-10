import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type { ModuleStatus } from "@desqu/ui";
import type { DesktopEntry, DesktopStatus, ItemInfo, WorkspacesConfig } from "./types";

const PLUGIN = "plugin:workspaces-module";

export const api = {
    getConfig: () => invoke<WorkspacesConfig>(`${PLUGIN}|get_config`),
    setConfig: (config: WorkspacesConfig) => invoke<WorkspacesConfig>(`${PLUGIN}|set_config`, { config }),
    listDesktops: () => invoke<DesktopEntry[]>(`${PLUGIN}|list_desktops`),
    listItems: () => invoke<ItemInfo[]>(`${PLUGIN}|list_items`),
    itemIcon: (id: string, size = 32) => invoke<string>(`${PLUGIN}|item_icon`, { id, size }),
    assignItem: (itemId: string, desktopId: string | null) =>
        invoke<WorkspacesConfig>(`${PLUGIN}|assign_item`, { itemId, desktopId }),
    assignAllItems: (desktopId: string | null) =>
        invoke<WorkspacesConfig>(`${PLUGIN}|assign_all_items`, { desktopId }),
    desktopStatus: () => invoke<DesktopStatus>(`${PLUGIN}|desktop_status`),
    moduleStart: () => invoke<void>(`${PLUGIN}|module_start`),
    moduleStop: () => invoke<void>(`${PLUGIN}|module_stop`),
    moduleStatus: () => invoke<ModuleStatus>(`${PLUGIN}|module_status`),
};

export const events = {
    /** The engine changed something (desktop switch, icons hidden or shown, a new file assigned). */
    onChanged: (cb: () => void) => listen("workspaces://changed", () => cb()),
};

export type { UnlistenFn };
