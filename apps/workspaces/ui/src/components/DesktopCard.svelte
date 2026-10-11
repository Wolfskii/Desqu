<script lang="ts">
  import { workspaces } from "../state.svelte";
  import type { DesktopEntry } from "../types";

  let { entry }: { entry: DesktopEntry } = $props();
</script>

<section class="card" class:current={entry.current} class:removed={entry.removed}>
  <header>
    <h3>{entry.name}</h3>
    {#if entry.current}<span class="badge">You are here</span>{/if}
    {#if entry.removed}<span class="badge warn">Desktop removed</span>{/if}
    <span class="spacer"></span>
    <span class="count">{entry.itemCount} {entry.itemCount === 1 ? "item" : "items"}</span>
    {#if entry.itemCount > 0}
      <button
        onclick={() => workspaces.removeWorkspace(entry.id)}
        title="Its items will show on every desktop again"
      >
        {entry.removed ? "Release its items" : "Show on every desktop"}
      </button>
    {/if}
  </header>

  {#if entry.removed}
    <p class="dim">This desktop no longer exists, so its items are shown everywhere again.</p>
  {:else if entry.itemCount === 0}
    <p class="dim">
      Nothing is tied to this desktop yet. Select icons on another desktop and press Ctrl+X, then come here and
      press Ctrl+V on the desktop to move them here. Files you add while you are here join it automatically.
    </p>
  {/if}
</section>

<style>
  .card {
    display: flex;
    flex-direction: column;
    gap: 8px;
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
  .count {
    font-variant-numeric: tabular-nums;
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
</style>
