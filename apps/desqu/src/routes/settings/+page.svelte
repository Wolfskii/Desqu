<script lang="ts">
  import { onMount } from "svelte";
  import { shellApi, ShellSettingsSection, UpdateCard, type AppInfo } from "@desqu/ui";
  import { desqu } from "$lib/state.svelte";

  let info = $state<AppInfo | null>(null);

  onMount(async () => {
    try {
      info = await shellApi.appInfo();
    } catch (e) {
      desqu.error = String(e);
    }
  });

  const startsWithSystem = $derived(desqu.modules.some((m) => m.enabled && m.startWithSystem));
</script>

<section class="page">
  <h1>Settings</h1>

  <ShellSettingsSection appName="Desqu" showStartWithSystem={false} />

  <div class="card">
    <h3>Start with system</h3>
    <p class="dim">
      Desqu {startsWithSystem ? "starts" : "does not start"} with Windows. This follows your apps: it starts when at
      least one app that is turned on has <em>Start with system</em> enabled, and then it only starts those apps.
      Change it on the Home page.
    </p>
  </div>

  <UpdateCard />

  <div class="card">
    <h3>About</h3>
    <table>
      <tbody>
        <tr><td>Desqu</td><td class="mono">{info?.version ?? "…"}{info?.portable ? " (portable)" : ""}</td></tr>
        {#each desqu.modules as m (m.id)}
          <tr><td>{m.name}</td><td class="mono">{m.version}</td></tr>
        {/each}
      </tbody>
    </table>
    <p class="dim">
      Settings of each app are shared with its standalone version, if you have one installed. Uninstalling a
      standalone app with <em>Delete app data</em> checked removes those shared settings.
    </p>
  </div>
</section>

<style>
  .page {
    display: flex;
    flex-direction: column;
    gap: 12px;
    max-width: 760px;
    padding: 18px 20px;
  }
  h1 {
    margin: 0;
    font-size: 20px;
  }
  .card p {
    margin: 0 0 8px;
  }
  table {
    margin-bottom: 10px;
    border-collapse: collapse;
  }
  td {
    padding: 2px 24px 2px 0;
  }
</style>
