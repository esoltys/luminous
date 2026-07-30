<script lang="ts">
  import type { Snippet } from "svelte";

  interface Props {
    onclick: (e: MouseEvent) => void;
    title?: string;
    class?: string;
    children: Snippet;
  }

  let { onclick, title, class: className = "", children }: Props = $props();
</script>

<button
  type="button"
  {onclick}
  {title}
  class="text-left cursor-pointer hover:underline hover:text-brand-accent-text transition-colors max-w-full {className}"
>
  {@render children()}
</button>
