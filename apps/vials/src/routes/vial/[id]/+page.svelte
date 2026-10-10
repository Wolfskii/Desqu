<script lang="ts">
  // A vial's own window on the desktop; the module opens it at `vial/<id>`.
  import { page } from "$app/state";
  import { VialView } from "@desqu/vials-ui";

  const id = $derived(page.params.id ?? "");
</script>

<VialView vialId={id} />
