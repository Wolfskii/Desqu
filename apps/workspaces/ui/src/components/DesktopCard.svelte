<script lang="ts">
  import { Switch } from "@desqu/ui";
  import { workspaces } from "../state.svelte";
  import { type DesktopEntry } from "../types";
  import ItemIcon from "./ItemIcon.svelte";

  let { entry }: { entry: DesktopEntry } = $props();

  const ws = $derived(workspaces.workspace(entry.id));
  const report = $derived(workspaces.reports[entry.id]);
  const busy = $derived(workspaces.busy === entry.id);
  let editing = $state(false);
  let filter = $state("");

  const rows = $derived.by(() => {
    const needle = filter.trim().toLowerCase();
    return workspaces.items
      .filter((i) => !needle || i.name.toLowerCase().includes(needle))
      .sort((a, b) => a.name.localeCompare(b.name, undefined, { sensitivity: "base" }));
  });

  function fileName(path: string): string {
    return path.split(/[\\/]/).pop() ?? path;
  }

  function setHidden(id: string, hide: boolean) {
    const w = workspaces.ensureWorkspace(entry);
    if (!w) return;
    w.hiddenItems = hide ? [...w.hiddenItems.filter((i) => i !== id), id] : w.hiddenItems.filter((i) => i !== id);
    workspaces.scheduleSave(true);
  }

  function setManage(next: boolean) {
    const w = workspaces.ensureWorkspace(entry);
    if (!w) return;
    w.manageIcons = next;
    workspaces.scheduleSave(true);
  }
</script>

<section class="card" class:current={entry.current} class:removed={entry.removed}>
  <header>
    <h3>{entry.name}</h3>
    {#if entry.current}<span class="badge">You are here</span>{/if}
    {#if entry.removed}<span class="badge warn">Desktop removed</span>{/if}
    <span class="spacer"></span>
    {#if entry.removed}
      <button onclick={() => workspaces.removeWorkspace(entry.id)}>Delete workspace</button>
    {:else}
      <button disabled={busy} onclick={() => workspaces.capture(entry)} title="Remember the apps and folders that are open on this desktop">
        {ws ? "Update from open windows" : "Save open windows"}
      </button>
      <button
        disabled={busy || !ws || !entry.current || (ws.apps.length === 0 && ws.folders.length === 0)}
        onclick={() => workspaces.restore(entry)}
        title={entry.current ? "Start what is not open yet" : "Switch to this desktop first"}>Launch here</button
      >
    {/if}
  </header>

  {#if ws}
    <div class="lists">
      <div>
        <h4>Apps</h4>
        {#each ws.apps as app (app.exe + (app.args ?? ""))}
          <div class="row">
            <span class="name" title={app.exe + (app.args ? " " + app.args : "")}>{app.name || fileName(app.exe)}</span>
            <button
              class="x"
              aria-label="Remove {app.name || fileName(app.exe)}"
              onclick={() => {
                ws.apps = ws.apps.filter((a) => a !== app);
                workspaces.scheduleSave(true);
              }}>×</button
            >
          </div>
        {:else}
          <p class="dim">None saved.</p>
        {/each}
      </div>
      <div>
        <h4>Folders</h4>
        {#each ws.folders as folder (folder)}
          <div class="row">
            <span class="name" title={folder}>{folder}</span>
            <button
              class="x"
              aria-label="Remove {folder}"
              onclick={() => {
                ws.folders = ws.folders.filter((f) => f !== folder);
                workspaces.scheduleSave(true);
              }}>×</button
            >
          </div>
        {:else}
          <p class="dim">None saved.</p>
        {/each}
      </div>
    </div>

    <div class="icons">
      <div class="line">
        <div>
          <strong>Hide some desktop icons on this desktop</strong>
          <p class="dim small">
            The chosen icons are hidden here and shown again on the other desktops. Nothing is deleted or moved.
          </p>
        </div>
        <Switch checked={ws.manageIcons} label="Hide desktop icons on {entry.name}" onchange={setManage} />
      </div>
      {#if ws.manageIcons}
        <button class="link" onclick={() => (editing = !editing)}>
          {editing ? "Done choosing" : `Choose icons (${ws.hiddenItems.length} hidden)`}
        </button>
        {#if editing}
          <input placeholder="Filter by name" bind:value={filter} aria-label="Filter desktop items" />
          <div class="items">
            {#each rows as item (item.id)}
              <label class="item">
                <input
                  type="checkbox"
                  checked={ws.hiddenItems.includes(item.id)}
                  onchange={(e) => setHidden(item.id, e.currentTarget.checked)}
                />
                <ItemIcon id={item.id} size={24} />
                <span class="name">{item.name}</span>
              </label>
            {:else}
              <p class="dim">No items match.</p>
            {/each}
          </div>
        {/if}
      {/if}
    </div>
  {:else if !entry.removed}
    <p class="dim">
      No workspace yet. Open the apps and folders you want on this desktop, then press “Save open windows”.
    </p>
  {/if}

  {#if report}
    <div class="report" role="status">
      {#if report.launched.length}<p>Started: {report.launched.join(", ")}.</p>{/if}
      {#if report.alreadyOpen.length}<p class="dim">Already open: {report.alreadyOpen.join(", ")}.</p>{/if}
      {#each report.failed as f (f)}<p class="problem">{f}</p>{/each}
      {#if !report.launched.length && !report.failed.length}<p class="dim">Everything was already open.</p>{/if}
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
    opacity: 0.8;
  }
  header {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  h3,
  h4 {
    margin: 0;
  }
  h4 {
    font-size: 12px;
    color: var(--text-dim, inherit);
    margin-bottom: 4px;
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
  .badge.warn,
  .problem {
    color: var(--warn);
  }
  .lists {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 14px;
  }
  .row {
    display: flex;
    align-items: center;
    gap: 6px;
    min-width: 0;
  }
  .name {
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
  .icons {
    display: flex;
    flex-direction: column;
    gap: 8px;
    border-top: 1px solid var(--border);
    padding-top: 10px;
  }
  .line {
    display: flex;
    justify-content: space-between;
    align-items: center;
    gap: 12px;
  }
  .line p,
  p {
    margin: 2px 0 0;
  }
  .small {
    font-size: 12px;
  }
  .link {
    align-self: flex-start;
  }
  .items {
    display: flex;
    flex-direction: column;
    max-height: 240px;
    overflow: auto;
    border: 1px solid var(--border);
    border-radius: 6px;
  }
  .item {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 3px 8px;
  }
  .report {
    border-top: 1px solid var(--border);
    padding-top: 8px;
  }
  @media (max-width: 700px) {
    .lists {
      grid-template-columns: 1fr;
    }
  }
</style>
