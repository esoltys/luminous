<script lang="ts">
  import type { Snippet } from "svelte";
  import { i18n } from "../stores/i18n.svelte";

  let {
    widthClass = "w-full",
    onClick,
    oncontextmenu,
    onContextMenu,
    title,
    subtitleLabel = null,
    updatedLabel,
    trackCount,
    cover,
    footer,
  }: {
    widthClass?: string;
    onClick: () => void;
    oncontextmenu?: (e: MouseEvent) => void;
    onContextMenu?: (e: MouseEvent) => void;
    title: string;
    subtitleLabel?: string | null;
    updatedLabel: string;
    trackCount: number;
    cover: Snippet;
    footer?: Snippet;
  } = $props();
</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<!-- svelte-ignore a11y_click_events_have_key_events -->
<div
  onclick={onClick}
  oncontextmenu={(e) => { (oncontextmenu || onContextMenu)?.(e); }}
  class="{widthClass} h-full bg-brand-sidebar border border-brand-border/60 rounded-xl p-4 flex flex-col text-left outline-2 -outline-offset-2 outline-transparent hover:outline-brand-accent transition-[outline-color,border-color] duration-200 group relative"
>
  <div class="aspect-square w-full mb-3 bg-brand-main relative flex items-center justify-center overflow-hidden">
    {@render cover()}
  </div>

  <button
    onclick={(e) => { e.stopPropagation(); onClick(); }}
    class="font-semibold text-sm text-brand-text-primary group-hover:text-brand-accent-text group-hover:underline transition-all duration-150 text-left truncate w-full"
    {title}
  >
    {title}
  </button>
  {#if subtitleLabel}
    <div class="text-xs text-brand-text-secondary truncate w-full mt-0.5 font-medium">
      {subtitleLabel}
    </div>
  {/if}
  <div class="flex items-center justify-between mt-1.5 text-xs leading-[22px] text-brand-text-secondary">
    <span class="truncate">{updatedLabel}</span>
    <span class="shrink-0">{trackCount === 1 ? i18n.t('playlists.oneSong') : i18n.t("playlists.songsCount", { count: trackCount })}</span>
  </div>

  {#if footer}
    {@render footer()}
  {/if}
</div>
