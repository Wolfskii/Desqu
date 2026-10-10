import { api, events, type UnlistenFn } from "./api";
import type { ModuleEntry, TriState } from "./types";

class DesquApp {
    modules = $state<ModuleEntry[]>([]);
    loaded = $state(false);
    error = $state<string | null>(null);
    /** Ids with an action in flight, so their switches can be disabled meanwhile. */
    busy = $state<string[]>([]);

    #unlisten: UnlistenFn[] = [];

    supported = $derived(this.modules.filter((m) => m.supported));

    /** State of the "start all apps with system" master switch. */
    allStart = $derived.by<TriState>(() => {
        const list = this.supported;
        const on = list.filter((m) => m.startWithSystem).length;
        if (list.length === 0 || on === 0) return "off";
        return on === list.length ? "on" : "mixed";
    });

    /** Modules the user turned on that are currently run by their standalone app. */
    takeoverPending = $derived(this.modules.filter((m) => m.enabled && m.status === "ownedElsewhere"));

    async init() {
        await this.refresh();
        this.#unlisten = await Promise.all([
            events.onModulesChanged(() => void this.refresh()),
            events.onTakeoverNeeded(() => void this.refresh()),
        ]);
    }

    dispose() {
        this.#unlisten.forEach((u) => u());
    }

    async refresh() {
        try {
            this.modules = await api.listModules();
            this.error = null;
        } catch (e) {
            this.error = String(e);
        } finally {
            this.loaded = true;
        }
    }

    isBusy(id: string) {
        return this.busy.includes(id);
    }

    async #run<T>(id: string, action: () => Promise<T>): Promise<T | undefined> {
        this.busy = [...this.busy, id];
        try {
            const result = await action();
            this.error = null;
            return result;
        } catch (e) {
            this.error = String(e);
            return undefined;
        } finally {
            this.busy = this.busy.filter((b) => b !== id);
            await this.refresh();
        }
    }

    setEnabled(id: string, enabled: boolean) {
        return this.#run(id, () => api.setModuleEnabled(id, enabled));
    }

    setStartWithSystem(id: string, value: boolean) {
        return this.#run(id, () => api.setModuleStartWithSystem(id, value));
    }

    async setAllStartWithSystem(value: boolean) {
        try {
            this.modules = await api.setAllStartWithSystem(value);
            this.error = null;
        } catch (e) {
            this.error = String(e);
        }
    }

    takeover(id: string) {
        return this.#run(id, () => api.takeoverModule(id));
    }

    reset(id: string) {
        return this.#run(id, () => api.resetModuleSettings(id));
    }

    removeStandalone(id: string) {
        return this.#run(id, () => api.removeStandalone(id));
    }
}

export const desqu = new DesquApp();
