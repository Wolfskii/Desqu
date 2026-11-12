<script lang="ts">
  import { app } from "../state.svelte";
  import { BROWSER_INSTALLS, UNSUPPORTED_BROWSERS } from "../browserStores";
  import type { FullscreenMode, SitePattern, SiteRule, SiteRules } from "../types";

  const modes: { id: FullscreenMode; label: string; hint: string }[] = [
    { id: "native", label: "Browser default", hint: "The browser fullscreens the page. Desqu still limits it, with a short flash." },
    { id: "fullscreen", label: "Real fullscreen", hint: "The browser fullscreens the page. Desqu leaves it alone." },
    { id: "window", label: "In window", hint: "The video fills the browser tab. The window does not change." },
    { id: "zone", label: "Desqu zone", hint: "The browser window moves into a zone. Needs the extension and this app running." },
  ];

  const status = $derived(app.browserStatus);
  const integrationOn = $derived(app.config?.browser?.integrationEnabled ?? true);
  const siteRules = $derived<SiteRules | null>(app.config?.browser?.siteRules ?? null);
  const zones = $derived(app.config?.zones ?? []);

  /** Editing starts by making sure the document exists, then stamps it after every change. */
  function editable(): SiteRules | null {
    if (!app.config) return null;
    if (!app.config.browser?.siteRules) app.touchSiteRules();
    return app.config.browser?.siteRules ?? null;
  }

  function commit() {
    app.touchSiteRules();
  }

  function setDefaultMode(mode: FullscreenMode) {
    const rules = editable();
    if (!rules) return;
    rules.defaultMode = mode;
    commit();
  }

  function setDefaultZone(zoneId: string) {
    const rules = editable();
    if (!rules) return;
    rules.defaultZoneId = zoneId || null;
    commit();
  }

  function addRule() {
    const rules = editable();
    if (!rules) return;
    const rule: SiteRule = {
      id: crypto.randomUUID(),
      enabled: true,
      pattern: { type: "domain", value: "" },
      mode: "window",
      zoneId: null,
    };
    rules.rules.push(rule);
    commit();
  }

  function removeRule(id: string) {
    const rules = editable();
    if (!rules) return;
    rules.rules = rules.rules.filter((r) => r.id !== id);
    commit();
  }

  function patchRule(id: string, change: (rule: SiteRule) => void) {
    const rules = editable();
    const rule = rules?.rules.find((r) => r.id === id);
    if (!rule) return;
    change(rule);
    commit();
  }

  function setPatternType(rule: SiteRule, type: SitePattern["type"]) {
    rule.pattern = { type, value: rule.pattern.value };
  }

  const connectedNames = $derived(new Set((status?.connections ?? []).map((c) => c.browserName)));
</script>

<div class="browsers">
  <section class="card">
    <h3>Browser add-on (optional)</h3>
    <p class="dim">
      An optional add-on for your browser. Without it, a browser fullscreens a video by resizing the whole window, and
      Screenbound then shrinks it back, which causes a flash. With the add-on, the video stays in its window or moves
      into a zone instead. Each browser needs the Screenbound Companion add-on installed.
    </p>
    <label class="row">
      <input
        type="checkbox"
        checked={integrationOn}
        disabled={!app.config}
        onchange={(e) => app.setBrowserIntegration(e.currentTarget.checked)}
      />
      Browser integration
    </label>
    {#if status}
      {#if status.hostRegistered}
        <p class="ok">The browser host is registered{status.hostPath ? `: ${status.hostPath}` : ""}.</p>
      {:else if integrationOn}
        <p class="error-text">{status.hostProblem ?? "The browser host is not registered yet."}</p>
      {/if}
    {/if}
  </section>

  <section class="card">
    <h3>Install the extension</h3>
    <table>
      <tbody>
        {#each BROWSER_INSTALLS as browser (browser.name)}
          <tr>
            <td>{browser.name}</td>
            <td>
              {#if connectedNames.has(browser.name)}
                <span class="ok">Connected</span>
              {:else if browser.url}
                <a href={browser.url} target="_blank" rel="noreferrer">Get the extension</a>
              {:else}
                <span class="dim">Store listing coming soon. Load the unpacked extension (see the Desqu docs).</span>
              {/if}
            </td>
          </tr>
        {/each}
        {#each UNSUPPORTED_BROWSERS as name (name)}
          <tr>
            <td>{name}</td>
            <td><span class="dim">Not available: Desqu runs on Windows only.</span></td>
          </tr>
        {/each}
      </tbody>
    </table>
  </section>

  <section class="card">
    <h3>Connected browsers</h3>
    {#if status && status.connections.length > 0}
      <ul>
        {#each status.connections as c (c.connId)}
          <li>{c.browserName} {c.browserVersion} · extension {c.extensionVersion}</li>
        {/each}
      </ul>
    {:else}
      <p class="dim">No browser is connected right now.</p>
    {/if}
  </section>

  {#if siteRules}
    <section class="card">
      <h3>Site rules</h3>
      <p class="dim">
        The most specific rule wins: a full URL beats a host, and a host beats a domain (with its subdomains). The
        extension gets these rules within a second.
      </p>
      <div class="field">
        <span>Default for all sites</span>
        <div class="modes" role="radiogroup" aria-label="Default fullscreen mode">
          {#each modes as mode (mode.id)}
            <button
              type="button"
              role="radio"
              aria-checked={siteRules.defaultMode === mode.id}
              class:picked={siteRules.defaultMode === mode.id}
              title={mode.hint}
              onclick={() => setDefaultMode(mode.id)}
            >{mode.label}</button>
          {/each}
        </div>
        {#if zones.length > 0}
          <label class="row">
            Default zone
            <select value={siteRules.defaultZoneId ?? ""} onchange={(e) => setDefaultZone(e.currentTarget.value)}>
              <option value="">First zone</option>
              {#each zones as zone (zone.id)}<option value={zone.id}>{zone.name}</option>{/each}
            </select>
          </label>
        {/if}
      </div>

      <table>
        <thead><tr><th>On</th><th>Match</th><th>Value</th><th>Mode</th><th></th></tr></thead>
        <tbody>
          {#each siteRules.rules as rule (rule.id)}
            <tr>
              <td><input type="checkbox" checked={rule.enabled !== false} onchange={(e) => patchRule(rule.id, (r) => (r.enabled = e.currentTarget.checked))} /></td>
              <td>
                <select value={rule.pattern.type} onchange={(e) => patchRule(rule.id, (r) => setPatternType(r, e.currentTarget.value as SitePattern["type"]))}>
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
                  onchange={(e) => patchRule(rule.id, (r) => (r.pattern = { type: r.pattern.type, value: e.currentTarget.value }))}
                />
              </td>
              <td>
                <select value={rule.mode} onchange={(e) => patchRule(rule.id, (r) => (r.mode = e.currentTarget.value as FullscreenMode))}>
                  {#each modes as mode (mode.id)}<option value={mode.id}>{mode.label}</option>{/each}
                </select>
              </td>
              <td><button type="button" onclick={() => removeRule(rule.id)}>Delete</button></td>
            </tr>
          {:else}
            <tr><td colspan="5" class="dim">No site rules yet. Every site uses the default.</td></tr>
          {/each}
        </tbody>
      </table>
      <button type="button" onclick={addRule}>Add rule</button>
    </section>
  {/if}

  <section class="card">
    <h3>Without the extension</h3>
    <p class="dim">When a website goes fullscreen, the browser first fills the whole monitor, then Desqu shrinks it into your zone.</p>
    <ul>
      <li>You see a short flash on entering and leaving fullscreen, and the taskbar may flicker.</li>
      <li>Desqu cannot tell which website is fullscreen, so every site gets the same rule.</li>
      <li>You cannot let one site use real fullscreen while others stay in the zone.</li>
    </ul>
  </section>
</div>

<style>
  .browsers {
    display: flex;
    flex-direction: column;
    gap: 10px;
    max-width: 760px;
  }
  .card h3 {
    margin: 0 0 6px;
  }
  .card p {
    margin: 0 0 10px;
  }
  .row {
    display: flex;
    align-items: center;
    gap: 8px;
    margin: 6px 0;
  }
  .field {
    display: flex;
    flex-direction: column;
    gap: 6px;
    margin-bottom: 10px;
  }
  .modes {
    display: grid;
    grid-template-columns: repeat(4, 1fr);
    gap: 6px;
  }
  .modes button.picked {
    border-color: var(--accent);
    background: var(--accent-soft);
  }
  table {
    width: 100%;
    border-collapse: collapse;
  }
  td,
  th {
    text-align: left;
    padding: 4px 6px;
    vertical-align: middle;
  }
  td input[type="text"] {
    width: 100%;
    box-sizing: border-box;
  }
  .ok {
    color: var(--ok, #3fb950);
  }
</style>
