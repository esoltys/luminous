<script lang="ts">
  import type { Snippet } from "svelte";

  interface Props {
    onclick: () => void;
    disabled?: boolean;
    title: string;
    icon: Snippet;
    class?: string;
  }

  let { onclick, disabled = false, title, icon, class: extraClass = "" }: Props = $props();
</script>

<button
  {onclick}
  {disabled}
  {title}
  class="flex items-center justify-center w-10 h-10 rounded-full border border-brand-border text-brand-text-secondary hover:text-brand-accent-text hover:bg-brand-sidebar transition-colors cursor-pointer disabled:opacity-50 disabled:cursor-not-allowed shadow-xs {extraClass}"
>
  {@render icon()}
</button>
