<script lang="ts">
  import type { Snippet } from "svelte";
  import { i18n } from "../stores/i18n.svelte";
  import { PLAIN_TILE_COVER, useCardAppearance } from "../utils/cardAppearance";

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

  const plain = useCardAppearance() === "plain";

  // Plain tiles fold the kind and the song count onto one line ("Moment mix, 299 songs").
  const plainDetail = $derived(
    [subtitleLabel, i18n.plural("playlists.songsCount", trackCount)].filter(Boolean).join(", "),
  );
</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<!-- svelte-ignore a11y_click_events_have_key_events -->
<div
  onclick={onClick}
  oncontextmenu={(e) => {
    (oncontextmenu || onContextMenu)?.(e);
  }}
  class="{widthClass} h-full flex flex-col text-left group relative {plain
    ? ''
    : 'bg-brand-sidebar border border-brand-border/60 rounded-xl p-4 outline-2 -outline-offset-2 outline-transparent hover:outline-brand-accent transition-[outline-color,border-color] duration-200'}"
>
  <div
    class="aspect-square w-full mb-3 bg-brand-main relative flex items-center justify-center overflow-hidden {plain
      ? PLAIN_TILE_COVER
      : ''}"
  >
    {@render cover()}
  </div>

  <button
    onclick={(e) => {
      e.stopPropagation();
      onClick();
    }}
    class="font-semibold text-sm text-brand-text-primary group-hover:text-brand-accent-text group-hover:underline transition-colors duration-150 text-left truncate w-full"
    {title}
  >
    {title}
  </button>
  {#if plain}
    <div class="text-xs text-brand-text-secondary truncate w-full mt-0.5 font-medium">
      {plainDetail}
    </div>
  {:else}
    {#if subtitleLabel}
      <div class="text-xs text-brand-text-secondary truncate w-full mt-0.5 font-medium">
        {subtitleLabel}
      </div>
    {/if}
    <div
      class="flex items-center justify-between mt-1.5 text-xs leading-[22px] text-brand-text-secondary"
    >
      <span class="truncate">{updatedLabel}</span>
      <span class="shrink-0">{i18n.plural("playlists.songsCount", trackCount)}</span>
    </div>
  {/if}

  {#if footer}
    {@render footer()}
  {/if}
</div>
