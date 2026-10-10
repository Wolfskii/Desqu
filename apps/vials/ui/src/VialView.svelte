<script lang="ts">
  // The content of one vial's own window on the desktop.
  import { getCurrentWebview } from "@tauri-apps/api/webview";
  import { onDestroy, onMount, tick } from "svelte";
  import { api } from "./api";
  import ItemIcon from "./components/ItemIcon.svelte";
  import { vials } from "./state.svelte";

  let { vialId }: { vialId: string } = $props();

  const vial = $derived(vials.config?.vials.find((v) => v.id === vialId));
  const items = $derived(vial ? vials.itemsOf(vial) : []);

  let dropping = $state(false);
  let unlistenDrop: (() => void) | null = null;

  onMount(() => {
    void vials.init();
    // Icons dragged from the desktop onto this window are added to the vial.
    getCurrentWebview()
      .onDragDropEvent((event) => {
        const p = event.payload;
        if (p.type === "enter" || p.type === "over") dropping = true;
        else if (p.type === "leave") dropping = false;
        else if (p.type === "drop") {
          dropping = false;
          api.dropItems(vialId, p.paths).then(() => vials.refresh()).catch((e) => (vials.error = String(e)));
        }
      })
      .then((un) => (unlistenDrop = un))
      .catch(() => {});
  });
  onDestroy(() => {
    vials.dispose();
    unlistenDrop?.();
  });

  // Dragging an icon out starts the standard Windows drag (the icon follows the cursor, targets see a
  // real file), so it can be dropped on the desktop, another vial or a folder like any desktop icon.
  const DRAG_THRESHOLD = 5;
  let press: { id: string; x: number; y: number } | null = null;
  let dragging = false;

  function beginPress(e: PointerEvent, id: string, missing: boolean) {
    if (e.button !== 0 || missing) return;
    press = { id, x: e.clientX, y: e.clientY };
  }

  function movePress(e: PointerEvent) {
    if (!press || dragging || !(e.buttons & 1)) return;
    if (Math.hypot(e.clientX - press.x, e.clientY - press.y) < DRAG_THRESHOLD) return;
    const id = press.id;
    press = null;
    dragging = true;
    api
      .dragItemOut(id)
      .catch((err) => (vials.error = String(err)))
      .finally(() => {
        dragging = false;
        void vials.refresh();
      });
  }

  function endPress() {
    press = null;
  }

  // Clicking the name edits it right on the desktop.
  let editing = $state(false);
  let draft = $state("");
  let nameInput: HTMLInputElement | undefined = $state();

  async function startEdit() {
    if (!vial || editing) return;
    draft = vial.name;
    try {
      await api.beginRename(vialId);
    } catch (e) {
      vials.error = String(e);
      return;
    }
    editing = true;
    await tick();
    nameInput?.focus();
    nameInput?.select();
  }

  async function finishEdit(save: boolean) {
    if (!editing) return;
    editing = false;
    const name = draft.trim();
    if (save && vial && name && name !== vial.name) {
      vial.name = name;
      vials.scheduleSave(true);
    }
    try {
      await api.endRename(vialId);
    } catch (e) {
      vials.error = String(e);
    }
  }

  function toggleCollapsed() {
    if (!vial) return;
    vial.collapsed = !vial.collapsed;
    vials.scheduleSave(true);
  }
</script>

<div class="vial" class:dropping>
  <header data-tauri-drag-region>
    {#if editing}
      <input
        class="name-input"
        bind:this={nameInput}
        bind:value={draft}
        maxlength="80"
        aria-label="Vial name"
        onkeydown={(e) => {
          if (e.key === "Enter") void finishEdit(true);
          else if (e.key === "Escape") void finishEdit(false);
        }}
        onblur={() => finishEdit(true)}
      />
    {:else}
      <button class="title" title="Click to rename" onclick={startEdit}>{vial?.name ?? "Vial"}</button>
    {/if}
    <span class="grab" data-tauri-drag-region></span>
    <button
      class="chevron"
      title={vial?.collapsed ? "Expand" : "Collapse"}
      aria-label={vial?.collapsed ? "Expand" : "Collapse"}
      onclick={toggleCollapsed}>{vial?.collapsed ? "▾" : "▴"}</button
    >
  </header>

  {#if !vial?.collapsed}
    <div class="body">
      {#if !vial}
        <p class="dim msg">This vial no longer exists.</p>
      {:else}
        <div class="grid">
          {#each items as item (item.id)}
            <div class="item" class:missing={item.missing} title={item.missing ? `${item.name} (no longer on the desktop)` : item.name}>
              <button
                class="open"
                ondblclick={() => !item.missing && vials.open(item.id)}
                onkeydown={(e) => e.key === "Enter" && !item.missing && vials.open(item.id)}
                onpointerdown={(e) => beginPress(e, item.id, item.missing)}
                onpointermove={(e) => movePress(e)}
                onpointerup={endPress}
                onpointercancel={endPress}
                aria-label="Open {item.name}"
              >
                <ItemIcon id={item.id} size={40} dim={item.missing} />
                <span class="label">{item.name}</span>
              </button>
            </div>
          {/each}
        </div>
      {/if}
    </div>
  {/if}
</div>

<style>
  :global(html),
  :global(body) {
    background: transparent !important;
    height: 100%;
    overflow: hidden;
    user-select: none;
  }
  .vial {
    display: flex;
    flex-direction: column;
    height: 100vh;
    border: 1px solid var(--border-strong);
    border-radius: 8px;
    background: rgba(24, 27, 32, 0.9);
    overflow: hidden;
  }
  .vial.dropping {
    border-color: var(--accent);
    background: rgba(34, 52, 76, 0.85);
  }
  header {
    display: flex;
    align-items: center;
    gap: 8px;
    height: 34px;
    flex: none;
    padding: 0 6px 0 12px;
    background: rgba(34, 38, 45, 0.85);
    cursor: default;
  }
  .title {
    flex: 0 1 auto;
    min-width: 0;
    padding: 2px 6px;
    margin-left: -6px;
    border: 1px solid transparent;
    border-radius: 4px;
    background: transparent;
    font-weight: 600;
    text-align: left;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    cursor: text;
  }
  .title:hover {
    border-color: var(--border-strong);
  }
  .name-input {
    flex: 1;
    min-width: 0;
    height: 24px;
    font-weight: 600;
  }
  .grab {
    flex: 1;
    align-self: stretch;
  }
  .chevron {
    width: 26px;
    padding: 0;
    background: transparent;
    border-color: transparent;
  }
  .body {
    flex: 1;
    overflow: auto;
    padding: 8px;
    min-height: 0;
  }
  .grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(84px, 1fr));
    gap: 4px;
  }
  .item {
    position: relative;
  }
  .open {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 4px;
    width: 100%;
    padding: 8px 4px 6px;
    background: transparent;
    border-color: transparent;
    border-radius: 6px;
  }
  .open:hover {
    background: var(--surface-2);
    border-color: var(--border-strong);
  }
  .label {
    max-width: 100%;
    font-size: 12px;
    text-align: center;
    display: -webkit-box;
    -webkit-line-clamp: 2;
    line-clamp: 2;
    -webkit-box-orient: vertical;
    overflow: hidden;
    word-break: break-word;
  }
  .item.missing .label {
    opacity: 0.5;
  }
</style>
