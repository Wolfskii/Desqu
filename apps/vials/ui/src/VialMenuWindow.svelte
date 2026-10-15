<script lang="ts">
  // The menu a vial opens on a right click, in its own small window: the actions on the left, and the
  // "Sort by" box that opens to the right of them when the pointer rests on it, as in Explorer.
  import { onMount } from "svelte";
  import { api } from "./api";
  import type { VialsConfig } from "./types";

  let { vialId }: { vialId: string } = $props();

  const FIELDS: [string, string][] = [
    ["name", "Name"],
    ["size", "Size"],
    ["type", "Type"],
    ["modified", "Date modified"],
  ];

  let config = $state<VialsConfig | null>(null);
  let sortOpen = $state(false);
  let error = $state<string | null>(null);
  let busy = $state(false);

  const vial = $derived(config?.vials.find((v) => v.id === vialId));
  const theme = $derived(config?.theme ?? "glass");
  const rules = $derived((config?.rules ?? []).filter((r) => r.targetVial === vialId));

  async function close() {
    await api.dismissMenu().catch(() => {});
  }

  async function openRules() {
    try {
      await api.openRules(vialId);
    } catch (e) {
      error = String(e);
      return;
    }
    await close();
  }

  /** A field chosen again flips its direction; a new field starts ascending, as in Explorer. */
  /** Folders first overrides the field and direction. Shown with a tick, like the other checked items. */
  async function toggleFoldersFirst() {
    if (busy) return;
    busy = true;
    try {
      await api.setVialFoldersFirst(vialId, !vial?.foldersFirst);
      await close();
    } catch (e) {
      error = String(e);
      busy = false;
    }
  }

  async function sort(key: string) {
    if (busy) return;
    const again = vial?.sortKey === key;
    const descending = again ? !vial?.sortDesc : false;
    busy = true;
    try {
      await api.sortVialItems(vialId, key, descending);
      await close();
    } catch (e) {
      error = String(e);
      busy = false;
    }
  }

  onMount(() => {
    api.getConfig().then((c) => (config = c)).catch((e) => (error = String(e)));
    // Shows the window now that it is drawn.
    void api.menuReady();
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") void close();
      else if (e.key === "ArrowRight") sortOpen = true;
      else if (e.key === "ArrowLeft") sortOpen = false;
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  });
</script>

<div class="menu" class:fluent={theme === "fluent"} class:fluentLight={theme === "fluentLight"} role="menu">
  <button role="menuitem" class="item" onclick={openRules}>Automatic rules…</button>

  <div
    class="sortwrap"
    role="none"
    onmouseenter={() => (sortOpen = true)}
    onmouseleave={() => (sortOpen = false)}
  >
    <button
      role="menuitem"
      class="item"
      aria-haspopup="menu"
      aria-expanded={sortOpen}
      onclick={() => (sortOpen = true)}
    >
      <span>Sort by</span><span class="arrow">▸</span>
    </button>

    {#if sortOpen}
      <div class="flyout" role="menu">
        <div class="box" class:fluent={theme === "fluent"} class:fluentLight={theme === "fluentLight"}>
          <button role="menuitem" class="item" disabled={busy} onclick={toggleFoldersFirst}>
            <span>Folders first</span>
            {#if vial?.foldersFirst}<span class="arrow" aria-label="on">✓</span>{/if}
          </button>
          <div class="sep"></div>
          {#each FIELDS as [key, label] (key)}
            <button role="menuitem" class="item" disabled={busy} onclick={() => sort(key)}>
              <span>{label}</span>
              {#if vial?.sortKey === key}
                <span class="arrow" aria-label={vial.sortDesc ? "descending" : "ascending"}>{vial.sortDesc ? "▼" : "▲"}</span>
              {/if}
            </button>
          {/each}
        </div>
      </div>
    {/if}
  </div>

  {#if rules.length}
    <div class="sep"></div>
    <p class="caption">Rules that put items here</p>
    {#each rules as r (r.id)}
      <p class="rule">{r.name}</p>
    {/each}
  {/if}

  {#if error}<p class="error" role="alert">{error}</p>{/if}
</div>

<style>
  :global(html),
  :global(body) {
    background: transparent !important;
    overflow: hidden;
  }
  .menu {
    box-sizing: border-box;
    width: 230px;
    padding: 6px;
    border-radius: 8px;
    border: 1px solid rgba(255, 255, 255, 0.12);
    background: rgba(30, 33, 39, 0.97);
    color: rgba(255, 255, 255, 0.92);
    font-size: 13px;
    display: flex;
    flex-direction: column;
    gap: 2px;
    user-select: none;
  }
  .item {
    display: flex;
    justify-content: flex-start;
    align-items: center;
    width: 100%;
    padding: 6px 8px;
    text-align: left;
    background: transparent;
    border-color: transparent;
    border-radius: 4px;
  }
  .item:hover:not(:disabled) {
    background: rgba(255, 255, 255, 0.1);
  }
  .sortwrap {
    position: relative;
  }
  .flyout {
    position: absolute;
    left: 100%;
    top: -6px;
    /* Padding, not a margin: the pointer crosses no gap between the item and the box. */
    padding-left: 4px;
  }
  .box {
    box-sizing: border-box;
    width: 190px;
    padding: 6px;
    border-radius: 8px;
    border: 1px solid rgba(255, 255, 255, 0.12);
    background: rgba(30, 33, 39, 0.97);
    display: flex;
    flex-direction: column;
    gap: 2px;
    box-shadow: 0 8px 24px rgba(0, 0, 0, 0.4);
  }
  .menu.fluent,
  .box.fluent {
    background: rgba(27, 37, 44, 0.95);
    border-color: rgba(217, 236, 245, 0.125);
    color: #ffffff;
    font-family: "Segoe UI Variable Text", "Segoe UI", system-ui, sans-serif;
  }
  .menu.fluentLight,
  .box.fluentLight {
    background: rgba(244, 247, 248, 0.95);
    border-color: rgba(35, 66, 83, 0.102);
    color: rgba(0, 0, 0, 0.894);
    font-family: "Segoe UI Variable Text", "Segoe UI", system-ui, sans-serif;
  }
  .fluent .item:hover:not(:disabled) {
    background: rgba(255, 255, 255, 0.0588);
  }
  .fluentLight .item:hover:not(:disabled) {
    background: rgba(0, 0, 0, 0.035);
  }
  .sep {
    height: 1px;
    margin: 4px 2px;
    background: rgba(255, 255, 255, 0.12);
  }
  .caption {
    margin: 0;
    padding: 2px 8px;
    font-size: 11px;
    opacity: 0.65;
  }
  .rule {
    margin: 0;
    padding: 0 8px;
    font-size: 12px;
  }
  .error {
    margin: 0;
    padding: 4px 8px;
    color: var(--error);
    font-size: 12px;
  }
</style>
