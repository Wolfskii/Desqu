<script lang="ts">
  // The popup that appears after dragging a box on the empty desktop, styled like a context menu.
  import { onMount } from "svelte";
  import { api } from "./api";

  let glass = $state(true);
  let busy = $state(false);
  let error = $state<string | null>(null);
  let button: HTMLButtonElement | undefined = $state();

  // Show the window only now that the page is drawn.
  onMount(async () => {
    try {
      glass = (await api.getConfig()).theme !== "classic";
    } catch {
      // Keep the default look.
    }
    button?.focus();
    void api.menuReady();
  });

  async function create() {
    if (busy) return;
    busy = true;
    try {
      await api.createVialFromMenu();
    } catch (e) {
      error = String(e);
      busy = false;
    }
  }

  function onkeydown(e: KeyboardEvent) {
    if (e.key === "Escape") void api.dismissMenu();
  }
</script>

<svelte:window {onkeydown} />

<div class="menu" class:glass role="menu">
  <button bind:this={button} role="menuitem" disabled={busy} onclick={create}>
    <span class="plus" aria-hidden="true">+</span>
    Create vial here
  </button>
  {#if error}<p class="error" role="alert">{error}</p>{/if}
</div>

<style>
  :global(html),
  :global(body) {
    margin: 0;
    height: 100%;
    overflow: hidden;
    user-select: none;
  }
  .menu {
    box-sizing: border-box;
    height: 100%;
    padding: 4px;
    border: 1px solid var(--border-strong);
    border-radius: 8px;
    background: var(--surface-1);
  }
  button {
    display: flex;
    align-items: center;
    gap: 10px;
    width: 100%;
    height: 100%;
    padding: 0 12px;
    border: none;
    border-radius: 5px;
    background: transparent;
    color: inherit;
    font-size: 14px;
    text-align: left;
    cursor: default;
  }
  button:hover,
  button:focus-visible {
    background: var(--accent-soft);
    outline: none;
  }
  .plus {
    width: 18px;
    text-align: center;
    color: var(--accent);
    font-size: 18px;
    line-height: 1;
  }
  .error {
    margin: 4px 8px;
    color: var(--error);
    font-size: 12px;
  }

  .menu.glass {
    border: 1px solid rgba(255, 255, 255, 0.28);
    border-radius: 12px;
    color: rgba(255, 255, 255, 0.95);
    background:
      radial-gradient(120% 90% at 0% 0%, rgba(255, 255, 255, 0.22), transparent 60%),
      linear-gradient(160deg, rgba(255, 255, 255, 0.1), rgba(255, 255, 255, 0.03));
    box-shadow:
      inset 0 1px 0 rgba(255, 255, 255, 0.55),
      inset 0 -1px 0 rgba(255, 255, 255, 0.1);
  }
  .glass button {
    border-radius: 9px;
    text-shadow: 0 1px 2px rgba(0, 0, 0, 0.4);
  }
  .glass button:hover,
  .glass button:focus-visible {
    background: rgba(255, 255, 255, 0.18);
    box-shadow: inset 0 1px 0 rgba(255, 255, 255, 0.35);
  }
</style>
