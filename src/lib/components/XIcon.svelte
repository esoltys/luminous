<script lang="ts">
  import { XIcon } from "phosphor-svelte";
  import type { ComponentProps } from "svelte";

  // Always the single-tone glyph: the app's duotone default draws a shaded box behind the close X
  // that is hard to read at small sizes. Import this, not phosphor-svelte's `XIcon` (`single-tone-glyphs`).
  let props: ComponentProps<typeof XIcon> = $props();
</script>

<XIcon {...props} weight="regular" />
