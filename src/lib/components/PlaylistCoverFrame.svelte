<script lang="ts">
  import type { Snippet } from "svelte";

  let {
    gradientClass,
    children,
  }: {
    gradientClass?: string | null;
    children: Snippet;
  } = $props();
</script>

{#if gradientClass}
  <div class="w-full h-full bg-brand-main bg-gradient-to-br {gradientClass} flex items-center justify-center overflow-hidden border">
    {@render children()}
  </div>
{:else}
  {@render children()}
{/if}
