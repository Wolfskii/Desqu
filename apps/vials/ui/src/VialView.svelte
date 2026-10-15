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
    select(e, id);
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

  // Right click on an icon shows Explorer's own menu for the file; nowhere in a vial may the web
  // view's browser menu (Back, Refresh, Save as...) appear.
  function itemMenu(e: MouseEvent, id: string, missing: boolean) {
    e.preventDefault();
    if (missing) return;
    press = null;
    if (!selected.includes(id)) {
      selected = [id];
      anchor = id;
    }
    api.showItemMenu(id).catch((err) => (vials.error = String(err)));
  }

  // Selecting icons and the keyboard: click, Ctrl+click, Shift+click and Ctrl+A select; Ctrl+C and Ctrl+X
  // put the real files on the clipboard (so Explorer and every other program can paste them); Ctrl+V
  // brings the files on the clipboard into this vial. Clicking inside the vial gives it the keyboard.
  let selected = $state<string[]>([]);
  let anchor: string | null = null;
  let cutIds = $state<string[]>([]);
  let notice = $state<string | null>(null);
  let noticeTimer: ReturnType<typeof setTimeout> | null = null;

  function say(message: string) {
    notice = message;
    if (noticeTimer) clearTimeout(noticeTimer);
    noticeTimer = setTimeout(() => (notice = null), 3500);
  }

  function takeKeyboard() {
    void api.focusVial(vialId).catch(() => {});
  }

  function select(e: MouseEvent, id: string) {
    const ids = items.map((i) => i.id);
    if (e.shiftKey && anchor && ids.includes(anchor)) {
      const a = ids.indexOf(anchor);
      const b = ids.indexOf(id);
      selected = ids.slice(Math.min(a, b), Math.max(a, b) + 1).filter((x) => !items.find((i) => i.id === x)?.missing);
      return;
    }
    if (e.ctrlKey || e.metaKey) {
      selected = selected.includes(id) ? selected.filter((x) => x !== id) : [...selected, id];
      anchor = id;
      return;
    }
    // Starting a drag from inside a multi-selection keeps it.
    if (!selected.includes(id)) selected = [id];
    anchor = id;
  }

  // A selection never points at an icon that is gone.
  $effect(() => {
    const here = new Set(items.map((i) => i.id));
    if (selected.some((id) => !here.has(id))) selected = selected.filter((id) => here.has(id));
    if (cutIds.some((id) => !here.has(id))) cutIds = cutIds.filter((id) => here.has(id));
  });

  async function copySelected(cut: boolean) {
    const ids = selected.filter((id) => items.some((i) => i.id === id && !i.missing));
    if (ids.length === 0) return;
    try {
      await api.copyItems(ids, cut);
      cutIds = cut ? ids : [];
      say(`${cut ? "Cut" : "Copied"} ${ids.length} ${ids.length === 1 ? "item" : "items"}`);
    } catch (err) {
      say(String(err));
    }
  }

  async function paste() {
    try {
      const n = await api.pasteIntoVial(vialId);
      cutIds = [];
      say(n > 0 ? `Pasted ${n} ${n === 1 ? "item" : "items"}` : "Nothing was pasted");
      await vials.refresh();
    } catch (err) {
      say(String(err));
    }
  }

  function onKeydown(e: KeyboardEvent) {
    if (editing || e.target instanceof HTMLInputElement) return;
    const mod = e.ctrlKey || e.metaKey;
    const key = e.key.toLowerCase();
    if (mod && !e.shiftKey && !e.altKey && (key === "c" || key === "x")) {
      e.preventDefault();
      void copySelected(key === "x");
    } else if (mod && !e.shiftKey && !e.altKey && key === "v") {
      e.preventDefault();
      void paste();
    } else if (mod && !e.shiftKey && !e.altKey && key === "a") {
      e.preventDefault();
      selected = items.filter((i) => !i.missing).map((i) => i.id);
    } else if (e.key === "Enter") {
      for (const id of selected) void vials.open(id);
    } else if (e.key === "Escape") {
      selected = [];
      cutIds = [];
    }
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

<svelte:window onkeydown={onKeydown} />

<div
  class="vial"
  class:dropping
  class:glass={vials.config?.theme !== "classic"}
  role="presentation"
  oncontextmenu={(e) => e.preventDefault()}
  onpointerup={takeKeyboard}
>
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
    <div
      class="body"
      role="presentation"
      onpointerdown={(e) => {
        // A click on empty space clears the selection.
        const target = e.target as HTMLElement;
        if (target === e.currentTarget || target.classList.contains("grid")) selected = [];
      }}
    >
      {#if !vial}
        <p class="dim msg">This vial no longer exists.</p>
      {:else}
        <div class="grid">
          {#each items as item (item.id)}
            <div class="item" class:missing={item.missing} class:cut={cutIds.includes(item.id)} title={item.missing ? `${item.name} (no longer on the desktop)` : item.name}>
              <button
                class="open"
                class:selected={selected.includes(item.id)}
                ondblclick={() => !item.missing && vials.open(item.id)}
                onpointerdown={(e) => beginPress(e, item.id, item.missing)}
                onpointermove={(e) => movePress(e)}
                onpointerup={endPress}
                onpointercancel={endPress}
                oncontextmenu={(e) => itemMenu(e, item.id, item.missing)}
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
  {#if notice}
    <div class="notice" role="status">{notice}</div>
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
    box-sizing: border-box;
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
  .open.selected {
    background: var(--surface-2);
    border-color: var(--accent);
  }
  /* Cut icons are dimmed until they are pasted, like on the desktop. */
  .item.cut {
    opacity: 0.45;
  }
  .notice {
    flex: none;
    padding: 4px 12px;
    font-size: 12px;
    border-top: 1px solid var(--border);
    background: rgba(34, 38, 45, 0.85);
  }

  /* Glass theme: the blur comes from the window itself; this is the light on top of it. */
  .vial.glass {
    color: rgba(255, 255, 255, 0.94);
    border: 1px solid rgba(255, 255, 255, 0.26);
    border-radius: 12px;
    background:
      radial-gradient(120% 80% at 0% 0%, rgba(255, 255, 255, 0.14), transparent 55%),
      linear-gradient(160deg, rgba(255, 255, 255, 0.05), rgba(255, 255, 255, 0) 45%, rgba(255, 255, 255, 0.03));
    box-shadow:
      inset 0 1px 0 rgba(255, 255, 255, 0.6),
      inset 0 -1px 0 rgba(255, 255, 255, 0.12),
      inset 0 0 28px rgba(255, 255, 255, 0.04);
    transition:
      background 0.15s,
      border-color 0.15s;
  }
  .vial.glass.dropping {
    border-color: rgba(170, 210, 255, 0.85);
    background:
      radial-gradient(120% 80% at 0% 0%, rgba(190, 220, 255, 0.34), transparent 55%),
      linear-gradient(160deg, rgba(120, 175, 255, 0.28), rgba(120, 175, 255, 0.12));
  }
  .glass header {
    background: linear-gradient(180deg, rgba(255, 255, 255, 0.12), rgba(255, 255, 255, 0));
    border-bottom: 1px solid rgba(255, 255, 255, 0.14);
    text-shadow: 0 1px 2px rgba(0, 0, 0, 0.35);
  }
  .glass .title:hover {
    border-color: rgba(255, 255, 255, 0.35);
    background: rgba(255, 255, 255, 0.1);
  }
  .glass .open.selected {
    background: rgba(120, 175, 255, 0.28);
    border-color: rgba(170, 210, 255, 0.7);
  }
  .glass .notice {
    background: rgba(255, 255, 255, 0.1);
    border-top-color: rgba(255, 255, 255, 0.14);
    text-shadow: 0 1px 2px rgba(0, 0, 0, 0.35);
  }
  .glass .open:hover {
    background: rgba(255, 255, 255, 0.16);
    border-color: rgba(255, 255, 255, 0.34);
    box-shadow: inset 0 1px 0 rgba(255, 255, 255, 0.4);
  }
  .glass .label {
    color: rgba(255, 255, 255, 0.95);
    text-shadow: 0 1px 3px rgba(0, 0, 0, 0.6);
  }
</style>
