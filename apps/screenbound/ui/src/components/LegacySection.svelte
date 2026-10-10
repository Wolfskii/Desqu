<script lang="ts">
  import { app } from "../state.svelte";

  let confirmingDelete = $state(false);
</script>

{#if app.legacy?.oldInstall || app.legacy?.settingsBackup}
  <section class="card">
    <h3>Older Screenbound</h3>

    {#if app.legacy.oldInstall}
      <div class="row">
        <p class="dim">
          Another Screenbound installation was found{app.legacy.oldInstall.version
            ? ` (version ${app.legacy.oldInstall.version})`
            : ""}. You can remove it once you no longer need it.
        </p>
        <button type="button" onclick={() => app.uninstallOld()}>Uninstall</button>
      </div>
    {/if}

    {#if app.legacy.settingsBackup}
      <div class="row">
        <p class="dim">
          Your settings were copied from <span class="mono">{app.legacy.settingsBackup}</span>. That folder is kept
          as a backup; it is safe to delete once you have checked everything works.
        </p>
        {#if confirmingDelete}
          <span class="confirm">
            <button
              type="button"
              class="danger"
              onclick={async () => {
                await app.deleteSettingsBackup();
                confirmingDelete = false;
              }}>Delete permanently</button
            >
            <button type="button" onclick={() => (confirmingDelete = false)}>Cancel</button>
          </span>
        {:else}
          <button type="button" class="danger" onclick={() => (confirmingDelete = true)}>Delete old settings…</button>
        {/if}
      </div>
    {/if}
  </section>
{/if}

<style>
  .row {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: 16px;
  }
  .row + .row {
    margin-top: 10px;
  }
  .row p {
    margin: 0;
  }
  .confirm {
    display: flex;
    gap: 6px;
    flex: none;
  }
</style>
