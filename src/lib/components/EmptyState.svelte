<script lang="ts">
  import type { Component } from "svelte";

  interface Props {
    icon: Component<{ class?: string }>;
    title: string;
    subtitle?: string;
    /** Whether the icon plays its bounce/pulse animation. */
    pulse?: boolean;
    /** Wraps the content in the dashed-border "card" treatment used in grid empty states. */
    card?: boolean;
    subtitleClass?: string;
  }

  let {
    icon: Icon,
    title,
    subtitle,
    pulse = true,
    card = false,
    subtitleClass = "text-xs text-brand-text-secondary/60 font-medium",
  }: Props = $props();
</script>

{#if card}
  <div class="flex flex-col items-center justify-center max-w-sm mx-auto p-6 bg-brand-sidebar/20 rounded-xl border border-dashed border-brand-border/60 select-none">
    <Icon class="w-12 h-12 text-brand-accent-text/40 mb-3 {pulse ? 'animate-pulse' : ''}" />
    <h3 class="text-base font-semibold text-brand-text-primary mb-1">{title}</h3>
    {#if subtitle}
      <p class={subtitleClass}>{subtitle}</p>
    {/if}
  </div>
{:else}
  <Icon class="w-12 h-12 text-brand-accent-text/40 mb-3 mx-auto {pulse ? 'animate-pulse' : ''}" />
  <h3 class="text-sm font-semibold text-brand-text-primary mb-1">{title}</h3>
{/if}
