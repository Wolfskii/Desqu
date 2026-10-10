<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { ScreenboundView } from "@desqu/screenbound-ui";
</script>

<ScreenboundView onOpenDesqu={() => invoke("open_desqu")} onExit={() => invoke("exit_app")} />
