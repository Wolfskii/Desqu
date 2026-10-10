import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type { ModuleStatus } from "@desqu/ui";
import type { DesktopStatus, ItemInfo, MonitorInfo, VialsConfig } from "./types";

const PLUGIN = "plugin:vials-module";

export const api = {
    getConfig: () => invoke<VialsConfig>(`${PLUGIN}|get_config`),
    setConfig: (config: VialsConfig) => invoke<VialsConfig>(`${PLUGIN}|set_config`, { config }),
    listItems: () => invoke<ItemInfo[]>(`${PLUGIN}|list_items`),
    itemIcon: (id: string, size = 32) => invoke<string>(`${PLUGIN}|item_icon`, { id, size }),
    assignItem: (itemId: string, vialId: string | null) =>
        invoke<VialsConfig>(`${PLUGIN}|assign_item`, { itemId, vialId }),
    openItem: (id: string) => invoke<void>(`${PLUGIN}|open_item`, { id }),
    createVialFromMenu: () => invoke<string>(`${PLUGIN}|create_vial_from_menu`),
    dismissMenu: () => invoke<void>(`${PLUGIN}|dismiss_menu`),
    dropItems: (vialId: string, paths: string[]) => invoke<VialsConfig>(`${PLUGIN}|drop_items`, { vialId, paths }),
    getMonitors: () => invoke<MonitorInfo[]>(`${PLUGIN}|get_monitors`),
    desktopStatus: () => invoke<DesktopStatus>(`${PLUGIN}|desktop_status`),
    moduleStart: () => invoke<void>(`${PLUGIN}|module_start`),
    moduleStop: () => invoke<void>(`${PLUGIN}|module_stop`),
    moduleStatus: () => invoke<ModuleStatus>(`${PLUGIN}|module_status`),
};

export const events = {
    /** The engine changed something (desktop change, auto-sort, a vial window moved). */
    onChanged: (cb: () => void) => listen("vials://changed", () => cb()),
};

export type { UnlistenFn };
