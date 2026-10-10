<script lang="ts">
  import { vials } from "../state.svelte";
  import { KIND_LABELS, type ItemInfo } from "../types";
  import ItemIcon from "./ItemIcon.svelte";

  let filter = $state("");
  let show = $state<"all" | "free" | "placed">("all");

  const rows = $derived.by(() => {
    const needle = filter.trim().toLowerCase();
    return vials.items
      .filter((i) => (show === "free" ? !i.vial : show === "placed" ? !!i.vial : true))
      .filter((i) => !needle || i.name.toLowerCase().includes(needle) || i.id.toLowerCase().includes(needle))
      .sort((a, b) => a.name.localeCompare(b.name, undefined, { sensitivity: "base" }));
  });

  function vialName(id: string | null): string {
    return vials.config?.vials.find((v) => v.id === id)?.name ?? "";
  }

  function choose(item: ItemInfo, value: string) {
    void vials.assign(item.id, value || null);
  }
</script>

<div class="panel">
  <div class="bar">
    <input placeholder="Filter by name or path" bind:value={filter} aria-label="Filter items" />
    <label class="inline">
      Show
      <select bind:value={show}>
        <option value="all">All desktop items</option>
        <option value="free">Not in a vial</option>
        <option value="placed">In a vial</option>
      </select>
    </label>
    <span class="dim">{rows.length} of {vials.items.length}</span>
  </div>
  <p class="dim note">
    Pick a vial for an item to move it there. It stays in your Desktop folder; the vial only shows it. Taking an item
    out of a vial also stops the sort rules from putting it back.
  </p>

  <div class="list" role="table" aria-label="Desktop items">
    {#each rows as item (item.id)}
      <div class="item" class:missing={item.missing} role="row">
        <ItemIcon id={item.id} size={28} dim={item.missing} />
        <div class="text">
          <span class="name">{item.name}</span>
          <span class="dim small">
            {KIND_LABELS[item.kind]}{item.parked ? " · hidden on the desktop" : ""}{item.missing ? " · no longer on the desktop" : ""}
          </span>
        </div>
        <select
          value={item.vial ?? ""}
          onchange={(e) => choose(item, e.currentTarget.value)}
          aria-label="Vial for {item.name}"
          title={item.vial ? `In ${vialName(item.vial)}` : "Not in a vial"}
        >
          <option value="">Not in a vial</option>
          {#each vials.config?.vials ?? [] as v (v.id)}
            <option value={v.id}>{v.name}</option>
          {/each}
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
  }
  .bar {
    display: flex;
    align-items: center;
    gap: 12px;
  }
  .bar input {
    flex: 1;
    max-width: 360px;
  }
  .inline {
    flex-direction: row;
    align-items: center;
    gap: 6px;
  }
  .note {
    margin: 0;
  }
  .list {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .item {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 4px 8px;
    border-radius: 6px;
    /* Skip layout and paint for rows that are off screen: a desktop can have hundreds of items. */
    content-visibility: auto;
    contain-intrinsic-size: auto 40px;
  }
  .item:hover {
    background: var(--surface-1);
  }
  .item.missing {
    opacity: 0.7;
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
    font-size: 11.5px;
  }
  select {
    min-width: 170px;
  }
</style>
