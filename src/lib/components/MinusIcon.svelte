<script lang="ts">
  import { MinusIcon } from "phosphor-svelte";
  import type { ComponentProps } from "svelte";

  // Always the single-tone glyph: the app's duotone default turns the minus into a shaded box that
  // is hard to read at small sizes. Import this, not phosphor-svelte's `MinusIcon` (`no-duotone-plus-minus`).
  let props: ComponentProps<typeof MinusIcon> = $props();
</script>

<MinusIcon {...props} weight="regular" />
