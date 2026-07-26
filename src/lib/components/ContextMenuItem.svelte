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
  }

  let { icon: Icon, label, onclick, accent = false, destructive = false }: Props = $props();
</script>

<button
  {onclick}
  class="w-full text-left px-3 py-1.5 flex items-center gap-2.5 transition-colors cursor-pointer {destructive
    ? 'hover:bg-red-500/10 hover:text-red-400 text-red-400'
    : accent
      ? 'hover:bg-brand-accent/15 hover:text-brand-accent-text'
      : 'hover:bg-brand-sidebar/80 hover:text-brand-text-primary'}"
  role="menuitem"
>
  <Icon
    class="w-3.5 h-3.5 shrink-0 {destructive ? 'text-red-400' : accent ? 'text-brand-accent-text' : 'text-brand-text-secondary'}"
  />
  <span>{label}</span>
</button>
