<script lang="ts">
  import { DotsThreeIcon } from "phosphor-svelte";

  let { class: className = "" }: { class?: string } = $props();
</script>

<!-- The app's "more" (three dots) glyph. Always the single-tone bold weight, not the duotone
     the app-wide IconContext applies, so every "more" button looks the same. -->
<DotsThreeIcon weight="bold" class={className} />
