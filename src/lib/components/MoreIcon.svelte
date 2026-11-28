<script lang="ts">
  import { DotsThreeIcon } from "phosphor-svelte";

  let { class: className = "" }: { class?: string } = $props();
</script>

<!-- The app's "more" (three dots) glyph, always at the bold weight so every "more" button looks the
     same; the `single-tone-glyphs` ast-grep rule says why it isn't duotone. Import this, not
     phosphor-svelte's `DotsThreeIcon`. -->
<DotsThreeIcon weight="bold" class={className} />
