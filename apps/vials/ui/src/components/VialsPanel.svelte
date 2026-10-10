<script lang="ts">
  import { vials } from "../state.svelte";
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
        <div class="spacer"></div>
        <button class="danger" onclick={() => vials.removeVial(vial.id)}>Delete</button>
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
</style>
