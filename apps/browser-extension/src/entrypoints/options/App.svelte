<script lang="ts">
  import { onMount } from "svelte";
  import { MODE_LABELS, LIMITATIONS } from "../../shared/labels";
  import {
    MODES,
    sanitizeRules,
    isSiteRules,
    type FullscreenMode,
    type SitePattern,
    type SiteRule,
    type SiteRules,
  } from "../../shared/rules";
  import { getRules, saveRules } from "../../shared/storage";
  import { describeStatus } from "../../shared/bridge-state";
  import { getStatus, onStatusChanged, type DesquStatus } from "../../shared/status";

  let doc = $state<SiteRules | null>(null);
  let status = $state<DesquStatus>({ state: "unknown" });

  onMount(async () => {
    doc = await getRules();
  });

  onMount(() => {
    void getStatus().then((s) => (status = s));
    return onStatusChanged((s) => (status = s));
  });

  async function persist(next: SiteRules) {
    doc = await saveRules(next);
  }

  function setDefaultMode(mode: FullscreenMode) {
    if (doc) void persist({ ...doc, defaultMode: mode });
  }

  function addRule() {
    if (!doc) return;
    const rule: SiteRule = {
      id: crypto.randomUUID(),
      enabled: true,
      pattern: { type: "domain", value: "" },
      mode: "window",
      zoneId: null,
    };
    void persist({ ...doc, rules: [...doc.rules, rule] });
  }

  function updateRule(id: string, change: Partial<Omit<SiteRule, "pattern">> & { pattern?: Partial<SitePattern> }) {
    if (!doc) return;
    const rules = doc.rules.map((r) => {
      if (r.id !== id) return r;
      const pattern = change.pattern ? ({ ...r.pattern, ...change.pattern } as SitePattern) : r.pattern;
      return { ...r, ...change, pattern };
    });
    void persist({ ...doc, rules });
  }

  function deleteRule(id: string) {
    if (doc) void persist({ ...doc, rules: doc.rules.filter((r) => r.id !== id) });
  }

  async function exportRules() {
    if (!doc) return;
    const blob = new Blob([JSON.stringify(doc, null, 2)], { type: "application/json" });
    const a = document.createElement("a");
    a.href = URL.createObjectURL(blob);
    a.download = "desqu-fullscreen-rules.json";
    a.click();
    URL.revokeObjectURL(a.href);
  }

  async function importRules(event: Event) {
    const file = (event.currentTarget as HTMLInputElement).files?.[0];
    if (!file) return;
    const parsed: unknown = JSON.parse(await file.text());
    if (isSiteRules(parsed)) await persist(sanitizeRules(parsed));
  }
</script>

<main>
  <h1>Desqu Fullscreen</h1>

  {#if doc}
    <section>
      <h2>Default for all sites</h2>
      <div class="modes" role="radiogroup" aria-label="Default fullscreen mode">
        {#each MODES as mode (mode)}
          <button
            type="button"
            role="radio"
            aria-checked={doc.defaultMode === mode}
            class:picked={doc.defaultMode === mode}
            onclick={() => setDefaultMode(mode)}
          >
            <span class="name">{MODE_LABELS[mode].name}</span>
            <span class="hint">{MODE_LABELS[mode].hint}</span>
          </button>
        {/each}
      </div>
    </section>

    <section>
      <h2>Rules for specific sites</h2>
      <p class="dim">The most specific rule wins: a full URL beats a host, and a host beats a domain (with its subdomains).</p>
      <table>
        <thead>
          <tr><th>On</th><th>Pattern</th><th>Value</th><th>Mode</th><th></th></tr>
        </thead>
        <tbody>
          {#each doc.rules as rule (rule.id)}
            <tr>
              <td><input type="checkbox" checked={rule.enabled} onchange={(e) => updateRule(rule.id, { enabled: e.currentTarget.checked })} /></td>
              <td>
                <select value={rule.pattern.type} onchange={(e) => updateRule(rule.id, { pattern: { type: e.currentTarget.value as SitePattern["type"] } })}>
                  <option value="domain">Domain and subdomains</option>
                  <option value="host">Exact host</option>
                  <option value="urlPrefix">URL starts with</option>
                </select>
              </td>
              <td>
                <input
                  type="text"
                  value={rule.pattern.value}
                  placeholder={rule.pattern.type === "urlPrefix" ? "https://www.example.com/watch" : "example.com"}
                  onchange={(e) => updateRule(rule.id, { pattern: { value: e.currentTarget.value } })}
                />
              </td>
              <td>
                <select value={rule.mode} onchange={(e) => updateRule(rule.id, { mode: e.currentTarget.value as FullscreenMode })}>
                  {#each MODES as mode (mode)}
                    <option value={mode}>{MODE_LABELS[mode].name}</option>
                  {/each}
                </select>
              </td>
              <td><button type="button" onclick={() => deleteRule(rule.id)}>Delete</button></td>
            </tr>
          {:else}
            <tr><td colspan="5" class="dim">No site rules yet. Every site uses the default.</td></tr>
          {/each}
        </tbody>
      </table>
      <div class="row">
        <button type="button" onclick={addRule}>Add rule</button>
        <button type="button" onclick={exportRules}>Export</button>
        <label class="file">Import <input type="file" accept="application/json" onchange={importRules} /></label>
      </div>
    </section>
  {:else}
    <p class="dim">Loading…</p>
  {/if}

  <section>
    <h2>Desqu</h2>
    <p>{describeStatus(status)} Desqu is optional: everything else works without it.</p>
    <p><a href="https://github.com/Wolfskii/Desqu/releases" target="_blank" rel="noreferrer">Get Desqu</a></p>
  </section>

  <section>
    <h2>Limitations</h2>
    <ul>
      {#each LIMITATIONS as line (line)}
        <li>{line}</li>
      {/each}
    </ul>
  </section>
</main>

<style>
  main {
    max-width: 760px;
    margin: 24px auto;
    padding: 0 16px;
    font: 14px/1.45 system-ui, sans-serif;
  }
  section {
    margin-bottom: 24px;
  }
  .modes {
    display: grid;
    grid-template-columns: repeat(2, 1fr);
    gap: 8px;
  }
  .modes button {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    text-align: left;
    padding: 8px 10px;
    border: 1px solid #ccc;
    border-radius: 6px;
    background: #fff;
    cursor: pointer;
  }
  .modes button.picked {
    border-color: #2a6fdb;
    background: #eaf1ff;
  }
  .name {
    font-weight: 600;
  }
  .hint,
  .dim {
    color: #555;
    font-size: 13px;
  }
  table {
    width: 100%;
    border-collapse: collapse;
  }
  th,
  td {
    text-align: left;
    padding: 4px 6px;
    vertical-align: middle;
  }
  td input[type="text"] {
    width: 100%;
    box-sizing: border-box;
  }
  .row {
    display: flex;
    gap: 8px;
    align-items: center;
    margin-top: 8px;
  }
  .file input {
    display: none;
  }
  .file {
    cursor: pointer;
    text-decoration: underline;
  }
</style>
