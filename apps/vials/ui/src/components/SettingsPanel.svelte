<script lang="ts">
  import { SettingsRow, ShellSettingsSection, Switch, UpdateCard } from "@desqu/ui";
  import { vials } from "../state.svelte";

  let { showShellSettings = true }: { showShellSettings?: boolean } = $props();
</script>

<div class="settings">
  <section class="card">
    <SettingsRow
      label="Sort new items automatically"
      description="New desktop items go into a vial by the first matching rule (Rules tab)."
    >
      <Switch
        checked={vials.config?.autoSort ?? true}
        label="Sort new items automatically"
        disabled={!vials.config}
        onchange={(next) => {
          if (!vials.config) return;
          vials.config.autoSort = next;
          vials.scheduleSave(true);
        }}
      />
    </SettingsRow>
    <SettingsRow label="Look of vials" description="Glass blurs the desktop behind a vial with rounded corners and light edges. Classic is a plain dark panel.">
      <select
        value={vials.config?.theme ?? "glass"}
        disabled={!vials.config}
        aria-label="Look of vials"
        onchange={(e) => {
          if (!vials.config) return;
          vials.config.theme = e.currentTarget.value === "classic" ? "classic" : "glass";
          vials.scheduleSave(true);
        }}
      >
        <option value="glass">Glass</option>
        <option value="classic">Classic</option>
      </select>
    </SettingsRow>
    <SettingsRow
      label="Create vials by dragging a box on the desktop"
      description="Drag a box over empty space on the desktop and a small menu offers to create a vial there. Dragging over icons selects them as usual."
    >
      <Switch
        checked={vials.config?.createOnDrag ?? true}
        label="Create vials by dragging a box on the desktop"
        disabled={!vials.config}
        onchange={(next) => {
          if (!vials.config) return;
          vials.config.createOnDrag = next;
          vials.scheduleSave(true);
        }}
      />
    </SettingsRow>
    <SettingsRow
      label="Add “Create vial here” to the desktop right-click menu"
      description="Right-click empty desktop space and choose it to make a vial there. On Windows 11 it is under “Show more options”. The entry exists only while Vials is running."
    >
      <Switch
        checked={vials.config?.contextMenu ?? true}
        label="Add Create vial here to the desktop right-click menu"
        disabled={!vials.config}
        onchange={(next) => {
          if (!vials.config) return;
          vials.config.contextMenu = next;
          vials.scheduleSave(true);
        }}
      />
    </SettingsRow>
    <SettingsRow
      label="Hide items on the desktop"
      description="Items in a vial are moved out of sight on the desktop (never deleted or changed) and put back when you take them out of the vial, turn Vials off, or quit. If Vials crashes, they are restored the next time it starts."
    >
      <Switch
        checked={vials.config?.hideOnDesktop ?? true}
        label="Hide items on the desktop"
        disabled={!vials.config}
        onchange={(next) => {
          if (!vials.config) return;
          vials.config.hideOnDesktop = next;
          vials.scheduleSave(true);
        }}
      />
    </SettingsRow>
  </section>

  <section class="card status">
    <h3>Desktop</h3>
    {#if vials.desktop?.problem}
      <p class="problem">{vials.desktop.problem}</p>
    {:else if vials.desktop?.canHide === false}
      <p class="problem">
        “Auto arrange icons” is on, so Windows ignores icon positions and items cannot be hidden. Right-click the
        desktop → View → turn off “Auto arrange icons”.
      </p>
    {:else}
      <p class="dim">
        {vials.desktop?.hiddenCount ?? 0}
        {(vials.desktop?.hiddenCount ?? 0) === 1 ? "item is" : "items are"} currently hidden on the desktop.
      </p>
    {/if}
  </section>

  {#if showShellSettings}
    <ShellSettingsSection appName="Vials" />
    <UpdateCard />
  {/if}
</div>

<style>
  .settings {
    display: flex;
    flex-direction: column;
    gap: 10px;
    max-width: 720px;
  }
  .card {
    display: flex;
    flex-direction: column;
    gap: 14px;
  }
  .status h3 {
    margin: 0;
  }
  .status p {
    margin: 0;
  }
  .problem {
    color: var(--warn);
  }
</style>
