import type { ModuleStatus } from "@desqu/ui";
import { api, events, type UnlistenFn } from "./api";
import type { DesktopEntry, DesktopStatus, ItemInfo, RestoreReport, Workspace, WorkspacesConfig } from "./types";

const SAVE_DELAY_MS = 350;
const REFRESH_DELAY_MS = 250;

class WorkspacesState {
    moduleStatus = $state<ModuleStatus | null>(null);
    config = $state<WorkspacesConfig | null>(null);
    desktops = $state<DesktopEntry[]>([]);
    items = $state<ItemInfo[]>([]);
    desktop = $state<DesktopStatus | null>(null);
    error = $state<string | null>(null);
    saveState = $state<"idle" | "pending" | "saving" | "saved">("idle");
    /** Result of the last restore, per desktop id. */
    reports = $state<Record<string, RestoreReport>>({});
    busy = $state<string | null>(null);

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
        await this.refresh(true);
        this.#unlisten = [await events.onChanged(() => this.#refreshSoon())];
    }

    dispose() {
        this.#unlisten.forEach((u) => u());
        this.#unlisten = [];
        if (this.#refreshTimer) clearTimeout(this.#refreshTimer);
    }

    /** Engine events come in bursts (a switch, then the delayed re-check): coalesce them. */
    #refreshSoon() {
        if (this.#refreshTimer) clearTimeout(this.#refreshTimer);
        this.#refreshTimer = setTimeout(() => void this.refresh(), REFRESH_DELAY_MS);
    }

    async refresh(first = false) {
        try {
            const [desktops, items, desktop] = await Promise.all([
                api.listDesktops(),
                api.listItems(),
                api.desktopStatus(),
            ]);
            this.desktops = desktops;
            this.items = items;
            this.desktop = desktop;
            // Don't clobber edits the user is still making.
            if (first || (this.saveState !== "pending" && this.saveState !== "saving")) {
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
            this.desktops = await api.listDesktops();
        } catch (e) {
            this.error = String(e);
            this.saveState = "idle";
        }
    }

    workspace(desktopId: string): Workspace | undefined {
        return this.config?.workspaces.find((w) => w.desktopId === desktopId);
    }

    /** Creates the workspace of a desktop (empty) so it can be edited. */
    ensureWorkspace(entry: DesktopEntry): Workspace | null {
        if (!this.config) return null;
        let ws = this.workspace(entry.id);
        if (!ws) {
            this.config.workspaces.push({
                desktopId: entry.id,
                name: entry.name,
                manageIcons: false,
                hiddenItems: [],
                apps: [],
                folders: [],
            });
            ws = this.workspace(entry.id)!;
            this.scheduleSave(true);
        }
        return ws;
    }

    removeWorkspace(desktopId: string) {
        if (!this.config) return;
        this.config.workspaces = this.config.workspaces.filter((w) => w.desktopId !== desktopId);
        this.scheduleSave(true);
    }

    async capture(entry: DesktopEntry) {
        this.busy = entry.id;
        try {
            this.config = await api.captureWorkspace(entry.id);
            this.desktops = await api.listDesktops();
            this.error = null;
        } catch (e) {
            this.error = String(e);
        } finally {
            this.busy = null;
        }
    }

    async restore(entry: DesktopEntry) {
        this.busy = entry.id;
        try {
            this.reports[entry.id] = await api.restoreWorkspace(entry.id);
            this.error = null;
        } catch (e) {
            this.error = String(e);
        } finally {
            this.busy = null;
        }
    }
}

export const workspaces = new WorkspacesState();
