<script lang="ts">
  import { onMount } from "svelte";
  import SettingsRow from "./SettingsRow.svelte";
  import { shellApi, type ShellSettings } from "./shell-api";
  import Switch from "./Switch.svelte";

  let { appName = "this app" }: { appName?: string } = $props();

  let settings = $state<ShellSettings | null>(null);
  let error = $state<string | null>(null);

  onMount(async () => {
    try {
      settings = await shellApi.getShellSettings();
    } catch (e) {
      error = String(e);
    }
  });

  async function update(patch: Partial<ShellSettings>) {
    if (!settings) return;
    try {
      settings = await shellApi.setShellSettings({ ...settings, ...patch });
      error = null;
    } catch (e) {
      error = String(e);
    }
  }
</script>

<section class="card">
  <SettingsRow
    label="Start with system"
    description="Launch {appName} when you sign in to Windows. It stays in the tray so it works without opening the window first."
  >
    <Switch
      checked={settings?.startWithSystem ?? false}
      label="Start with system"
      disabled={!settings}
      onchange={(next) => update({ startWithSystem: next })}
    />
  </SettingsRow>
  <SettingsRow label="Start minimized to tray" description="Keep the window hidden when {appName} starts.">
    <Switch
      checked={settings?.startMinimized ?? false}
      label="Start minimized to tray"
      disabled={!settings}
      onchange={(next) => update({ startMinimized: next })}
    />
  </SettingsRow>
  <SettingsRow
    label="Close to tray"
    description="The close button hides the window and keeps {appName} running. Turn off to quit when the window closes."
  >
    <Switch
      checked={settings?.closeToTray ?? true}
      label="Close to tray"
      disabled={!settings}
      onchange={(next) => update({ closeToTray: next })}
    />
  </SettingsRow>
  {#if error}<p class="error">{error}</p>{/if}
</section>

<style>
  section {
    display: flex;
    flex-direction: column;
    gap: 14px;
  }
  .error {
    margin: 0;
    color: var(--error);
  }
</style>
