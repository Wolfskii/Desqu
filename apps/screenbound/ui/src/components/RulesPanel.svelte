<script lang="ts">
  import GroupBadge from "./GroupBadge.svelte";
  import { Switch } from "@desqu/ui";
  import { app, newId } from "../state.svelte";
  import type { Action, ChromeMode, RuleScope, WindowRule } from "../types";

  type FullscreenAction = Extract<Action, { type: "fullscreenZone" }>;
  type BorderlessAction = Extract<Action, { type: "borderless" }>;

  function fullscreenAction(rule: WindowRule): FullscreenAction | undefined {
    return rule.actions.find((a): a is FullscreenAction => a.type === "fullscreenZone");
  }

  function borderlessAction(rule: WindowRule): BorderlessAction | undefined {
    return rule.actions.find((a): a is BorderlessAction => a.type === "borderless");
  }

  function kindOf(rule: WindowRule): "fullscreen" | "borderless" {
    return borderlessAction(rule) ? "borderless" : "fullscreen";
  }

  /** A rule has one action in this UI; switching kind replaces it with that kind's defaults. */
  function setKind(rule: WindowRule, kind: string) {
    if (kind === kindOf(rule)) return;
    const zoneId = app.config?.zones[0]?.id;
    if (kind === "borderless") {
      rule.actions = [{ type: "borderless", target: { type: "monitor" }, topmost: false }];
    } else if (zoneId) {
      rule.actions = [{ type: "fullscreenZone", zoneId, chrome: "keep", useSnappedZone: true }];
    } else {
      return;
    }
    app.scheduleSave(true);
  }

  function targetValue(action: BorderlessAction): string {
    return action.target.type === "zone" ? `zone:${action.target.zoneId}` : "monitor";
  }

  function setTarget(action: BorderlessAction, value: string) {
    action.target = value.startsWith("zone:") ? { type: "zone", zoneId: value.slice(5) } : { type: "monitor" };
    app.scheduleSave(true);
  }

  function scopeKind(rule: WindowRule): "legacy" | RuleScope["type"] {
    return rule.scope?.type ?? "legacy";
  }

  function setScopeKind(rule: WindowRule, kind: string) {
    if (kind === "legacy") return;
    if (kind === "all") rule.scope = { type: "all" };
    else if (kind === "groups") rule.scope = { type: "groups", groupIds: rule.scope?.type === "groups" ? rule.scope.groupIds : [] };
    else rule.scope = { type: "apps", processNames: rule.scope?.type === "apps" ? rule.scope.processNames : [] };
    rule.matcher = { type: "any", value: [] };
    app.scheduleSave(true);
  }

  function toggleId(list: string[], id: string): string[] {
    return list.includes(id) ? list.filter((item) => item !== id) : [...list, id];
  }

  function addRule() {
    const zoneId = app.config?.zones[0]?.id;
    if (!app.config || !zoneId) return;
    app.config.rules.push({
      id: newId("rule"),
      name: "New rule",
      enabled: true,
      matcher: { type: "any", value: [] },
      scope: { type: "all" },
      actions: [{ type: "fullscreenZone", zoneId, chrome: "keep", useSnappedZone: true }],
    });
    app.scheduleSave(true);
  }

  function addBorderlessRule() {
    if (!app.config) return;
    app.config.rules.push({
      id: newId("rule"),
      name: "New borderless rule",
      enabled: true,
      matcher: { type: "any", value: [] },
      scope: { type: "all" },
      actions: [{ type: "borderless", target: { type: "monitor" }, topmost: false }],
    });
    app.scheduleSave(true);
  }

  function removeRule(id: string) {
    if (!app.config) return;
    app.config.rules = app.config.rules.filter((r) => r.id !== id);
    app.scheduleSave(true);
  }

  function move(index: number, delta: number) {
    if (!app.config) return;
    const rules = app.config.rules;
    const target = index + delta;
    if (target < 0 || target >= rules.length) return;
    [rules[index], rules[target]] = [rules[target], rules[index]];
    app.scheduleSave(true);
  }
</script>

<div class="rules">
  <div class="head">
    <p class="dim">
      Rules are checked top to bottom; the first enabled match wins. A rule applies to apps that are turned on:
      every one of them, specific groups, or specific apps. Put a narrower rule above an All rule.
    </p>
    <div class="add">
      <button onclick={addRule} disabled={!app.config?.zones.length}>+ New rule</button>
      <button onclick={addBorderlessRule} title="For apps that run in a window, e.g. games">+ Borderless rule</button>
    </div>
  </div>

  {#each app.config?.rules ?? [] as rule, i (rule.id)}
    {@const fullscreen = fullscreenAction(rule)}
    {@const borderless = borderlessAction(rule)}
    <div class="card rule" class:disabled={!rule.enabled}>
      <div class="row">
        <label class="toggle">
          <input type="checkbox" bind:checked={rule.enabled} onchange={() => app.scheduleSave(true)} />
        </label>
        <input class="name" bind:value={rule.name} oninput={() => app.scheduleSave()} />
        <div class="spacer"></div>
        <button title="Move up" onclick={() => move(i, -1)} disabled={i === 0}>↑</button>
        <button title="Move down" onclick={() => move(i, 1)} disabled={i === (app.config?.rules.length ?? 0) - 1}>↓</button>
        <button class="danger" onclick={() => removeRule(rule.id)}>Delete</button>
      </div>

      <div class="grid">
        <label>
          Applies to
          <select value={scopeKind(rule)} onchange={(e) => setScopeKind(rule, e.currentTarget.value)}>
            {#if !rule.scope}<option value="legacy">Previous process list</option>{/if}
            <option value="all">All apps that are on</option>
            <option value="groups">Specific groups</option>
            <option value="apps">Specific apps</option>
          </select>
        </label>

        <label>
          Action
          <select value={kindOf(rule)} onchange={(e) => setKind(rule, e.currentTarget.value)}>
            <option value="fullscreen">Fullscreen → zone</option>
            <option value="borderless">Borderless window</option>
          </select>
        </label>

        {#if fullscreen}
          <label>
            Fullscreen zone
            <select bind:value={fullscreen.zoneId} onchange={() => app.scheduleSave(true)}>
              {#each app.config?.zones ?? [] as z (z.id)}
                <option value={z.id}>{z.name}</option>
              {/each}
            </select>
          </label>
          <label>
            Window chrome
            <select
              value={fullscreen.chrome}
              onchange={(e) => {
                fullscreen.chrome = e.currentTarget.value as ChromeMode;
                app.scheduleSave(true);
              }}
            >
              <option value="keep">Keep title bar (default)</option>
              <option value="hide">Hide title bar (borderless)</option>
            </select>
          </label>
        {:else if borderless}
          <label>
            Fit to
            <select value={targetValue(borderless)} onchange={(e) => setTarget(borderless, e.currentTarget.value)}>
              <option value="monitor">Whole monitor</option>
              {#each app.config?.zones ?? [] as z (z.id)}
                <option value="zone:{z.id}">Zone: {z.name}</option>
              {/each}
            </select>
          </label>
        {/if}
      </div>

      {#if fullscreen}
        <div class="option">
          <div class="info">
            <span>Use FancyZones zone</span>
            <span class="dim">
              A window snapped into a PowerToys FancyZones zone fills that zone when it goes fullscreen, instead of the
              zone above.
            </span>
          </div>
          <Switch
            checked={fullscreen.useSnappedZone ?? true}
            label="Use the FancyZones zone for snapped windows in {rule.name}"
            onchange={(next) => {
              fullscreen.useSnappedZone = next;
              app.scheduleSave(true);
            }}
          />
        </div>
      {:else if borderless}
        <div class="option">
          <div class="info">
            <span>Keep on top</span>
            <span class="dim">Keep the window above other windows while it is borderless.</span>
          </div>
          <Switch
            checked={borderless.topmost ?? false}
            label="Keep {rule.name} on top"
            onchange={(next) => {
              borderless.topmost = next;
              app.scheduleSave(true);
            }}
          />
        </div>
        <p class="dim hint">
          For apps that run in a window, such as many games: Screenbound removes the title bar and fits the window to the
          monitor or zone, and puts everything back when you release it or quit. Games in exclusive fullscreen, or running
          as administrator, cannot be changed.
        </p>
      {/if}

      {#if rule.scope?.type === "groups"}
        <div class="chips">
          {#each app.config?.groups ?? [] as group (group.id)}
            <button
              type="button"
              class="chip"
              class:on={rule.scope.groupIds.includes(group.id)}
              onclick={() => {
                if (rule.scope?.type !== "groups") return;
                rule.scope.groupIds = toggleId(rule.scope.groupIds, group.id);
                app.scheduleSave(true);
              }}
            >
              <GroupBadge icon={group.icon} color={group.color} size={18} />
              {group.name}
            </button>
          {:else}
            <span class="dim">No groups yet. Create them on the Groups tab.</span>
          {/each}
        </div>
      {:else if rule.scope?.type === "apps"}
        <div class="chips">
          {#each app.config?.knownApps ?? [] as known (known.processName)}
            <button
              type="button"
              class="chip"
              class:on={rule.scope.processNames.some((name) => name.toLowerCase() === known.processName.toLowerCase())}
              onclick={() => {
                if (rule.scope?.type !== "apps") return;
                const has = rule.scope.processNames.some((name) => name.toLowerCase() === known.processName.toLowerCase());
                rule.scope.processNames = has
                  ? rule.scope.processNames.filter((name) => name.toLowerCase() !== known.processName.toLowerCase())
                  : [...rule.scope.processNames, known.processName];
                app.scheduleSave(true);
              }}
            >
              {known.title && known.title.toLowerCase() !== known.processName.toLowerCase() ? known.title : known.processName}
            </button>
          {:else}
            <span class="dim">No apps seen yet. Open one, then refresh the Apps tab.</span>
          {/each}
        </div>
      {/if}
    </div>
  {/each}
</div>

<style>
  .rules {
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
  .rule.disabled {
    opacity: 0.6;
  }
  .row {
    display: flex;
    align-items: center;
    gap: 6px;
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
  .add {
    display: flex;
    gap: 6px;
    flex: none;
  }
  .hint {
    margin: 0;
  }
  .option {
    display: flex;
    align-items: center;
    gap: 12px;
  }
  .option .info {
    display: flex;
    flex-direction: column;
    gap: 2px;
    flex: 1;
  }
  .chips {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
  }
  .chip {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    background: transparent;
  }
  .chip.on {
    border-color: var(--accent);
    background: var(--accent-soft);
  }
</style>
