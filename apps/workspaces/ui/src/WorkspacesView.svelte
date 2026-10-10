<script lang="ts">
  import { onDestroy, onMount } from "svelte";
  import DesktopCard from "./components/DesktopCard.svelte";
  import ItemsPanel from "./components/ItemsPanel.svelte";
  import SettingsPanel from "./components/SettingsPanel.svelte";
  import { workspaces } from "./state.svelte";

  // Desqu shows its own start-up and update settings, so it hides these.
  let { showShellSettings = true }: { showShellSettings?: boolean } = $props();

  type Tab = "desktops" | "items" | "settings";
  let tab = $state<Tab>("desktops");

  onMount(() => void workspaces.init());
  onDestroy(() => workspaces.dispose());
</script>

{#if workspaces.moduleStatus !== null && !workspaces.isRunning}
  <div class="off">
    {#if workspaces.moduleStatus === "ownedElsewhere"}
      <h2>Workspaces is running elsewhere</h2>
      <p class="dim">Another Desqu or Workspaces window already controls your desktop. Close it, then try again.</p>
      <button onclick={() => workspaces.turnOn()}>Try again</button>
    {:else if workspaces.moduleStatus === "unsupported"}
      <h2>Workspaces is not supported here</h2>
      <p class="dim">This operating system has no Workspaces backend yet.</p>
    {:else}
      <h2>Workspaces is off</h2>
      <p class="dim">Your desktop items are all visible and nothing follows your virtual desktops.</p>
      <button onclick={() => workspaces.turnOn()}>Turn on</button>
    {/if}
    {#if workspaces.error}<p class="error-text">{workspaces.error}</p>{/if}
  </div>
{:else}
  <div class="shell">
    <header>
      <div class="brand">Workspaces</div>
      <nav>
        {#each [["desktops", "Desktops"], ["items", "Items"], ["settings", "Settings"]] as [id, label] (id)}
          <button class:active={tab === id} onclick={() => (tab = id as Tab)}>{label}</button>
        {/each}
      </nav>
      <div class="spacer"></div>
      <span class="dim save">
        {#if workspaces.saveState === "pending" || workspaces.saveState === "saving"}Saving…{:else if workspaces.saveState === "saved"}Saved{/if}
      </span>
    </header>

    {#if workspaces.error}
      <div class="error">
        {workspaces.error}
        <button onclick={() => (workspaces.error = null)}>Dismiss</button>
      </div>
    {/if}

    <main>
      {#if !workspaces.config}
        <p class="dim">Loading…</p>
      {:else if tab === "desktops"}
        {#if workspaces.needsSetup}
          <section class="card setup">
            <h3>Where do your desktop items belong?</h3>
            <p class="dim">
              You have {workspaces.items.filter((i) => !i.owner).length} shortcuts, files and folders on your desktop.
              Put all of them on one virtual desktop to start with; they will then show only there. After that, new
              files you add belong to the desktop you are on, and you can move any item on the Items tab.
            </p>
            <div class="choices">
              {#each workspaces.desktops.filter((d) => !d.removed) as d (d.id)}
                <button onclick={() => workspaces.assignAll(d.id)}>
                  {d.name}{d.current ? " (this one)" : ""}
                </button>
              {/each}
              <button class="quiet" onclick={() => workspaces.assignAll(null)}>Leave them on every desktop</button>
            </div>
          </section>
        {/if}
        <div class="cards">
          {#each workspaces.desktops as entry (entry.id)}
            <DesktopCard {entry} />
          {:else}
            <section class="card">
              <h3>No virtual desktops yet</h3>
              <p class="dim">
                Windows only lists virtual desktops once you have more than one. Press Win+Tab and choose “New
                desktop”, then come back here.
              </p>
            </section>
          {/each}
        </div>
      {:else if tab === "items"}
        <ItemsPanel />
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
  .setup {
    display: flex;
    flex-direction: column;
    gap: 10px;
    max-width: 860px;
    margin-bottom: 10px;
  }
  .setup h3,
  .setup p {
    margin: 0;
  }
  .choices {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
  }
  .cards {
    display: flex;
    flex-direction: column;
    gap: 10px;
    max-width: 860px;
  }
</style>
