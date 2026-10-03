<script lang="ts">
  import type { AddonTheme } from "../stores/addons.svelte";

  // Generic mount point for an add-on's overlay above the player bar (#1036).
  // The sandboxed iframe runtime and its event bridge arrive in #1413; until
  // then this is only the positioned, inert container.
  let { addon }: { addon: AddonTheme } = $props();
</script>

<div
  class="absolute bottom-full inset-x-0 h-0 pointer-events-none"
  aria-hidden="true"
  data-addon-id={addon.id}
></div>
