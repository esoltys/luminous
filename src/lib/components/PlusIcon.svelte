<script lang="ts">
  import { PlusIcon } from "phosphor-svelte";
  import type { ComponentProps } from "svelte";

  // Always the single-tone glyph; the `single-tone-glyphs` ast-grep rule says why. Import this,
  // not phosphor-svelte's `PlusIcon`.
  let props: ComponentProps<typeof PlusIcon> = $props();
</script>

<PlusIcon {...props} weight="regular" />
