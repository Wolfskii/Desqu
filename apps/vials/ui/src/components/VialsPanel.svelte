<script lang="ts">
  import { Switch } from "@desqu/ui";
  import { vials } from "../state.svelte";
  import type { NormalizedRect } from "../types";

  const percent = (v: number) => Math.round(v * 1000) / 10;

  function setPercent(rect: NormalizedRect, key: keyof NormalizedRect, value: number) {
    if (!Number.isFinite(value)) return;
    rect[key] = Math.min(1, Math.max(0, value / 100));
    vials.scheduleSave();
  }

  const fields: { key: keyof NormalizedRect; label: string }[] = [
    { key: "x", label: "Left" },
    { key: "y", label: "Top" },
    { key: "width", label: "Width" },
    { key: "height", label: "Height" },
  ];
</script>

<div class="panel">
  <div class="head">
    <p class="dim">
      A vial is a box on your desktop that holds shortcuts, files and folders. Make one by dragging a box over empty
      desktop space, or right-click the desktop and choose “Create vial here”. Drag icons in and out like any desktop
      icon, and move or resize a vial right on the desktop; the position is saved.
    </p>
    <button onclick={() => vials.addVial()}>+ New vial</button>
  </div>

  {#each vials.config?.vials ?? [] as vial (vial.id)}
    <div class="card vial">
      <div class="row">
        <input class="name" bind:value={vial.name} oninput={() => vials.scheduleSave()} aria-label="Vial name" />
        <span class="dim">{vial.items.length} {vial.items.length === 1 ? "item" : "items"}</span>
        <div class="spacer"></div>
        <button class="danger" onclick={() => vials.removeVial(vial.id)}>Delete</button>
      </div>

      <div class="grid">
        <label>
          Monitor
          <select
            value={vial.monitor ?? ""}
            onchange={(e) => {
              vial.monitor = e.currentTarget.value || null;
              vials.scheduleSave(true);
            }}
          >
            <option value="">Primary monitor</option>
            {#each vials.monitors as m (m.name)}
              <option value={m.name}>{m.name}{m.primary ? " (primary)" : ""}</option>
            {/each}
          </select>
        </label>
        {#each fields as f (f.key)}
          <label>
            {f.label} %
            <input
              type="number"
              min="0"
              max="100"
              step="0.5"
              value={percent(vial.rect[f.key])}
              oninput={(e) => setPercent(vial.rect, f.key, e.currentTarget.valueAsNumber)}
            />
          </label>
        {/each}
      </div>

      <div class="option">
        <div class="info">
          <span>Collapsed</span>
          <span class="dim">Shrink the vial to just its title bar.</span>
        </div>
        <Switch
          checked={vial.collapsed ?? false}
          label="Collapse {vial.name}"
          onchange={(next) => {
            vial.collapsed = next;
            vials.scheduleSave(true);
          }}
        />
      </div>
    </div>
  {:else}
    <p class="dim">No vials yet. Drag a box on your empty desktop to create one.</p>
  {/each}
</div>

<style>
  .panel {
    display: flex;
    flex-direction: column;
    gap: 10px;
  }
  .head {
    display: flex;
    justify-content: space-between;
    align-items: center;
    gap: 12px;
  }
  .head p {
    margin: 0;
  }
  .vial {
    display: flex;
    flex-direction: column;
    gap: 10px;
  }
  .row {
    display: flex;
    align-items: center;
    gap: 10px;
  }
  .name {
    font-weight: 600;
    min-width: 220px;
  }
  .spacer {
    flex: 1;
  }
  .grid {
    display: grid;
    grid-template-columns: 2fr repeat(4, 1fr);
    gap: 10px;
  }
  .option {
    display: flex;
    align-items: center;
    gap: 12px;
  }
  .option .info {
    display: flex;
    flex-direction: column;
    gap: 2px;
    flex: 1;
  }
</style>
