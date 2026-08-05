<script lang="ts">
  import type { ComponentType, SvelteComponent } from "svelte";

  interface Props {
    icon: ComponentType<SvelteComponent<{ class?: string }>>;
    label: string;
    onclick: () => void;
    /** Highlights the item as the menu's primary action (e.g. Play). */
    accent?: boolean;
    /** Highlights the item as a destructive action (e.g. Remove/Delete). */
    destructive?: boolean;
    disabled?: boolean;
  }

  let { icon: Icon, label, onclick, accent = false, destructive = false, disabled = false }: Props = $props();
</script>

<button
  onclick={disabled ? undefined : onclick}
  {disabled}
  class="w-full text-left px-3 py-1.5 flex items-center gap-2.5 transition-colors {disabled
    ? 'opacity-40 cursor-not-allowed text-brand-text-secondary'
    : destructive
      ? 'hover:bg-red-500/10 hover:text-red-400 text-red-400 cursor-pointer'
      : accent
        ? 'hover:bg-brand-accent/15 hover:text-brand-accent-text cursor-pointer'
        : 'hover:bg-brand-main hover:text-brand-text-primary cursor-pointer'}"
  role="menuitem"
>
  <Icon
    class="w-3.5 h-3.5 shrink-0 {disabled ? 'text-brand-text-secondary/50' : destructive ? 'text-red-400' : accent ? 'text-brand-accent-text' : 'text-brand-text-secondary'}"
  />
  <span>{label}</span>
</button>
