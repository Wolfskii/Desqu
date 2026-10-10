import { api, events, type UnlistenFn } from "./api";
import type { ActivityEntry, AppConfig, EngineStatus, LegacyStatus, ModuleStatus, MonitorInfo, Zone } from "./types";

const SAVE_DELAY_MS = 350;
const ACTIVITY_LIMIT = 300;

export function newId(prefix: string): string {
    const bytes = crypto.getRandomValues(new Uint8Array(6));
    return `${prefix}-${Array.from(bytes, (b) => b.toString(16).padStart(2, "0")).join("")}`;
}

class AppState {
    moduleStatus = $state<ModuleStatus | null>(null);
    legacy = $state<LegacyStatus | null>(null);
    monitors = $state<MonitorInfo[]>([]);
    config = $state<AppConfig | null>(null);
    status = $state<EngineStatus | null>(null);
    activity = $state<ActivityEntry[]>([]);
    error = $state<string | null>(null);
    saveState = $state<"idle" | "pending" | "saving" | "saved">("idle");
    selectedMonitorId = $state<string | null>(null);
    selectedZoneId = $state<string | null>(null);

    #saveTimer: ReturnType<typeof setTimeout> | null = null;
    #unlisten: UnlistenFn[] = [];

    selectedMonitor = $derived(
        this.monitors.find((m) => m.id === this.selectedMonitorId) ??
        this.monitors.find((m) => m.isPrimary) ??
        this.monitors[0] ??
        null,
    );

    selectedZone = $derived(this.config?.zones.find((z) => z.id === this.selectedZoneId) ?? this.config?.zones[0] ?? null);

    async init() {
        try {
            this.moduleStatus = await api.moduleStatus();
        } catch (e) {
            this.error = String(e);
            return;
        }
        await this.refreshLegacy();
        if (!this.isRunning) return;
        await this.#load();
    }

    async refreshLegacy() {
        try {
            this.legacy = await api.legacyStatus();
        } catch (e) {
            this.error = String(e);
        }
    }

    async uninstallOld() {
        try {
            await api.legacyUninstallOld();
        } catch (e) {
            this.error = String(e);
        }
    }

    async deleteSettingsBackup() {
        try {
            await api.legacyDeleteSettingsBackup();
            await this.refreshLegacy();
        } catch (e) {
            this.error = String(e);
        }
    }

    /** `on` or `paused`: the engine runs in this process, so the tabs can load. */
    get isRunning() {
        return this.moduleStatus === "on" || this.moduleStatus === "paused";
    }

    async turnOn() {
        try {
            await api.moduleStart();
            this.error = null;
            this.moduleStatus = await api.moduleStatus();
            await this.#load();
        } catch (e) {
            this.error = String(e);
            // The module may now be `ownedElsewhere`; show that instead of a stale `off`.
            this.moduleStatus = await api.moduleStatus().catch(() => this.moduleStatus);
            await this.refreshLegacy();
        }
    }

    async #load() {
        this.#unlisten.forEach((u) => u());
        try {
            const [monitors, config, status, activity] = await Promise.all([
                api.getMonitors(),
                api.getConfig(),
                api.getStatus(),
                api.getActivity(),
            ]);
            this.monitors = monitors;
            this.config = config;
            this.status = status;
            this.activity = activity.reverse();
        } catch (e) {
            this.error = String(e);
        }

        this.#unlisten = await Promise.all([
            events.onActivity((entry) => {
                this.activity = [entry, ...this.activity].slice(0, ACTIVITY_LIMIT);
            }),
            events.onManagedChanged(() => void this.refreshStatus()),
            events.onMonitorsChanged((m) => (this.monitors = m)),
            events.onConfigChanged((c) => {
                // Don't clobber edits the user is still making.
                if (this.saveState !== "pending" && this.saveState !== "saving") this.config = c;
            }),
        ]);
    }

    dispose() {
        this.#unlisten.forEach((u) => u());
    }

    async refreshStatus() {
        try {
            this.status = await api.getStatus();
        } catch (e) {
            this.error = String(e);
        }
    }

    async refreshMonitors() {
        this.monitors = await api.getMonitors();
    }

    /** Call after mutating `config` in place; persists after a short debounce. */
    scheduleSave(immediate = false) {
        this.saveState = "pending";
        if (this.#saveTimer) clearTimeout(this.#saveTimer);
        this.#saveTimer = setTimeout(() => void this.#save(), immediate ? 0 : SAVE_DELAY_MS);
    }

    async #save() {
        if (!this.config) return;
        this.saveState = "saving";
        try {
            this.config = await api.setConfig($state.snapshot(this.config));
            this.error = null;
            this.saveState = "saved";
        } catch (e) {
            this.error = String(e);
            this.saveState = "idle";
        }
    }

    async setEnabled(enabled: boolean) {
        try {
            this.config = await api.setEnabled(enabled);
            await this.refreshStatus();
        } catch (e) {
            this.error = String(e);
        }
    }

    addZone(): Zone | null {
        if (!this.config) return null;
        const zone: Zone = {
            id: newId("zone"),
            name: `Zone ${this.config.zones.length + 1}`,
            rect: { x: 0, y: 0, width: 0.5, height: 1 },
            unit: "percent",
            pixels: null,
            monitor: null,
            reference: "monitor",
        };
        this.config.zones.push(zone);
        this.selectedZoneId = zone.id;
        this.scheduleSave(true);
        return zone;
    }

    zoneInUse(zoneId: string): boolean {
        return !!this.config?.rules.some((r) =>
            r.actions.some(
                (a) =>
                    (a.type === "fullscreenZone" && a.zoneId === zoneId) ||
                    (a.type === "borderless" && a.target.type === "zone" && a.target.zoneId === zoneId),
            ),
        );
    }

    deleteZone(zoneId: string) {
        if (!this.config || this.zoneInUse(zoneId)) return;
        this.config.zones = this.config.zones.filter((z) => z.id !== zoneId);
        if (this.selectedZoneId === zoneId) this.selectedZoneId = this.config.zones[0]?.id ?? null;
        this.scheduleSave(true);
    }
}

export const app = new AppState();
