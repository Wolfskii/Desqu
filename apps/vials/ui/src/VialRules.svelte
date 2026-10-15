<script lang="ts">
  // The window for automatic rules: what goes into which vial by itself. Opened from a vial's menu, or
  // from the Rules tab. A rule is a list of conditions that all (or any) have to match; the first
  // matching rule, checked top to bottom, puts the item into its vial.
  import { onDestroy, onMount } from "svelte";
  import { Switch } from "@desqu/ui";
  import { api, events, type UnlistenFn } from "./api";
  import { newId } from "./state.svelte";
  import {
    CONDITION_HINTS,
    CONDITION_KINDS,
    CONDITION_LABELS,
    KINDS,
    describe,
    draftFrom,
    draftMatcher,
    emptyCondition,
    matches,
    type CondKind,
    type RuleDraft,
  } from "./rules-model";
  import { KIND_LABELS, type ItemInfo, type VialsConfig } from "./types";

  let { initialVial = null }: { initialVial?: string | null } = $props();

  let config = $state<VialsConfig | null>(null);
  let items = $state<ItemInfo[]>([]);
  let draft = $state<RuleDraft | null>(null);
  /** A vial a new rule should go to (from the vial's menu). */
  let wantedVial = $state<string | null>(null);
  let notice = $state<string | null>(null);
  let error = $state<string | null>(null);
  let noticeTimer: ReturnType<typeof setTimeout> | null = null;
  let unlisten: UnlistenFn[] = [];

  const targets = $derived((config?.vials ?? []).filter((v) => !v.inbox));
  const rules = $derived(config?.rules ?? []);
  const matcher = $derived(draft ? draftMatcher(draft) : null);
  const preview = $derived(matcher ? items.filter((i) => !i.missing && matches(matcher, i)) : []);

  function say(message: string) {
    notice = message;
    if (noticeTimer) clearTimeout(noticeTimer);
    noticeTimer = setTimeout(() => (notice = null), 3000);
  }

  async function load() {
    try {
      const [c, list] = await Promise.all([api.getConfig(), api.listItems()]);
      config = c;
      items = list;
      error = null;
    } catch (e) {
      error = String(e);
    }
  }

  /** Changes the saved config: always from a fresh copy, so nothing another window saved is lost. */
  async function persist(change: (cfg: VialsConfig) => void, message?: string): Promise<VialsConfig | null> {
    try {
      const fresh = await api.getConfig();
      change(fresh);
      config = await api.setConfig(fresh);
      error = null;
      if (message) say(message);
      return config;
    } catch (e) {
      error = String(e);
      return null;
    }
  }

  function select(id: string) {
    const rule = rules.find((r) => r.id === id);
    if (rule) draft = draftFrom(rule);
  }

  function newRule() {
    const target = targets.find((v) => v.id === wantedVial)?.id ?? targets[0]?.id;
    if (!target) {
      error = "Make a vial first: the rules put items into vials.";
      return;
    }
    draft = { id: null, name: "", enabled: true, targetVial: target, mode: "all", conditions: [emptyCondition("extension")] };
  }

  async function saveDraft() {
    if (!draft) return;
    const m = draftMatcher(draft);
    if (!m) {
      error = "Fill in every condition first.";
      return;
    }
    const d = draft;
    const id = d.id ?? newId("rule");
    const name = d.name.trim() || describe(d);
    const saved = await persist((cfg) => {
      const existing = cfg.rules.find((r) => r.id === id);
      if (existing) {
        existing.name = name;
        existing.enabled = d.enabled;
        existing.targetVial = d.targetVial;
        existing.matcher = m;
      } else {
        cfg.rules.push({ id, name, enabled: d.enabled, matcher: m, targetVial: d.targetVial });
      }
    }, "Rule saved");
    if (saved) select(id);
  }

  async function remove(id: string) {
    await persist((cfg) => {
      cfg.rules = cfg.rules.filter((r) => r.id !== id);
    }, "Rule deleted");
    if (draft?.id === id) draft = null;
  }

  async function toggle(id: string, on: boolean) {
    await persist((cfg) => {
      const r = cfg.rules.find((x) => x.id === id);
      if (r) r.enabled = on;
    });
  }

  async function move(index: number, delta: number) {
    await persist((cfg) => {
      const target = index + delta;
      if (target < 0 || target >= cfg.rules.length) return;
      [cfg.rules[index], cfg.rules[target]] = [cfg.rules[target], cfg.rules[index]];
    });
  }

  async function turnOnSorting() {
    await persist((cfg) => {
      cfg.autoSort = true;
    }, "Automatic sorting is on");
  }

  /** Back to the saved rule, or away from a new one. */
  function revert() {
    const saved = draft?.id ? rules.find((r) => r.id === draft?.id) : undefined;
    draft = saved ? draftFrom(saved) : null;
  }

  function addCondition() {
    if (draft) draft.conditions.push(emptyCondition("contains"));
  }

  function removeCondition(i: number) {
    if (draft && draft.conditions.length > 1) draft.conditions.splice(i, 1);
  }

  function hintFor(kind: CondKind): string {
    return kind === "advanced" || kind === "kind" ? "" : CONDITION_HINTS[kind];
  }

  onMount(() => {
    void load();
    // The vial's menu asks for a new rule while this window is open.
    void events
      .onRulesOpen((vial) => {
        wantedVial = vial;
        newRule();
      })
      .then((un) => unlisten.push(un));
    void events.onChanged(() => void load()).then((un) => unlisten.push(un));
    wantedVial = initialVial;
    if (initialVial) newRule();
  });

  onDestroy(() => {
    unlisten.forEach((u) => u());
    if (noticeTimer) clearTimeout(noticeTimer);
  });
</script>

<div class="rules">
  <header>
    <h1>Automatic rules</h1>
    <p class="dim">
      A rule puts new desktop items into a vial by itself, and items that already match are moved in as soon as the
      rule is saved. Rules are checked top to bottom and the first match wins. Items you took out of a vial on purpose
      stay out.
    </p>
  </header>

  {#if config && !config.autoSort}
    <div class="banner" role="status">
      Automatic sorting is off, so no rule does anything yet.
      <button onclick={turnOnSorting}>Turn it on</button>
    </div>
  {/if}

  {#if error}<p class="error" role="alert">{error} <button onclick={() => (error = null)}>Dismiss</button></p>{/if}
  {#if notice}<p class="notice" role="status">{notice}</p>{/if}

  <section class="card">
    <div class="head">
      <h2>Rules</h2>
      <button onclick={newRule} disabled={targets.length === 0}>+ New rule</button>
    </div>
    {#each rules as rule, i (rule.id)}
      <div class="rule-row" class:selected={draft?.id === rule.id} class:off={!rule.enabled}>
        <Switch checked={rule.enabled} label="Enable {rule.name}" onchange={(on) => toggle(rule.id, on)} />
        <button class="rule-name" onclick={() => select(rule.id)}>
          <strong>{rule.name}</strong>
          <span class="dim">→ {config?.vials.find((v) => v.id === rule.targetVial)?.name ?? "a deleted vial"}</span>
        </button>
        <button title="Move up" onclick={() => move(i, -1)} disabled={i === 0}>↑</button>
        <button title="Move down" onclick={() => move(i, 1)} disabled={i === rules.length - 1}>↓</button>
      </div>
    {:else}
      <p class="dim">No rules yet. Make one with “+ New rule”, or from a vial's right-click menu.</p>
    {/each}
  </section>

  {#if draft}
    <section class="card editor">
      <h2>{draft.id ? "Edit rule" : "New rule"}</h2>

      <label class="field">
        Name
        <input bind:value={draft.name} placeholder={describe(draft)} />
      </label>

      <label class="field">
        Put matching items into
        <select bind:value={draft.targetVial}>
          {#each targets as v (v.id)}
            <option value={v.id}>{v.name}</option>
          {/each}
        </select>
      </label>

      <div class="field">
        <span>An item matches when</span>
        <select bind:value={draft.mode} aria-label="All or any of the conditions">
          <option value="all">all of these conditions are true</option>
          <option value="any">any of these conditions is true</option>
        </select>
      </div>

      <div class="conditions">
        {#each draft.conditions as c, i (i)}
          <div class="condition">
            {#if c.kind === "advanced"}
              <span class="dim">A condition that is only editable in the config file</span>
            {:else}
              <select bind:value={c.kind} aria-label="Condition">
                {#each CONDITION_KINDS as k (k)}
                  <option value={k}>{CONDITION_LABELS[k]}</option>
                {/each}
              </select>
              {#if c.kind === "kind"}
                <select bind:value={c.itemKind} aria-label="Kind">
                  {#each KINDS as k (k)}
                    <option value={k}>{KIND_LABELS[k]}</option>
                  {/each}
                </select>
              {:else}
                <input bind:value={c.text} placeholder={hintFor(c.kind)} aria-label="Condition text" />
              {/if}
            {/if}
            <button class="x" onclick={() => removeCondition(i)} disabled={draft.conditions.length === 1} aria-label="Remove condition">
              ✕
            </button>
          </div>
        {/each}
        <button onclick={addCondition}>+ Add condition</button>
      </div>

      <div class="preview">
        {#if !matcher}
          <p class="dim">Fill in the conditions to see what they catch.</p>
        {:else if preview.length === 0}
          <p class="dim">No item on your desktop matches right now.</p>
        {:else}
          <p class="dim">{preview.length} {preview.length === 1 ? "item" : "items"} on your desktop match now:</p>
          <ul>
            {#each preview.slice(0, 12) as item (item.id)}
              <li>{item.name}</li>
            {/each}
          </ul>
          {#if preview.length > 12}<p class="dim">…and {preview.length - 12} more</p>{/if}
        {/if}
      </div>

      <div class="actions">
        <button class="primary" onclick={saveDraft} disabled={!matcher}>Save rule</button>
        {#if draft.id}
          <button class="danger" onclick={() => draft && remove(draft.id!)}>Delete</button>
        {/if}
        <div class="spacer"></div>
        <button onclick={revert}>{draft.id ? "Revert" : "Cancel"}</button>
      </div>
    </section>
  {/if}
</div>

<style>
  .rules {
    box-sizing: border-box;
    height: 100vh;
    overflow: auto;
    padding: 16px;
    display: flex;
    flex-direction: column;
    gap: 14px;
  }
  header h1 {
    margin: 0 0 4px;
    font-size: 20px;
  }
  header p {
    margin: 0;
    font-size: 13px;
  }
  h2 {
    margin: 0;
    font-size: 15px;
  }
  .card {
    display: flex;
    flex-direction: column;
    gap: 10px;
  }
  .head,
  .actions {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .spacer {
    flex: 1;
  }
  .banner {
    padding: 8px 10px;
    border-radius: 6px;
    background: rgba(255, 190, 90, 0.14);
    display: flex;
    align-items: center;
    gap: 10px;
    font-size: 13px;
  }
  .error {
    color: var(--error);
    margin: 0;
  }
  .notice {
    margin: 0;
    font-size: 13px;
  }
  .rule-row {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 6px;
    border-radius: 6px;
  }
  .rule-row.selected {
    background: var(--surface-2);
  }
  .rule-row.off {
    opacity: 0.55;
  }
  .rule-name {
    flex: 1;
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    text-align: left;
    background: transparent;
    border-color: transparent;
  }
  .editor {
    padding-top: 6px;
    border-top: 1px solid var(--border);
  }
  .field {
    display: flex;
    flex-direction: column;
    gap: 4px;
    font-size: 13px;
  }
  .conditions {
    display: flex;
    flex-direction: column;
    gap: 8px;
    align-items: flex-start;
  }
  .condition {
    display: flex;
    align-items: center;
    gap: 6px;
    width: 100%;
  }
  .condition input {
    flex: 1;
    min-width: 0;
  }
  .x {
    background: transparent;
    border-color: transparent;
  }
  .preview ul {
    margin: 4px 0;
    padding-left: 18px;
    font-size: 13px;
    max-height: 180px;
    overflow: auto;
  }
  .preview p {
    margin: 0;
    font-size: 13px;
  }
</style>
