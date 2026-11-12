<script lang="ts">
  import type { Snippet } from "svelte";
  import { windowLayoutStore } from "../stores/windowLayout.svelte";

  interface Props {
    /** Localized collection title displayed in the Expose heading face. */
    title: string;
    /** Pre-formatted count or filtered-count subtitle displayed beneath the title. */
    subtitle: string;
    /** Optional right-aligned view controls (sort select, view-mode toggle, etc.). */
    actions?: Snippet;
    /** Optional secondary action row rendered beneath the title/controls row. */
    toolbar?: Snippet;
  }

  let { title, subtitle, actions, toolbar }: Props = $props();
</script>

<header class="px-6 {windowLayoutStore.isDetailHeaderCollapsed ? 'py-3' : 'pt-6 pb-4'}">
  <div class="flex flex-wrap items-center justify-between gap-x-4 gap-y-2">
    <div class="min-w-0">
      <h1
        class="text-3xl @xl:text-4xl font-heading font-bold text-brand-text-primary leading-snug truncate py-0.5"
      >
        {title}
      </h1>
      <p class="text-xs text-brand-text-secondary font-medium mt-0.5">
        {subtitle}
      </p>
    </div>

    {#if actions}
      <div class="flex flex-wrap items-center gap-2 shrink-0">
        {@render actions()}
      </div>
    {/if}
  </div>

  {#if toolbar}
    <div
      class="flex flex-wrap items-center gap-2 {windowLayoutStore.isDetailHeaderCollapsed
        ? 'mt-2'
        : 'mt-3'}"
    >
      {@render toolbar()}
    </div>
  {/if}
</header>
