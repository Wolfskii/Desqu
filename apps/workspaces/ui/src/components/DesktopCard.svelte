<script lang="ts">
  import { workspaces } from "../state.svelte";
  import type { DesktopEntry } from "../types";
  import ItemIcon from "./ItemIcon.svelte";

  let { entry }: { entry: DesktopEntry } = $props();

  const owned = $derived(workspaces.itemsOf(entry.id));
  // The ones this card shows when the desktop is gone: only ids are known.
  const orphaned = $derived(workspaces.workspace(entry.id)?.items ?? []);
</script>

<section class="card" class:current={entry.current} class:removed={entry.removed}>
  <header>
    <h3>{entry.name}</h3>
    {#if entry.current}<span class="badge">You are here</span>{/if}
    {#if entry.removed}<span class="badge warn">Desktop removed</span>{/if}
    <span class="spacer"></span>
    <span class="dim">{entry.itemCount} {entry.itemCount === 1 ? "item" : "items"}</span>
    {#if entry.removed}
      <button onclick={() => workspaces.removeWorkspace(entry.id)} title="Its items will show on every desktop">
        Release its items
      </button>
    {/if}
  </header>

  {#if entry.removed}
    <p class="dim">This desktop no longer exists, so its {orphaned.length} items are shown everywhere again.</p>
  {:else if owned.length === 0}
    <p class="dim">
      Nothing is tied to this desktop yet. Pick it for an item on the Items tab; new files you add while you are
      here will join it automatically.
    </p>
  {:else}
    <div class="grid">
      {#each owned as item (item.id)}
        <div class="tile" class:hidden={item.hidden} title={item.name}>
          <ItemIcon id={item.id} size={32} dim={item.hidden} />
          <span class="label">{item.name}</span>
          <button
            class="x"
            aria-label="Show {item.name} on every desktop"
            title="Show on every desktop"
            onclick={() => workspaces.assign(item.id, null)}>×</button
          >
        </div>
      {/each}
    </div>
  {/if}
</section>

<style>
  .card {
    display: flex;
    flex-direction: column;
    gap: 10px;
  }
  .current {
    border-color: var(--accent, var(--border-strong));
  }
  .removed {
    opacity: 0.85;
  }
  header {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  h3 {
    margin: 0;
  }
  p {
    margin: 0;
  }
  .spacer {
    flex: 1;
  }
  .badge {
    font-size: 11px;
    padding: 1px 8px;
    border: 1px solid var(--border-strong);
    border-radius: 10px;
  }
  .badge.warn {
    color: var(--warn);
  }
  .grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(150px, 1fr));
    gap: 6px;
  }
  .tile {
    position: relative;
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 6px 8px;
    border: 1px solid var(--border);
    border-radius: 6px;
    min-width: 0;
  }
  .tile.hidden .label {
    opacity: 0.55;
  }
  .label {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .x {
    background: transparent;
    border-color: transparent;
    padding: 0 6px;
  }
</style>
