<script lang="ts">
  import type { Snippet } from "svelte";

  let { label, description, children }: { label: string; description?: string; children: Snippet } = $props();
</script>

<div class="row">
  <div class="text">
    <h3>{label}</h3>
    {#if description}<p class="dim">{description}</p>{/if}
  </div>
  <div class="control">{@render children()}</div>
</div>

<style>
  .row {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: 16px;
  }
  .text p {
    margin: 0;
  }
  .control {
    flex: none;
  }
</style>
