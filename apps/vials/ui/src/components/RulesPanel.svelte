<script lang="ts">
  import { Switch } from "@desqu/ui";
  import { api } from "../api";
  import { vials } from "../state.svelte";

  const vialName = (id: string) => vials.config?.vials.find((v) => v.id === id)?.name ?? "a deleted vial";
</script>

<div class="panel">
  <div class="head">
    <p class="dim">
      Rules put <em>new</em> desktop items into a vial by themselves, and items that already match are moved in as soon
      as a rule is saved. Items you took out of a vial on purpose stay out. Edit them in the rules window.
    </p>
    <button onclick={() => api.openRules(null)}>Edit rules…</button>
  </div>

  {#if vials.config && !vials.config.autoSort}
    <p class="warn">Automatic sorting is off under Settings, so no rule does anything yet.</p>
  {/if}

  {#each vials.config?.rules ?? [] as rule (rule.id)}
    <div class="card row" class:off={!rule.enabled}>
      <Switch
        checked={rule.enabled}
        label="Enable {rule.name}"
        onchange={(next) => {
          rule.enabled = next;
          vials.scheduleSave(true);
        }}
      />
      <div class="text">
        <strong>{rule.name}</strong>
        <span class="dim">→ {vialName(rule.targetVial)}</span>
      </div>
    </div>
  {:else}
    <p class="dim">No rules yet. Make one with “Edit rules…”, or from a vial's right-click menu.</p>
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
  .row {
    display: flex;
    align-items: center;
    gap: 10px;
  }
  .row.off {
    opacity: 0.6;
  }
  .text {
    display: flex;
    flex-direction: column;
  }
  .warn {
    margin: 0;
    color: var(--warning, #e0a040);
  }
</style>
