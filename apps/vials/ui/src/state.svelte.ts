import type { ModuleStatus } from "@desqu/ui";
import { api, events, type UnlistenFn } from "./api";
import type { DesktopStatus, ItemInfo, MonitorInfo, SortRule, Vial, VialsConfig } from "./types";

const SAVE_DELAY_MS = 350;
const REFRESH_DELAY_MS = 250;

export function newId(prefix: string): string {
    const bytes = crypto.getRandomValues(new Uint8Array(5));
    return `${prefix}-${Array.from(bytes, (b) => b.toString(16).padStart(2, "0")).join("")}`;
}

class VialsState {
    moduleStatus = $state<ModuleStatus | null>(null);
    config = $state<VialsConfig | null>(null);
    items = $state<ItemInfo[]>([]);
    desktop = $state<DesktopStatus | null>(null);
    monitors = $state<MonitorInfo[]>([]);
    error = $state<string | null>(null);
    saveState = $state<"idle" | "pending" | "saving" | "saved">("idle");

    #saveTimer: ReturnType<typeof setTimeout> | null = null;
    #refreshTimer: ReturnType<typeof setTimeout> | null = null;
    #unlisten: UnlistenFn[] = [];

    get isRunning() {
        return this.moduleStatus === "on";
    }

    async init() {
        try {
            this.moduleStatus = await api.moduleStatus();
        } catch (e) {
            this.error = String(e);
            return;
        }
        if (!this.isRunning) return;
        await this.#load();
    }

    async turnOn() {
        try {
            await api.moduleStart();
            this.error = null;
            this.moduleStatus = await api.moduleStatus();
            await this.#load();
        } catch (e) {
            this.error = String(e);
            this.moduleStatus = await api.moduleStatus().catch(() => this.moduleStatus);
        }
    }

    async #load() {
        this.#unlisten.forEach((u) => u());
        try {
            const [config, items, desktop, monitors] = await Promise.all([
                api.getConfig(),
                api.listItems(),
                api.desktopStatus(),
                api.getMonitors(),
            ]);
            this.config = config;
            this.items = items;
            this.desktop = desktop;
            this.monitors = monitors;
        } catch (e) {
            this.error = String(e);
        }
        this.#unlisten = [await events.onChanged(() => this.#refreshSoon())];
    }

    dispose() {
        this.#unlisten.forEach((u) => u());
        this.#unlisten = [];
        if (this.#refreshTimer) clearTimeout(this.#refreshTimer);
    }

    /** Engine events come in bursts (a desktop change, then the delayed re-check): coalesce them. */
    #refreshSoon() {
        if (this.#refreshTimer) clearTimeout(this.#refreshTimer);
        this.#refreshTimer = setTimeout(() => void this.refresh(), REFRESH_DELAY_MS);
    }

    async refresh() {
        try {
            const [items, desktop] = await Promise.all([api.listItems(), api.desktopStatus()]);
            this.items = items;
            this.desktop = desktop;
            // Don't clobber edits the user is still making.
            if (this.saveState !== "pending" && this.saveState !== "saving") {
                this.config = await api.getConfig();
            }
            this.error = null;
        } catch (e) {
            this.error = String(e);
        }
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

    /** Items of one vial in its own order, including references whose item has gone. */
    itemsOf(vial: Vial): ItemInfo[] {
        const byId = new Map(this.items.map((i) => [i.id, i]));
        return vial.items.map(
            (id) =>
                byId.get(id) ?? {
                    id,
                    name: id.split(/[\\/]/).pop() ?? id,
                    kind: "file" as const,
                    vial: vial.id,
                    parked: false,
                    missing: true,
                },
        );
    }

    async assign(itemId: string, vialId: string | null) {
        try {
            this.config = await api.assignItem(itemId, vialId);
            await this.refresh();
        } catch (e) {
            this.error = String(e);
        }
    }

    async open(itemId: string) {
        try {
            await api.openItem(itemId);
        } catch (e) {
            this.error = String(e);
        }
    }

    addVial(): Vial | null {
        if (!this.config) return null;
        const n = this.config.vials.length;
        const vial: Vial = {
            id: newId("vial"),
            name: `Vial ${n + 1}`,
            monitor: null,
            rect: { x: Math.min(0.02 + 0.03 * n, 0.7), y: Math.min(0.05 + 0.04 * n, 0.6), width: 0.2, height: 0.35 },
            items: [],
            collapsed: false,
        };
        this.config.vials.push(vial);
        this.scheduleSave(true);
        return vial;
    }

    removeVial(id: string) {
        if (!this.config) return;
        this.config.vials = this.config.vials.filter((v) => v.id !== id);
        // A rule pointing at a deleted vial would make the whole config invalid.
        this.config.rules = this.config.rules.filter((r) => r.targetVial !== id);
        this.scheduleSave(true);
    }

    addRule(): SortRule | null {
        const target = this.config?.vials[0]?.id;
        if (!this.config || !target) return null;
        const rule: SortRule = {
            id: newId("rule"),
            name: "New rule",
            enabled: true,
            matcher: { type: "extension", value: "pdf" },
            targetVial: target,
        };
        this.config.rules.push(rule);
        this.scheduleSave(true);
        return rule;
    }

    removeRule(id: string) {
        if (!this.config) return;
        this.config.rules = this.config.rules.filter((r) => r.id !== id);
        this.scheduleSave(true);
    }

    moveRule(index: number, delta: number) {
        if (!this.config) return;
        const rules = this.config.rules;
        const target = index + delta;
        if (target < 0 || target >= rules.length) return;
        [rules[index], rules[target]] = [rules[target], rules[index]];
        this.scheduleSave(true);
    }
}

export const vials = new VialsState();
