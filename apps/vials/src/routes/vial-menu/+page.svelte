<script lang="ts">
  // A vial's own menu, opened by a right click on it.
  import { page } from "$app/state";
  import { VialMenuWindow } from "@desqu/vials-ui";

  const vialId = $derived(page.url.searchParams.get("vial") ?? "");
</script>

<VialMenuWindow {vialId} />
