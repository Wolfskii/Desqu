// Mirrors the serde types in src-tauri/src/core and src-tauri/src/engine.

export interface Rect {
    left: number;
    top: number;
    right: number;
    bottom: number;
}

export interface MonitorInfo {
    id: string;
    deviceName: string;
    friendlyName: string;
    number: number;
    isPrimary: boolean;
    bounds: Rect;
    workArea: Rect;
    dpi: number;
}

export interface NormalizedRect {
    x: number;
    y: number;
    width: number;
    height: number;
}

/** Physical pixels, offset from the reference rect's top-left corner. */
export interface PixelRect {
    x: number;
    y: number;
    width: number;
    height: number;
}

export type ZoneReference = "monitor" | "workArea";
export type ZoneUnit = "percent" | "pixels";

export interface Zone {
    id: string;
    name: string;
    rect: NormalizedRect;
    unit: ZoneUnit;
    pixels: PixelRect | null;
    monitor: string | null;
    reference: ZoneReference;
}

export type ChromeMode = "keep" | "hide";

export type Matcher =
    | { type: "processName"; value: string }
    | { type: "executablePath"; value: string }
    | { type: "windowClass"; value: string }
    | { type: "titleContains"; value: string }
    | { type: "any"; value: Matcher[] }
    | { type: "all"; value: Matcher[] };

export type BorderlessTarget = { type: "monitor" } | { type: "zone"; zoneId: string };

export type Action =
    | {
          type: "fullscreenZone";
          zoneId: string;
          chrome: ChromeMode;
          /** Fill the FancyZones zone a snapped window was in. Omitted on older rules, which the engine treats as true. */
          useSnappedZone?: boolean;
      }
    | {
          /** Make a windowed (framed) window borderless and fill `target`, e.g. a game running in a window. */
          type: "borderless";
          target: BorderlessTarget;
          /** Keep above other windows while borderless. Omitted = off. */
          topmost?: boolean;
      };

export type RuleScope =
    | { type: "all" }
    | { type: "groups"; groupIds: string[] }
    | { type: "apps"; processNames: string[] };

export interface WindowRule {
    id: string;
    name: string;
    enabled: boolean;
    matcher: Matcher;
    /** When set, the rule applies to turned-on apps instead of `matcher`. */
    scope?: RuleScope | null;
    actions: Action[];
}

export interface AppGroup {
    id: string;
    name: string;
    color: string;
    icon: string;
}

export interface KnownApp {
    processName: string;
    title: string;
    enabled?: boolean;
    groupId?: string | null;
}

export type TransitionSpeed = "instant" | "fast" | "normal" | "slow";

export interface AppConfig {
    version: number;
    enabled: boolean;
    /** Omitted in configs written before this field existed; the engine treats that as "normal". */
    transition?: TransitionSpeed;
    zones: Zone[];
    rules: WindowRule[];
    groups?: AppGroup[];
    /** Apps seen on this PC. Omitted in configs written before this field existed. */
    knownApps?: KnownApp[];
}

export type ShowState = "normal" | "minimized" | "maximized";

export interface WindowState {
    bounds: Rect;
    visibleBounds: Rect;
    restoreBounds: Rect;
    showState: ShowState;
    visible: boolean;
    cloaked: boolean;
    hasTitleBar: boolean;
    hasResizeFrame: boolean;
    topmost: boolean;
    style: { primary: number; extended: number };
    monitor: string | null;
    dpi: number;
    /** Snapped into a PowerToys FancyZones zone. */
    zoneSnapped?: boolean;
    /** Click-through overlay, not a window to constrain. */
    clickThrough?: boolean;
}

export interface WindowIdentity {
    process: { pid: number; startTime: number };
    processName: string;
    executablePath: string;
    className: string;
    title: string;
}

export interface WindowInfo {
    id: number;
    idHex: string;
    identity: WindowIdentity;
    state: WindowState;
    monitorName: string | null;
    fullscreenLike: boolean;
    hung: boolean;
    managed: boolean;
    matchedRule: string | null;
}

export interface ManagedSummary {
    /** `fullscreen`: a fullscreen app held in a zone; `borderless`: a windowed app made borderless. */
    kind: "fullscreen" | "borderless";
    id: number;
    idHex: string;
    processName: string;
    title: string;
    ruleId: string;
    zoneId: string;
    chrome: ChromeMode;
    monitor: string | null;
    target: Rect;
    detectedBounds: Rect;
    preFullscreenBounds: Rect | null;
    gaveUp: boolean;
    sinceMs: number;
}

export interface EngineStatus {
    enabled: boolean;
    eventsActive: boolean;
    managed: ManagedSummary[];
}

export interface ActivityEntry {
    timestampMs: number;
    level: "info" | "warn" | "error";
    window: string | null;
    message: string;
}

export const rectW = (r: Rect) => r.right - r.left;
export const rectH = (r: Rect) => r.bottom - r.top;
export const fmtRect = (r: Rect) => `(${r.left}, ${r.top}) ${rectW(r)}×${rectH(r)}`;

/**
 * `off`: nothing running; `paused`: running but `enabled` is false; `on`: constraining windows;
 * `ownedElsewhere`: another process (standalone app or Desqu) runs the engine; `unsupported`: no OS backend.
 */
export type ModuleStatus = "off" | "paused" | "on" | "unsupported" | "ownedElsewhere";

export interface OldInstall {
    version: string | null;
    uninstallString: string;
}

/** Leftovers of the pre-rebrand Screenbound (mirrors the module's `legacy::LegacyStatus`). */
export interface LegacyStatus {
    /** Another screenbound.exe (not this process) is running. */
    olderRunning: boolean;
    oldInstall: OldInstall | null;
    /** Old settings folder kept as a backup after migration. */
    settingsBackup: string | null;
}
