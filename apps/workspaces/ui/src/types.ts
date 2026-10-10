// Mirrors the Rust types of workspaces-module (serde camelCase). Update both together.
import type { ModuleStatus } from "@desqu/ui";

export type { ModuleStatus };

export interface Workspace {
    /** The virtual desktop's id, `{XXXXXXXX-XXXX-XXXX-XXXX-XXXXXXXXXXXX}`. */
    desktopId: string;
    name: string;
    /** Desktop item ids that belong to this desktop. */
    items: string[];
}

export interface WorkspacesConfig {
    version: number;
    workspaces: Workspace[];
    /** New files belong to the desktop you are on. */
    autoAssignNew: boolean;
}

export type ItemKind = "shortcut" | "folder" | "app" | "file" | "special";

export interface DesktopEntry {
    id: string;
    name: string;
    current: boolean;
    itemCount: number;
    /** The workspace exists but its virtual desktop was removed. */
    removed: boolean;
}

export interface ItemInfo {
    id: string;
    name: string;
    kind: ItemKind;
    /** The virtual desktop that owns it; null = shown on every desktop. */
    owner: string | null;
    /** Currently hidden on this desktop. */
    hidden: boolean;
}

export interface DesktopStatus {
    running: boolean;
    currentDesktop: string | null;
    /** False when "Auto arrange icons" is on, so items cannot be hidden. */
    canHide: boolean | null;
    problem: string | null;
    hiddenCount: number;
}

export const KIND_LABELS: Record<ItemKind, string> = {
    shortcut: "Shortcut",
    folder: "Folder",
    app: "App",
    file: "File",
    special: "System item",
};
