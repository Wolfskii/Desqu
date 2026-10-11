<script lang="ts">
  import { Switch } from "@desqu/ui";
  import { desqu } from "$lib/state.svelte";
  import { statusText, type ModuleEntry } from "$lib/types";

  let { module }: { module: ModuleEntry } = $props();

  const busy = $derived(desqu.isBusy(module.id));
  const usable = $derived(module.supported && !busy);
</script>

<article class="card" class:unsupported={!module.supported}>
  <header>
    <span class="icon" aria-hidden="true">{module.name.slice(0, 1)}</span>
    <div class="titles">
      <h3>{module.name}</h3>
      <span
        class="status"
        class:on={module.status === "on"}
        class:paused={module.status === "paused"}
        class:elsewhere={module.status === "ownedElsewhere"}>{statusText(module.status)}</span
      >
    </div>
    <Switch
      checked={module.enabled}
      label="Turn {module.name} on"
      disabled={!usable}
      onchange={(next) => desqu.setEnabled(module.id, next)}
    />
  </header>

  <p class="dim">{module.description}</p>

  <footer>
    <span class="start">
      <Switch
        checked={module.enabled && module.startWithSystem}
        label="Start {module.name} with system"
        disabled={!usable || !module.enabled}
        onchange={(next) => desqu.setStartWithSystem(module.id, next)}
      />
      <span class="dim" title={module.enabled ? "" : "Turn the app on first"}>Start with system</span>
    </span>
    <a class="open" class:disabled={!module.supported} href="/{module.id}" aria-disabled={!module.supported}>Open</a>
  </footer>
</article>

<style>
  article {
    display: flex;
    flex-direction: column;
    gap: 10px;
  }
  article.unsupported {
    opacity: 0.6;
  }
  header {
    display: flex;
    align-items: center;
    gap: 12px;
  }
  .icon {
    display: grid;
    place-items: center;
    flex: none;
    width: 38px;
    height: 38px;
    border-radius: 10px;
    background: var(--accent-soft);
    border: 1px solid var(--accent);
    color: var(--accent);
    font-size: 18px;
    font-weight: 700;
  }
  .titles {
    flex: 1;
    min-width: 0;
  }
  .titles h3 {
    margin: 0;
    font-size: 15px;
  }
  .status {
    font-size: 12px;
    color: var(--text-dim);
  }
  .status.on {
    color: #4cc38a;
  }
  .status.paused {
    color: var(--warn);
  }
  .status.elsewhere {
    color: var(--accent);
  }
  p {
    margin: 0;
  }
  footer {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 10px;
  }
  .start {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .open {
    padding: 4px 14px;
    border: 1px solid var(--border-strong);
    border-radius: 4px;
    background: var(--surface-2);
    color: var(--text);
    text-decoration: none;
  }
  .open:hover:not(.disabled) {
    border-color: var(--accent);
  }
  .open.disabled {
    opacity: 0.45;
    pointer-events: none;
  }
</style>
