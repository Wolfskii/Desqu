<script lang="ts">
  import "../app.css";
  import { onDestroy, onMount } from "svelte";
  import Sidebar from "$lib/components/Sidebar.svelte";
  import { desqu } from "$lib/state.svelte";

  let { children } = $props();

  onMount(() => void desqu.init());
  onDestroy(() => desqu.dispose());
</script>

<div class="app">
  <Sidebar />
  <main>
    {#if desqu.error}
      <div class="error" role="alert">
        {desqu.error}
        <button type="button" onclick={() => (desqu.error = null)}>Dismiss</button>
      </div>
    {/if}
    {@render children()}
  </main>
</div>

<style>
  .app {
    display: flex;
    height: 100vh;
  }
  main {
    flex: 1;
    min-width: 0;
    overflow: auto;
  }
  .error {
    display: flex;
    justify-content: space-between;
    align-items: center;
    padding: 6px 14px;
    background: rgba(255, 107, 107, 0.12);
    color: var(--error);
  }
</style>
