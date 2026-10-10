<script lang="ts">
  import { Switch } from "@desqu/ui";
  import { vials } from "../state.svelte";
  import { KIND_LABELS, type ItemKind, type ItemMatcher, type SortRule } from "../types";

  const kinds = Object.keys(KIND_LABELS) as ItemKind[];

  /** The simple matchers this editor can change; combined ones are shown read-only. */
  function simpleKind(rule: SortRule): "extension" | "kind" | "nameGlob" | "complex" {
    const t = rule.matcher.type;
    return t === "extension" || t === "kind" || t === "nameGlob" ? t : "complex";
  }

  function setKind(rule: SortRule, kind: string) {
    const matcher: ItemMatcher =
      kind === "kind"
        ? { type: "kind", value: "shortcut" }
        : kind === "nameGlob"
          ? { type: "nameGlob", value: "*" }
          : { type: "extension", value: "pdf" };
    rule.matcher = matcher;
    vials.scheduleSave(true);
  }

  function setValue(rule: SortRule, value: string) {
    if (rule.matcher.type === "extension" || rule.matcher.type === "nameGlob") {
      rule.matcher.value = value;
    } else if (rule.matcher.type === "kind") {
      rule.matcher.value = value as ItemKind;
    }
    vials.scheduleSave();
  }
</script>

<div class="panel">
  <div class="head">
    <p class="dim">
      Rules put <em>new</em> desktop items into a vial automatically. They are checked top to bottom and the first
      match wins. Items you already placed, or took out on purpose, are never moved. Switch this off under Settings.
    </p>
    <button onclick={() => vials.addRule()} disabled={!vials.config?.vials.length}>+ New rule</button>
  </div>

  {#each vials.config?.rules ?? [] as rule, i (rule.id)}
    <div class="card rule" class:off={!rule.enabled}>
      <div class="row">
        <Switch
          checked={rule.enabled}
          label="Enable {rule.name}"
          onchange={(next) => {
            rule.enabled = next;
            vials.scheduleSave(true);
          }}
        />
        <input class="name" bind:value={rule.name} oninput={() => vials.scheduleSave()} aria-label="Rule name" />
        <div class="spacer"></div>
        <button title="Move up" onclick={() => vials.moveRule(i, -1)} disabled={i === 0}>↑</button>
        <button
          title="Move down"
          onclick={() => vials.moveRule(i, 1)}
          disabled={i === (vials.config?.rules.length ?? 0) - 1}>↓</button
        >
        <button class="danger" onclick={() => vials.removeRule(rule.id)}>Delete</button>
      </div>

      <div class="grid">
        {#if simpleKind(rule) === "complex"}
          <p class="dim">This rule combines several conditions. It works, but is edited in the config file.</p>
        {:else}
          <label>
            When an item…
            <select value={simpleKind(rule)} onchange={(e) => setKind(rule, e.currentTarget.value)}>
              <option value="extension">has the extension</option>
              <option value="kind">is a</option>
              <option value="nameGlob">has a name matching</option>
            </select>
          </label>
          <label>
            {rule.matcher.type === "extension" ? "Extension (without the dot)" : rule.matcher.type === "kind" ? "Kind" : "Pattern (* and ? work)"}
            {#if rule.matcher.type === "kind"}
              <select value={rule.matcher.value} onchange={(e) => setValue(rule, e.currentTarget.value)}>
                {#each kinds as k (k)}
                  <option value={k}>{KIND_LABELS[k]}</option>
                {/each}
              </select>
            {:else}
              <input value={rule.matcher.value} oninput={(e) => setValue(rule, e.currentTarget.value)} />
            {/if}
          </label>
        {/if}
        <label>
          …put it in
          <select
            value={rule.targetVial}
            onchange={(e) => {
              rule.targetVial = e.currentTarget.value;
              vials.scheduleSave(true);
            }}
          >
            {#each vials.config?.vials ?? [] as v (v.id)}
              <option value={v.id}>{v.name}</option>
            {/each}
          </select>
        </label>
      </div>
    </div>
  {:else}
    <p class="dim">No rules. Items only go into a vial when you place them yourself.</p>
  {/each}
</div>

<style>
  .panel {
    display: flex;
    flex-direction: column;
    gap: 10px;
  }
  .head {
    display: flex;
    justify-content: space-between;
    align-items: center;
    gap: 12px;
  }
  .head p {
    margin: 0;
  }
  .rule {
    display: flex;
    flex-direction: column;
    gap: 10px;
  }
  .rule.off {
    opacity: 0.6;
  }
  .row {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .name {
    font-weight: 600;
    min-width: 220px;
  }
  .spacer {
    flex: 1;
  }
  .grid {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(200px, 1fr));
    gap: 10px;
  }
  .grid p {
    margin: 0;
  }
</style>
