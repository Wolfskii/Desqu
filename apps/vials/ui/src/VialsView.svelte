<script lang="ts">
  import { onDestroy, onMount } from "svelte";
  import ItemsPanel from "./components/ItemsPanel.svelte";
  import RulesPanel from "./components/RulesPanel.svelte";
  import SettingsPanel from "./components/SettingsPanel.svelte";
  import VialsPanel from "./components/VialsPanel.svelte";
  import { vials } from "./state.svelte";

  // Desqu shows its own start-up and update settings, so it hides these.
  let { showShellSettings = true }: { showShellSettings?: boolean } = $props();

  type Tab = "vials" | "items" | "rules" | "settings";
  let tab = $state<Tab>("vials");

  onMount(() => void vials.init());
  onDestroy(() => vials.dispose());
</script>

{#if vials.moduleStatus !== null && !vials.isRunning}
  <div class="off">
    {#if vials.moduleStatus === "ownedElsewhere"}
      <h2>Vials is running elsewhere</h2>
      <p class="dim">Another Desqu or Vials window already controls your desktop. Close it, then try again.</p>
      <button onclick={() => vials.turnOn()}>Try again</button>
    {:else if vials.moduleStatus === "unsupported"}
      <h2>Vials is not supported here</h2>
      <p class="dim">This operating system has no Vials backend yet.</p>
    {:else}
      <h2>Vials is off</h2>
      <p class="dim">Your desktop items are all visible and no vials are shown.</p>
      <button onclick={() => vials.turnOn()}>Turn on</button>
    {/if}
    {#if vials.error}<p class="error-text">{vials.error}</p>{/if}
  </div>
{:else}
  <div class="shell">
    <header>
      <div class="brand">Vials</div>
      <nav>
        {#each [["vials", "Vials"], ["items", "Items"], ["rules", "Rules"], ["settings", "Settings"]] as [id, label] (id)}
          <button class:active={tab === id} onclick={() => (tab = id as Tab)}>{label}</button>
        {/each}
      </nav>
      <div class="spacer"></div>
      <span class="dim save">
        {#if vials.saveState === "pending" || vials.saveState === "saving"}Saving…{:else if vials.saveState === "saved"}Saved{/if}
      </span>
    </header>

    {#if vials.error}
      <div class="error">
        {vials.error}
        <button onclick={() => (vials.error = null)}>Dismiss</button>
      </div>
    {/if}

    <main>
      {#if !vials.config}
        <p class="dim">Loading…</p>
      {:else if tab === "vials"}
        <VialsPanel />
      {:else if tab === "items"}
        <ItemsPanel />
      {:else if tab === "rules"}
        <RulesPanel />
      {:else}
        <SettingsPanel {showShellSettings} />
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
