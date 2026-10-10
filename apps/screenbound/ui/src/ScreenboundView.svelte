<script lang="ts">
  import { onDestroy, onMount } from "svelte";
  import AppsPanel from "./components/AppsPanel.svelte";
  import DiagnosticsPanel from "./components/DiagnosticsPanel.svelte";
  import GroupsPanel from "./components/GroupsPanel.svelte";
  import RulesPanel from "./components/RulesPanel.svelte";
  import SettingsPanel from "./components/SettingsPanel.svelte";
  import ZonesPanel from "./components/ZonesPanel.svelte";
  import { api } from "./api";
  import { app } from "./state.svelte";

  // Only the standalone app passes these; inside Desqu the collection app handles take-over itself.
  let { onOpenDesqu, onExit }: { onOpenDesqu?: () => void; onExit?: () => void } = $props();
  const standalone = $derived(Boolean(onOpenDesqu || onExit));

  type Tab = "zones" | "apps" | "groups" | "rules" | "settings" | "diagnostics";
  let tab = $state<Tab>("zones");

  onMount(() => void app.init());
  onDestroy(() => app.dispose());

  const managedCount = $derived(app.status?.managed.length ?? 0);
</script>

{#if app.moduleStatus !== null && !app.isRunning}
  <div class="off">
    {#if app.moduleStatus === "ownedElsewhere" && standalone}
      <h2>Screenbound is managed by Desqu</h2>
      <p class="dim">Desqu is controlling your fullscreen windows. Open Desqu to change settings, or close this window.</p>
      <div class="buttons">
        {#if onOpenDesqu}<button onclick={() => onOpenDesqu()}>Open Desqu</button>{/if}
        {#if onExit}<button onclick={() => onExit()}>Exit</button>{/if}
      </div>
    {:else if app.moduleStatus === "ownedElsewhere"}
      <h2>Screenbound is running elsewhere</h2>
      <p class="dim">Another Desqu or Screenbound window already controls your fullscreen windows. Close it, then try again.</p>
      <button onclick={() => app.turnOn()}>Try again</button>
    {:else if app.moduleStatus === "unsupported"}
      <h2>Screenbound is not supported here</h2>
      <p class="dim">This operating system has no Screenbound backend yet.</p>
    {:else if app.legacy?.olderRunning}
      <h2>An older Screenbound is running</h2>
      <p class="dim">Right-click its tray icon and choose Exit. Two copies would fight over the same windows.</p>
      <button onclick={() => app.turnOn()}>Done, turn on</button>
    {:else}
      <h2>Screenbound is off</h2>
      <p class="dim">Fullscreen windows are not being constrained.</p>
      <button onclick={() => app.turnOn()}>Turn on</button>
    {/if}
    {#if app.error}<p class="error-text">{app.error}</p>{/if}
  </div>
{:else}
<div class="shell">
  <header>
    <div class="brand">Screenbound</div>
    <nav>
      {#each [["zones", "Zones"], ["apps", "Apps"], ["groups", "Groups"], ["rules", "Rules"], ["settings", "Settings"], ["diagnostics", "Diagnostics"]] as [id, label] (id)}
        <button class:active={tab === id} onclick={() => (tab = id as Tab)}>{label}</button>
      {/each}
    </nav>
    <div class="spacer"></div>
    <span class="dim save">
      {#if app.saveState === "pending" || app.saveState === "saving"}Saving…{:else if app.saveState === "saved"}Saved{/if}
    </span>
    {#if managedCount > 0}
      <button class="managed" onclick={() => api.releaseAll()} title="Restore all constrained windows">
        {managedCount} constrained · Release
      </button>
    {/if}
    <label class="enable">
      <input
        type="checkbox"
        checked={app.config?.enabled ?? false}
        disabled={!app.config}
        onchange={(e) => app.setEnabled(e.currentTarget.checked)}
      />
      Enabled
    </label>
  </header>

  {#if app.error}
    <div class="error">
      {app.error}
      <button onclick={() => (app.error = null)}>Dismiss</button>
    </div>
  {/if}

  <main>
    {#if !app.config}
      <p class="dim">Loading…</p>
    {:else if tab === "zones"}
      <ZonesPanel />
    {:else if tab === "apps"}
      <AppsPanel />
    {:else if tab === "groups"}
      <GroupsPanel />
    {:else if tab === "rules"}
      <RulesPanel />
    {:else if tab === "settings"}
      <SettingsPanel />
    {:else}
      <DiagnosticsPanel />
    {/if}
  </main>
</div>
{/if}

<style>
  .off {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 10px;
    height: var(--view-height, 100vh);
  }
  .buttons {
    display: flex;
    gap: 8px;
  }
  .error-text {
    color: var(--error);
  }
  .shell {
    display: flex;
    flex-direction: column;
    height: var(--view-height, 100vh);
  }
  header {
    display: flex;
    align-items: center;
    gap: 14px;
    padding: 8px 14px;
    border-bottom: 1px solid var(--border);
    background: var(--surface-1);
  }
  .brand {
    font-weight: 700;
    letter-spacing: 0.3px;
  }
  nav {
    display: flex;
    gap: 2px;
  }
  nav button {
    background: transparent;
    border-color: transparent;
  }
  nav button.active {
    background: var(--surface-2);
    border-color: var(--border-strong);
  }
  .spacer {
    flex: 1;
  }
  .save {
    font-size: 12px;
  }
  .managed {
    border-color: var(--accent);
    color: var(--accent);
  }
  .enable {
    flex-direction: row;
    align-items: center;
    gap: 6px;
    color: var(--text);
    font-size: 13px;
  }
  .error {
    display: flex;
    justify-content: space-between;
    align-items: center;
    padding: 6px 14px;
    background: rgba(255, 107, 107, 0.12);
    color: var(--error);
  }
  main {
    flex: 1;
    overflow: auto;
    padding: 12px 14px;
    min-height: 0;
  }
</style>
