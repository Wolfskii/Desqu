<script lang="ts">
  import { SettingsRow, ShellSettingsSection, Switch, UpdateCard } from "@desqu/ui";
  import { workspaces } from "../state.svelte";

  let { showShellSettings = true }: { showShellSettings?: boolean } = $props();
</script>

<div class="settings">
  <section class="card">
    <SettingsRow
      label="New files join the desktop I am on"
      description="A file that appears on your desktop while Workspaces is running is tied to the virtual desktop you are on, so it does not show up on the others. Files that were already there stay on every desktop until you choose."
    >
      <Switch
        checked={workspaces.config?.autoAssignNew ?? true}
        label="New files join the desktop I am on"
        disabled={!workspaces.config}
        onchange={(next) => {
          if (!workspaces.config) return;
          workspaces.config.autoAssignNew = next;
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
        {(workspaces.desktop?.hiddenCount ?? 0) === 1 ? "item is" : "items are"} hidden on this desktop right now.
      </p>
    {/if}
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
