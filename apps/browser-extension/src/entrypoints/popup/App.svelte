<script lang="ts">
  import { onMount } from "svelte";
  import { describeStatus } from "../../shared/bridge-state";
  import { getStatus, onStatusChanged, type DesquStatus } from "../../shared/status";
  import { browser } from "wxt/browser";
  import { MODE_LABELS } from "../../shared/labels";
  import { resolveMode, MODES, type FullscreenMode, type SiteRules } from "../../shared/rules";
  import { getRules, saveRules } from "../../shared/storage";

  let url = $state<string | null>(null);
  let host = $state<string | null>(null);
  let rules = $state<SiteRules | null>(null);
  let status = $state<DesquStatus>({ state: "unknown" });

  const hostRule = $derived(
    rules?.rules.find((r) => r.pattern.type === "host" && r.pattern.value === host) ?? null,
  );
  const resolved = $derived(rules && url ? resolveMode(rules, url) : null);

  onMount(() => {
    void getStatus().then((s) => (status = s));
    return onStatusChanged((s) => (status = s));
  });

  onMount(async () => {
    const [tab] = await browser.tabs.query({ active: true, currentWindow: true });
    url = tab?.url ?? null;
    try {
      host = url ? new URL(url).hostname.toLowerCase() : null;
    } catch {
      host = null;
    }
    rules = await getRules();
  });

  async function setHostMode(mode: FullscreenMode | "default") {
    if (!rules || !host) return;
    const others = rules.rules.filter((r) => !(r.pattern.type === "host" && r.pattern.value === host));
    if (mode === "default") {
      rules = await saveRules({ ...rules, rules: others });
      return;
    }
    const rule = {
      id: hostRule?.id ?? crypto.randomUUID(),
      enabled: true,
      pattern: { type: "host" as const, value: host },
      mode,
      zoneId: null,
    };
    rules = await saveRules({ ...rules, rules: [rule, ...others.filter((r) => r.id !== rule.id)] });
  }

  function openOptions() {
    void browser.runtime.openOptionsPage();
  }
</script>

<main>
  <header>
    <strong>Desqu Fullscreen</strong>
    <span class="host">{host ?? "No website"}</span>
  </header>

  {#if !host}
    <p class="dim">Open a website to choose its fullscreen mode.</p>
  {:else if rules}
    <div class="choices" role="radiogroup" aria-label="Fullscreen mode for this site">
      <button
        type="button"
        role="radio"
        aria-checked={!hostRule}
        class:picked={!hostRule}
        onclick={() => setHostMode("default")}
      >
        <span class="name">Use default</span>
        <span class="hint">{MODE_LABELS[rules.defaultMode].name}</span>
      </button>
      {#each MODES as mode (mode)}
        <button
          type="button"
          role="radio"
          aria-checked={hostRule?.mode === mode}
          class:picked={hostRule?.mode === mode}
          onclick={() => setHostMode(mode)}
        >
          <span class="name">{MODE_LABELS[mode].name}</span>
          <span class="hint">{MODE_LABELS[mode].hint}</span>
        </button>
      {/each}
    </div>
    {#if resolved}
      <p class="dim">
        Now: {MODE_LABELS[resolved.mode].name}{resolved.ruleId ? " (site rule)" : " (default)"}
      </p>
    {/if}
  {/if}

  <p class="desqu">{describeStatus(status)}</p>
  <button type="button" class="link" onclick={openOptions}>Options</button>
</main>

<style>
  main {
    width: 320px;
    padding: 10px;
    font: 13px/1.35 system-ui, sans-serif;
  }
  header {
    display: flex;
    justify-content: space-between;
    align-items: baseline;
    gap: 8px;
    margin-bottom: 8px;
  }
  .host {
    color: #666;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    max-width: 150px;
  }
  .choices {
    display: flex;
    flex-direction: column;
    gap: 4px;
  }
  .choices button {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    text-align: left;
    padding: 6px 8px;
    border: 1px solid #ccc;
    border-radius: 6px;
    background: #fff;
    cursor: pointer;
  }
  .choices button.picked {
    border-color: #2a6fdb;
    background: #eaf1ff;
  }
  .name {
    font-weight: 600;
  }
  .hint {
    color: #555;
    font-size: 12px;
  }
  .dim {
    color: #555;
  }
  .desqu {
    margin: 10px 0 4px;
    color: #555;
    font-size: 12px;
  }
  .link {
    border: none;
    background: none;
    color: #2a6fdb;
    padding: 0;
    cursor: pointer;
  }
</style>
