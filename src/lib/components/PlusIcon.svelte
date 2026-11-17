<script lang="ts">
  import { PlusIcon } from "phosphor-svelte";
  import type { ComponentProps } from "svelte";

  // Always the single-tone glyph: the app's duotone default turns the plus into a shaded box that
  // is hard to read at small sizes. Import this, not phosphor-svelte's `PlusIcon` (`no-duotone-plus-minus`).
  let props: ComponentProps<typeof PlusIcon> = $props();
</script>

<PlusIcon {...props} weight="regular" />
