<script lang="ts">
  import { TriStateSwitch } from "@desqu/ui";
  import ModuleCard from "$lib/components/ModuleCard.svelte";
  import { desqu } from "$lib/state.svelte";
</script>

<section class="page">
  <header>
    <div>
      <h1>Your apps</h1>
      <p class="dim">Turn on what you need. Everything runs in this one app.</p>
    </div>
    <div class="all">
      <TriStateSwitch
        state={desqu.allStart}
        label="Start all apps with system"
        disabled={desqu.enabledSupported.length === 0}
        onchange={(next) => desqu.setAllStartWithSystem(next)}
      />
      <span class="dim">Start all apps with system</span>
    </div>
  </header>

  {#each desqu.takeoverPending as m (m.id)}
    <div class="notice" role="alert">
      <div>
        <strong>{m.name} is running on its own.</strong>
        <p class="dim">Let Desqu manage it instead? The standalone copy will close and stop starting with Windows.</p>
      </div>
      <button type="button" disabled={desqu.isBusy(m.id)} onclick={() => desqu.takeover(m.id)}>
        {desqu.isBusy(m.id) ? "Taking over…" : "Let Desqu manage it"}
      </button>
    </div>
  {/each}

  {#if !desqu.loaded}
    <p class="dim">Loading…</p>
  {:else}
    <div class="grid">
      {#each desqu.modules as m (m.id)}
        <ModuleCard module={m} />
      {/each}
    </div>
  {/if}
</section>

<style>
  .page {
    display: flex;
    flex-direction: column;
    gap: 14px;
    padding: 18px 20px;
  }
  header {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: 16px;
    flex-wrap: wrap;
  }
  h1 {
    margin: 0 0 4px;
    font-size: 20px;
  }
  header p {
    margin: 0;
  }
  .all {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(320px, 1fr));
    gap: 12px;
  }
  .notice {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 16px;
    padding: 10px 12px;
    border: 1px solid var(--accent);
    border-radius: 6px;
    background: var(--accent-soft);
  }
  .notice p {
    margin: 2px 0 0;
  }
</style>
