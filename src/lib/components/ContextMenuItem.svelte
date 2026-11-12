<script lang="ts">
  import type { Component } from "svelte";

  interface Props {
    icon: Component<{ class?: string }>;
    label: string;
    onclick: () => void;
    onmouseenter?: () => void;
    /** Highlights the item as the menu's primary action (e.g. Play). */
    accent?: boolean;
    /** Highlights the item as a destructive action (e.g. Remove/Delete). */
    destructive?: boolean;
    disabled?: boolean;
    title?: string;
    /** The item is what Enter or double-click on the row does: bold, with an Enter hint. */
    isDefault?: boolean;
  }

  let {
    icon: Icon,
    label,
    onclick,
    onmouseenter,
    accent = false,
    destructive = false,
    disabled = false,
    title,
    isDefault = false,
  }: Props = $props();
</script>

<button
  onclick={disabled ? undefined : onclick}
  {onmouseenter}
  {disabled}
  {title}
  class="w-full text-left px-3 py-1.5 flex items-center gap-2.5 transition-colors {disabled
    ? 'opacity-40 cursor-not-allowed text-brand-text-secondary'
    : destructive
      ? 'hover:bg-brand-text-primary/15 hover:text-brand-text-primary text-brand-text-primary'
      : accent
        ? 'hover:bg-brand-accent/15 hover:text-brand-accent-text'
        : 'hover:bg-brand-main hover:text-brand-text-primary'}"
  role="menuitem"
>
  <Icon
    class="w-3.5 h-3.5 shrink-0 {disabled
      ? 'text-brand-text-secondary/50'
      : destructive
        ? 'text-brand-text-primary'
        : accent
          ? 'text-brand-accent-text'
          : 'text-brand-text-secondary'}"
  />
  <span class:font-semibold={isDefault}>{label}</span>
  {#if isDefault}
    <kbd class="ml-auto pl-4 font-sans text-[11px] text-brand-text-secondary" aria-hidden="true"
      >↵</kbd
    >
  {/if}
</button>
