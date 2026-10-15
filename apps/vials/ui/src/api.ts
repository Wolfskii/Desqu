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
    menuReady: () => invoke<void>(`${PLUGIN}|menu_ready`),
    beginRename: (vialId: string) => invoke<void>(`${PLUGIN}|begin_rename`, { vialId }),
    endRename: (vialId: string) => invoke<void>(`${PLUGIN}|end_rename`, { vialId }),
    dismissMenu: () => invoke<void>(`${PLUGIN}|dismiss_menu`),
    dropItems: (vialId: string, paths: string[]) => invoke<VialsConfig>(`${PLUGIN}|drop_items`, { vialId, paths }),
    dragItemsOut: (itemIds: string[]) => invoke<void>(`${PLUGIN}|drag_items_out`, { itemIds }),
    showItemMenu: (itemId: string) => invoke<void>(`${PLUGIN}|show_item_menu`, { itemId }),
    openRules: (vialId: string | null) => invoke<void>(`${PLUGIN}|open_rules`, { vialId }),
    openVialMenu: (vialId: string) => invoke<void>(`${PLUGIN}|open_vial_menu`, { vialId }),
    moveIntoFolder: (folderId: string, paths: string[]) =>
        invoke<number>(`${PLUGIN}|move_into_folder`, { folderId, paths }),
    setVialFoldersFirst: (vialId: string, on: boolean) =>
        invoke<void>(`${PLUGIN}|set_vial_folders_first`, { vialId, on }),
    sortVialItems: (vialId: string, key: string, descending: boolean) =>
        invoke<void>(`${PLUGIN}|sort_vial_items`, { vialId, key, descending }),
    copyItems: (itemIds: string[], cut: boolean) => invoke<void>(`${PLUGIN}|copy_items`, { itemIds, cut }),
    pasteIntoVial: (vialId: string) => invoke<number>(`${PLUGIN}|paste_into_vial`, { vialId }),
    focusVial: (vialId: string) => invoke<void>(`${PLUGIN}|focus_vial`, { vialId }),
    getMonitors: () => invoke<MonitorInfo[]>(`${PLUGIN}|get_monitors`),
    desktopStatus: () => invoke<DesktopStatus>(`${PLUGIN}|desktop_status`),
    moduleStart: () => invoke<void>(`${PLUGIN}|module_start`),
    moduleStop: () => invoke<void>(`${PLUGIN}|module_stop`),
    moduleStatus: () => invoke<ModuleStatus>(`${PLUGIN}|module_status`),
};

export const events = {
    /** The engine changed something (desktop change, auto-sort, a vial window moved). */
    onChanged: (cb: () => void) => listen("vials://changed", () => cb()),
    /** The rules window was asked for a new rule (for this vial, or none). */
    onRulesOpen: (cb: (vialId: string | null) => void) =>
        listen<string | null>("vials://rules-open", (e) => cb(e.payload)),
};

export type { UnlistenFn };
