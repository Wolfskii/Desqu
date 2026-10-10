import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type { ModuleEntry } from "./types";

export const api = {
    listModules: () => invoke<ModuleEntry[]>("list_modules"),
    setModuleEnabled: (id: string, enabled: boolean) => invoke<ModuleEntry>("set_module_enabled", { id, enabled }),
    setModuleStartWithSystem: (id: string, value: boolean) =>
        invoke<ModuleEntry>("set_module_start_with_system", { id, value }),
    setAllStartWithSystem: (value: boolean) => invoke<ModuleEntry[]>("set_all_start_with_system", { value }),
    takeoverModule: (id: string) => invoke<ModuleEntry>("takeover_module", { id }),
    resetModuleSettings: (id: string) => invoke<ModuleEntry>("reset_module_settings", { id }),
    removeStandalone: (id: string) => invoke<void>("remove_standalone", { id }),
};

export const events = {
    onModulesChanged: (cb: () => void) => listen("desqu://modules-changed", () => cb()),
    onTakeoverNeeded: (cb: (id: string) => void) => listen<string>("desqu://takeover-needed", (e) => cb(e.payload)),
};

export type { UnlistenFn };
