<script lang="ts">
  import { onDestroy, onMount } from "svelte";
  import SettingsRow from "./SettingsRow.svelte";
  import { shellApi, updateApi, type AppInfo, type ShellSettings, type UpdateStatus } from "./shell-api";
  import Switch from "./Switch.svelte";

  let info = $state<AppInfo | null>(null);
  let settings = $state<ShellSettings | null>(null);
  let status = $state<UpdateStatus | null>(null);
  let busy = $state<"checking" | "installing" | null>(null);
  let message = $state<string | null>(null);
  let unlisten: (() => void) | null = null;

  onMount(async () => {
    try {
      [info, settings, status] = await Promise.all([
        shellApi.appInfo(),
        shellApi.getShellSettings(),
        updateApi.status(),
      ]);
      unlisten = await updateApi.onAvailable(async () => (status = await updateApi.status()));
    } catch (e) {
      message = String(e);
    }
  });
  onDestroy(() => unlisten?.());

  const lastCheck = $derived(status?.lastCheck ? new Date(status.lastCheck * 1000).toLocaleString() : "never");

  async function checkNow() {
    busy = "checking";
    message = null;
    try {
      const found = await updateApi.check();
      message = found ? null : "You are up to date.";
    } catch (e) {
      message = String(e);
    } finally {
      status = await updateApi.status();
      busy = null;
    }
  }

  async function install() {
    busy = "installing";
    message = null;
    try {
      await updateApi.install();
    } catch (e) {
      message = String(e);
      busy = null;
    }
  }

  async function setUpdates(patch: Partial<ShellSettings["updates"]>) {
    if (!settings) return;
    try {
      settings = await shellApi.setShellSettings({ ...settings, updates: { ...settings.updates, ...patch } });
    } catch (e) {
      message = String(e);
    }
  }
</script>

<section class="card">
  <h3>Updates</h3>
  {#if info}
    <p class="dim line">
      {info.name} {info.version}{info.portable ? " (portable)" : ""} · last checked {lastCheck}
    </p>
  {/if}

  {#if status && !status.enabled}
    <p class="dim">Updates are not configured for this build.</p>
  {:else}
    {#if status?.available}
      <div class="available">
        <div>
          <strong>Version {status.available.version} is available.</strong>
          {#if status.available.notes}<p class="dim notes">{status.available.notes}</p>{/if}
        </div>
        {#if info?.portable}
          <button type="button" onclick={() => updateApi.openReleasesPage()}>Download</button>
        {:else}
          <button type="button" disabled={busy !== null} onclick={install}>
            {busy === "installing" ? "Installing…" : "Install and restart"}
          </button>
        {/if}
      </div>
    {/if}

    <div class="actions">
      <button type="button" disabled={busy !== null} onclick={checkNow}>
        {busy === "checking" ? "Checking…" : "Check now"}
      </button>
      {#if message}<span class="dim">{message}</span>{/if}
    </div>

    {#if settings}
      <div class="toggles">
        <SettingsRow label="Check automatically" description="Look for updates shortly after start and then once a day.">
          <Switch
            checked={settings.updates.checkAutomatically}
            label="Check automatically"
            onchange={(next) => setUpdates({ checkAutomatically: next })}
          />
        </SettingsRow>
        <SettingsRow
          label="Install automatically"
          description="Install found updates without asking. Not available for portable builds."
        >
          <Switch
            checked={settings.updates.installAutomatically}
            label="Install automatically"
            disabled={info?.portable ?? false}
            onchange={(next) => setUpdates({ installAutomatically: next })}
          />
        </SettingsRow>
      </div>
    {/if}
  {/if}
</section>

<style>
  .line {
    margin: 0 0 10px;
  }
  .available {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: 16px;
    padding: 8px 10px;
    margin-bottom: 10px;
    border: 1px solid var(--accent);
    border-radius: 6px;
    background: var(--accent-soft);
  }
  .notes {
    margin: 4px 0 0;
    white-space: pre-wrap;
  }
  .actions {
    display: flex;
    align-items: center;
    gap: 10px;
    margin-bottom: 12px;
  }
  .toggles {
    display: flex;
    flex-direction: column;
    gap: 12px;
  }
</style>
