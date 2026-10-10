<script lang="ts">
  import {
    PlaylistIcon as ListMusic
  } from "phosphor-svelte";
  import type { Playlist } from "../types";
  import { i18n } from "../stores/i18n.svelte";
  import { formatRelativeDate } from "../utils/date";
  import { isSmartPlaylistSpec } from "../utils/filterParser";
  import PlaylistKindIcon from "./PlaylistKindIcon.svelte";
  import { getPlaylistCardTheme } from "../utils/playlistCardTheme";
  import { getPlaylistDisplayName } from "../utils/playlist";

  let {
    playlist,
    onClick,
    oncontextmenu,
  }: { playlist: Playlist; onClick: () => void; oncontextmenu?: (e: MouseEvent) => void } = $props();

  let cardTitle = $derived(getPlaylistDisplayName(playlist));

  // System genre auto-playlists never reach this component (they render via
  // AutoPlaylistRowCard instead) — mirrors PlaylistCard's autoKind derivation.
  let autoKind = $derived<"genre" | "decade" | "smart" | null>(
    !playlist.dynamic_enabled
      ? null
      : playlist.dynamic_spec?.startsWith("decade:")
        ? "decade"
        : isSmartPlaylistSpec(playlist.dynamic_spec)
          ? "smart"
          : "genre"
  );

  let isQueue = $derived(playlist.is_queue);

  let subtitleLabel = $derived.by(() => {
    if (!playlist.dynamic_enabled) return null;
    if (autoKind === "decade") return i18n.t("playlists.decadeAutoPlaylist");
    if (autoKind === "genre") return i18n.t("playlists.genreAutoPlaylist");
    return i18n.t("playlists.smartRulePlaylistLabel");
  });

  let updatedLabel = $derived(formatRelativeDate(playlist.updated));
</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<!-- svelte-ignore a11y_click_events_have_key_events -->
<div
  onclick={onClick}
  {oncontextmenu}
  class="group flex items-center gap-3 px-3 py-2.5 rounded-lg bg-brand-sidebar border border-brand-border/60 outline-2 -outline-offset-2 outline-transparent hover:outline-brand-accent transition-[outline-color,border-color] duration-200 select-none"
>
  <div
    class="relative shrink-0 w-11 h-11 flex items-center justify-center overflow-hidden border {isQueue
      ? getPlaylistCardTheme("queue").tintClass
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
    <p class="truncate text-sm font-semibold text-brand-text-primary" title={cardTitle}>{cardTitle}</p>
    {#if subtitleLabel}
      <p class="truncate text-xs text-brand-text-secondary font-medium">{subtitleLabel}</p>
    {/if}
  </div>

  <div class="shrink-0 max-w-40 text-right">
    <p class="text-xs text-brand-text-secondary font-medium tabular-nums truncate">
      {i18n.plural("playlists.songsCount", playlist.track_count)}
    </p>
    <p class="text-xs text-brand-text-secondary truncate">{updatedLabel}</p>
  </div>
</div>
