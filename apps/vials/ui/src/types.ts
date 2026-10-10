// Mirrors the Rust types of vials-module (serde camelCase). Update both together.
import type { ModuleStatus } from "@desqu/ui";

export type { ModuleStatus };

export interface NormalizedRect {
    x: number;
    y: number;
    width: number;
    height: number;
}

export interface Vial {
    id: string;
    name: string;
    /** Monitor name; null/absent = the primary monitor. */
    monitor?: string | null;
    rect: NormalizedRect;
    /** Desktop item ids (absolute parsing names), in display order. */
    items: string[];
    collapsed?: boolean;
}

export type ItemKind = "shortcut" | "folder" | "app" | "file" | "special";

export type ItemMatcher =
    | { type: "extension"; value: string }
    | { type: "kind"; value: ItemKind }
    | { type: "nameGlob"; value: string }
    | { type: "any"; value: ItemMatcher[] }
    | { type: "all"; value: ItemMatcher[] };

export interface SortRule {
    id: string;
    name: string;
    enabled: boolean;
    matcher: ItemMatcher;
    /** The vial a matching item goes into. */
    targetVial: string;
}

export type VialTheme = "glass" | "classic";

export interface VialsConfig {
    version: number;
    vials: Vial[];
    rules: SortRule[];
    autoSort: boolean;
    hideOnDesktop: boolean;
    /** Dragging a box on the empty desktop offers to create a vial there. */
    createOnDrag: boolean;
    /** "Create vial here" in the desktop's right-click menu. */
    contextMenu: boolean;
    /** How vial windows look. */
    theme: VialTheme;
    /** Items the user took out of a vial on purpose; rules never put them back. */
    declined?: string[];
}

export interface ItemInfo {
    id: string;
    name: string;
    kind: ItemKind;
    /** The vial holding it, if any. */
    vial: string | null;
    /** Currently hidden on the desktop. */
    parked: boolean;
    /** In a vial but no longer on the desktop. */
    missing: boolean;
}

export interface MonitorInfo {
    name: string;
    left: number;
    top: number;
    right: number;
    bottom: number;
    primary: boolean;
}

export interface DesktopStatus {
    running: boolean;
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
