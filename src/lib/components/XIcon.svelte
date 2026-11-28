<script lang="ts">
  import { XIcon } from "phosphor-svelte";
  import type { ComponentProps } from "svelte";

  // Always the single-tone glyph; the `single-tone-glyphs` ast-grep rule says why. Import this,
  // not phosphor-svelte's `XIcon`.
  let props: ComponentProps<typeof XIcon> = $props();
</script>

<XIcon {...props} weight="regular" />
