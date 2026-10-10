<script lang="ts">
  import { SettingsRow, ShellSettingsSection, Switch, UpdateCard } from "@desqu/ui";
  import { workspaces } from "../state.svelte";

  let { showShellSettings = true }: { showShellSettings?: boolean } = $props();
</script>

<div class="settings">
  <section class="card">
    <SettingsRow
      label="Refresh a workspace when I leave its desktop"
      description="Replaces the saved apps and folders of that desktop with what is open there. Off by default because it overwrites your lists."
    >
      <Switch
        checked={workspaces.config?.autoSave ?? false}
        label="Refresh a workspace when I leave its desktop"
        disabled={!workspaces.config}
        onchange={(next) => {
          if (!workspaces.config) return;
          workspaces.config.autoSave = next;
          workspaces.scheduleSave(true);
        }}
      />
    </SettingsRow>
  </section>

  <section class="card status">
    <h3>Desktop</h3>
    {#if workspaces.desktop?.problem}
      <p class="problem">{workspaces.desktop.problem}</p>
    {:else if workspaces.desktop?.canHide === false}
      <p class="problem">
        “Auto arrange icons” is on, so Windows ignores icon positions and items cannot be hidden. Right-click the
        desktop → View → turn off “Auto arrange icons”.
      </p>
    {:else}
      <p class="dim">
        {workspaces.desktop?.hiddenCount ?? 0}
        {(workspaces.desktop?.hiddenCount ?? 0) === 1 ? "item is" : "items are"} currently hidden on the desktop.
      </p>
    {/if}
    <p class="dim">
      Workspaces starts apps and folders on the desktop you are on; Windows does not let an app move other apps'
      windows to another virtual desktop.
    </p>
  </section>

  {#if showShellSettings}
    <ShellSettingsSection appName="Workspaces" />
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
  .status h3,
  .status p {
    margin: 0;
  }
  .problem {
    color: var(--warn);
  }
</style>
