// Mirrors the Rust types of workspaces-module (serde camelCase). Update both together.
import type { ModuleStatus } from "@desqu/ui";

export type { ModuleStatus };

export interface AppLaunch {
    /** Absolute path of the executable. */
    exe: string;
    args?: string | null;
    /** Shown in the UI; empty = the file name. */
    name: string;
}

export interface Workspace {
    /** The virtual desktop's id, `{XXXXXXXX-XXXX-XXXX-XXXX-XXXXXXXXXXXX}`. */
    desktopId: string;
    name: string;
    /** Hide `hiddenItems` on the desktop while this virtual desktop is current. */
    manageIcons: boolean;
    hiddenItems: string[];
    apps: AppLaunch[];
    folders: string[];
}

export interface WorkspacesConfig {
    version: number;
    workspaces: Workspace[];
    /** Leaving a desktop refreshes its saved apps and folders. */
    autoSave: boolean;
}

export type ItemKind = "shortcut" | "folder" | "app" | "file" | "special";

export interface DesktopEntry {
    id: string;
    name: string;
    current: boolean;
    hasWorkspace: boolean;
    /** The workspace exists but its virtual desktop was removed. */
    removed: boolean;
}

export interface ItemInfo {
    id: string;
    name: string;
    kind: ItemKind;
}

export interface DesktopStatus {
    running: boolean;
    currentDesktop: string | null;
    /** False when "Auto arrange icons" is on, so items cannot be hidden. */
    canHide: boolean | null;
    problem: string | null;
    hiddenCount: number;
}

export interface RestoreReport {
    launched: string[];
    alreadyOpen: string[];
    failed: string[];
}
