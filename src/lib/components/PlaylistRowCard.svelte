<script lang="ts">
  import { PlaylistIcon as ListMusic } from "phosphor-svelte";
  import type { Playlist } from "../types";
  import { i18n } from "../stores/i18n.svelte";
  import { formatRelativeDate } from "../utils/date";
  import PlaylistKindIcon from "./PlaylistKindIcon.svelte";
  import { getPlaylistCardTheme } from "../utils/playlistCardTheme";
  import {
    formatPlaylistItemCount,
    getPlaylistAutoKindAndSubtitle,
    getPlaylistDisplayName,
  } from "../utils/playlist";
  import { rowClasses, useCardAppearance } from "../utils/cardAppearance";

  const appearance = useCardAppearance();

  let {
    playlist,
    onClick,
    oncontextmenu,
  }: { playlist: Playlist; onClick: () => void; oncontextmenu?: (e: MouseEvent) => void } =
    $props();

  let cardTitle = $derived(getPlaylistDisplayName(playlist));

  let dynamicMeta = $derived(getPlaylistAutoKindAndSubtitle(playlist));
  let autoKind = $derived(dynamicMeta?.kind ?? null);

  let isQueue = $derived(playlist.is_queue);

  let subtitleLabel = $derived(dynamicMeta?.subtitle ?? null);

  let updatedLabel = $derived(formatRelativeDate(playlist.updated));
</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<!-- svelte-ignore a11y_click_events_have_key_events -->
<div onclick={onClick} {oncontextmenu} class={rowClasses(appearance)}>
  <div
    class="relative shrink-0 w-11 h-11 flex items-center justify-center overflow-hidden border {isQueue
      ? getPlaylistCardTheme('queue').tintClass
      : autoKind
        ? getPlaylistCardTheme(autoKind).tintClass
        : 'bg-brand-main border-brand-border/60'}"
  >
    {#if isQueue}
      <PlaylistKindIcon kind="queue" sizeClass="w-5 h-5" />
    {:else if autoKind}
      <PlaylistKindIcon kind={autoKind} sizeClass="w-5 h-5" />
    {:else}
      <ListMusic class="w-5 h-5 text-brand-text-secondary" />
    {/if}
  </div>

  <div class="min-w-0 flex-1">
    <p class="truncate text-sm font-semibold text-brand-text-primary" title={cardTitle}>
      {cardTitle}
    </p>
    {#if subtitleLabel}
      <p class="truncate text-xs text-brand-text-secondary font-medium">{subtitleLabel}</p>
    {/if}
  </div>

  <div class="shrink-0 max-w-40 text-right">
    <p class="text-xs text-brand-text-secondary font-medium tabular-nums truncate">
      {formatPlaylistItemCount(playlist, playlist.track_count)}
    </p>
    <p class="text-xs text-brand-text-secondary truncate">{updatedLabel}</p>
  </div>
</div>
