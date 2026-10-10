<script lang="ts">
  import { workspaces } from "../state.svelte";
  import { KIND_LABELS, type ItemInfo } from "../types";
  import ItemIcon from "./ItemIcon.svelte";

  let filter = $state("");
  let show = $state<"all" | "free" | "tied">("all");

  const rows = $derived.by(() => {
    const needle = filter.trim().toLowerCase();
    return workspaces.items
      .filter((i) => (show === "free" ? !i.owner : show === "tied" ? !!i.owner : true))
      .filter((i) => !needle || i.name.toLowerCase().includes(needle))
      .sort((a, b) => a.name.localeCompare(b.name, undefined, { sensitivity: "base" }));
  });

  function choose(item: ItemInfo, value: string) {
    void workspaces.assign(item.id, value || null);
  }

  const live = $derived(workspaces.desktops.filter((d) => !d.removed));
</script>

<div class="panel">
  <div class="bar">
    <input placeholder="Filter by name" bind:value={filter} aria-label="Filter items" />
    <label class="inline">
      Show
      <select bind:value={show}>
        <option value="all">All desktop items</option>
        <option value="free">Shown on every desktop</option>
        <option value="tied">Tied to a desktop</option>
      </select>
    </label>
    <span class="dim">{rows.length} of {workspaces.items.length}</span>
  </div>
  <p class="dim note">
    Pick a desktop for an item and it shows only there; on the other desktops it is hidden. It stays in your Desktop
    folder and is never changed. “Every desktop” shows it everywhere.
  </p>

  <div class="list" role="table" aria-label="Desktop items">
    {#each rows as item (item.id)}
      <div class="item" role="row">
        <ItemIcon id={item.id} size={28} dim={item.hidden} />
        <div class="text">
          <span class="name">{item.name}</span>
          <span class="dim small">{KIND_LABELS[item.kind]}{item.hidden ? " · hidden on this desktop" : ""}</span>
        </div>
        <select
          value={item.owner ?? ""}
          onchange={(e) => choose(item, e.currentTarget.value)}
          aria-label="Desktop for {item.name}"
        >
          <option value="">Every desktop</option>
          {#each live as d (d.id)}
            <option value={d.id}>{d.name}</option>
          {/each}
          {#if item.owner && !live.some((d) => d.id === item.owner)}
            <option value={item.owner} disabled>Removed desktop</option>
          {/if}
        </select>
      </div>
    {:else}
      <p class="dim">No items match.</p>
    {/each}
  </div>
</div>

<style>
  .panel {
    display: flex;
    flex-direction: column;
    gap: 8px;
    min-height: 0;
    max-width: 860px;
  }
  .bar {
    display: flex;
    align-items: center;
    gap: 12px;
    flex-wrap: wrap;
  }
  .bar input {
    flex: 1;
    min-width: 160px;
  }
  .inline {
    display: flex;
    align-items: center;
    gap: 6px;
  }
  .note {
    margin: 0;
    font-size: 12px;
  }
  .list {
    display: flex;
    flex-direction: column;
    border: 1px solid var(--border);
    border-radius: 6px;
  }
  .item {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 4px 10px;
    border-bottom: 1px solid var(--border);
  }
  .item:last-child {
    border-bottom: none;
  }
  .text {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
  }
  .name {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .small {
    font-size: 11px;
  }
</style>
