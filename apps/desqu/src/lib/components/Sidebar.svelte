<script lang="ts">
  import { page } from "$app/state";
  import { desqu } from "$lib/state.svelte";
  import { statusText } from "$lib/types";

  const path = $derived(page.url.pathname);
  const modules = $derived(desqu.supported);
</script>

<nav aria-label="Desqu">
  <a class="brand" href="/">Desqu</a>

  <a class="item" class:active={path === "/"} href="/">Home</a>

  <div class="group">Apps</div>
  {#each modules as m (m.id)}
    <a class="item" class:active={path === `/${m.id}`} href="/{m.id}">
      <span
        class="dot"
        class:on={m.status === "on"}
        class:paused={m.status === "paused"}
        class:elsewhere={m.status === "ownedElsewhere"}
        title={statusText(m.status)}
      ></span>
      {m.name}
    </a>
  {/each}

  <div class="spacer"></div>
  <a class="item" class:active={path === "/settings"} href="/settings">Settings</a>
</nav>

<style>
  nav {
    display: flex;
    flex-direction: column;
    gap: 2px;
    width: 190px;
    flex: none;
    padding: 12px 8px;
    background: var(--surface-1);
    border-right: 1px solid var(--border);
  }
  .brand {
    padding: 4px 10px 12px;
    font-size: 16px;
    font-weight: 700;
    letter-spacing: 0.3px;
    color: var(--text);
    text-decoration: none;
  }
  .group {
    padding: 12px 10px 4px;
    font-size: 11px;
    text-transform: uppercase;
    letter-spacing: 0.6px;
    color: var(--text-dim);
  }
  .item {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 7px 10px;
    border-radius: 6px;
    border: 1px solid transparent;
    color: var(--text);
    text-decoration: none;
  }
  .item:hover {
    background: var(--surface-2);
  }
  .item:focus-visible {
    outline: 2px solid var(--accent);
    outline-offset: -2px;
  }
  .item.active {
    background: var(--surface-2);
    border-color: var(--border-strong);
  }
  .dot {
    width: 8px;
    height: 8px;
    border-radius: 50%;
    background: var(--border-strong);
  }
  .dot.on {
    background: #4cc38a;
  }
  .dot.paused {
    background: var(--warn);
  }
  .dot.elsewhere {
    background: var(--accent);
  }
  .spacer {
    flex: 1;
  }
</style>
