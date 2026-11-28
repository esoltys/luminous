<script lang="ts">
  import { MinusIcon } from "phosphor-svelte";
  import type { ComponentProps } from "svelte";

  // Always the single-tone glyph; the `single-tone-glyphs` ast-grep rule says why. Import this,
  // not phosphor-svelte's `MinusIcon`.
  let props: ComponentProps<typeof MinusIcon> = $props();
</script>

<MinusIcon {...props} weight="regular" />
