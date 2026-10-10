<script lang="ts">
  import { page } from "$app/state";
  import { ScreenboundView } from "@desqu/screenbound-ui";
  import { VialsView } from "@desqu/vials-ui";
  import { WorkspacesView } from "@desqu/workspaces-ui";
  import { desqu } from "$lib/state.svelte";

  const id = $derived(page.params.module ?? "");
  const entry = $derived(desqu.modules.find((m) => m.id === id));

  let menuOpen = $state(false);
  let confirmingReset = $state(false);
</script>

{#if !desqu.loaded}
  <p class="dim pad">Loading…</p>
{:else if !entry}
  <div class="pad">
    <h2>Unknown app</h2>
    <p class="dim">There is no app called “{id}”.</p>
    <a href="/">Back to your apps</a>
  </div>
{:else}
  <div class="strip">
    <strong>{entry.name}</strong>
    <span class="dim">version {entry.version}</span>
    <span class="spacer"></span>
    <div class="more">
      <button type="button" aria-expanded={menuOpen} onclick={() => (menuOpen = !menuOpen)}>More ▾</button>
      {#if menuOpen}
        <div class="menu" role="menu">
          {#if confirmingReset}
            <p class="warn">
              This deletes {entry.name}'s settings: zones, rules, vials, workspaces and groups. They are shared with the standalone
              {entry.name} app, so it loses them too.
            </p>
            <button
              type="button"
              class="danger"
              role="menuitem"
              onclick={async () => {
                await desqu.reset(entry.id);
                confirmingReset = false;
                menuOpen = false;
              }}>Delete settings</button
            >
            <button type="button" role="menuitem" onclick={() => (confirmingReset = false)}>Cancel</button>
          {:else}
            <button type="button" role="menuitem" onclick={() => (confirmingReset = true)}>Reset settings…</button>
            {#if entry.standaloneInstalled}
              <button
                type="button"
                role="menuitem"
                onclick={async () => {
                  await desqu.removeStandalone(entry.id);
                  menuOpen = false;
                }}>Remove standalone copy</button
              >
            {/if}
          {/if}
        </div>
      {/if}
    </div>
  </div>

  <!-- The module's own UI fills the rest of the window. -->
  <div class="view" style="--view-height: calc(100vh - 45px)">
    {#if entry.id === "screenbound"}
      <ScreenboundView />
    {:else if entry.id === "vials"}
      <VialsView showShellSettings={false} />
    {:else if entry.id === "workspaces"}
      <WorkspacesView showShellSettings={false} />
    {/if}
  </div>
{/if}

<style>
  .pad {
    padding: 18px 20px;
  }
  .strip {
    display: flex;
    align-items: center;
    gap: 10px;
    height: 45px;
    padding: 0 14px;
    border-bottom: 1px solid var(--border);
    background: var(--surface-1);
  }
  .spacer {
    flex: 1;
  }
  .more {
    position: relative;
  }
  .menu {
    position: absolute;
    right: 0;
    top: calc(100% + 4px);
    z-index: 10;
    display: flex;
    flex-direction: column;
    gap: 6px;
    min-width: 230px;
    padding: 8px;
    border: 1px solid var(--border-strong);
    border-radius: 6px;
    background: var(--surface-1);
    box-shadow: 0 6px 20px rgba(0, 0, 0, 0.4);
  }
  .menu button {
    text-align: left;
  }
  .warn {
    margin: 0;
    font-size: 12px;
    color: var(--warn);
  }
</style>
