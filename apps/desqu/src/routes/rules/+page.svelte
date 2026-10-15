<script lang="ts">
  // The window for automatic rules, opened from a vial's menu or the Rules tab.
  import { page } from "$app/state";
  import { VialRules } from "@desqu/vials-ui";

  const vial = $derived(page.url.searchParams.get("vial"));
</script>

<VialRules initialVial={vial} />
