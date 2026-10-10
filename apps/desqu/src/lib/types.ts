import type { ModuleStatus } from "@desqu/screenbound-ui";

export type { ModuleStatus };

// Mirrors the Rust `commands::ModuleEntry` (camelCase).
export interface ModuleEntry {
    id: string;
    name: string;
    description: string;
    version: string;
    /** False on operating systems without a backend for this module. */
    supported: boolean;
    enabled: boolean;
    startWithSystem: boolean;
    status: ModuleStatus;
    /** The module's standalone app is installed on this PC. */
    standaloneInstalled: boolean;
}

export type TriState = "on" | "off" | "mixed";

export function statusText(status: ModuleStatus): string {
    switch (status) {
        case "on":
            return "On";
        case "paused":
            return "Paused";
        case "off":
            return "Off";
        case "ownedElsewhere":
            return "Running standalone";
        case "unsupported":
            return "Not available on this OS";
    }
}
