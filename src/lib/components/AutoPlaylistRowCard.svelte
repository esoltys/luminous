<script lang="ts">
  import PlaylistKindIcon from "./PlaylistKindIcon.svelte";
  import { getPlaylistCardTheme } from "../utils/playlistCardTheme";
  import { i18n } from "../stores/i18n.svelte";
  import { formatRelativeDate } from "../utils/date";
  import { playlistsStore } from "../stores/playlists.svelte";
  import { getPlaylistDisplayName } from "../utils/playlist";
  import { toTitleCase } from "../utils/formatters";

  interface Props {
    label: string;
    kind: "favourites" | "recently_added" | "most_played" | "history" | "genre" | "decade" | "bpm" | "artist_tag" | "missing_metadata" | "missing_musicbrainz" | "daypart";
    genre?: string;
    artistTag?: string;
    decade?: string;
    bpm?: string;
    playlistId?: number;
    updated?: number;
    trackCount: number;
    onClick: () => void;
    oncontextmenu?: (e: MouseEvent) => void;
  }

  let { label, kind, genre, artistTag, decade, bpm, playlistId, updated, trackCount, onClick, oncontextmenu }: Props = $props();

  let displayLabel = $derived.by(() => {
    if (
      (kind === "genre" || kind === "decade" || kind === "bpm" || kind === "artist_tag" || kind === "missing_metadata" || kind === "missing_musicbrainz" || kind === "daypart") &&
      playlistId !== undefined
    ) {
      const pl = playlistsStore.playlists.find((p) => p.id === playlistId);
      if (pl) return getPlaylistDisplayName(pl);
    }
    return kind === "artist_tag" ? toTitleCase(label) : label;
  });

  let subtitleLabel = $derived.by(() => {
    if (kind === "missing_metadata") return i18n.t("playlists.missingMetadataAutoPlaylist");
    if (kind === "missing_musicbrainz") return i18n.t("playlists.missingMusicBrainzAutoPlaylist");
    if (kind === "daypart") return i18n.t("playlists.daypartAutoPlaylist");
    if (kind === "decade" || decade) return i18n.t("playlists.decadeAutoPlaylist");
    if (kind === "bpm") return i18n.t("playlists.bpmAutoPlaylist");
    if (kind === "artist_tag" || artistTag) return i18n.t("playlists.artistTagAutoPlaylist");
    if (kind === "genre" || genre) return i18n.t("playlists.genreAutoPlaylist");
    if (kind === "favourites") return i18n.t("playlists.favouritesAutoPlaylist");
    if (kind === "recently_added") return i18n.t("playlists.recentlyAddedAutoPlaylist");
    if (kind === "most_played") return i18n.t("playlists.mostPlayedAutoPlaylist");
    if (kind === "history") return i18n.t("playlists.historyAutoPlaylist");
    return i18n.t("playlists.genreAutoPlaylist");
  });

  let updatedLabel = $derived.by(() => {
    if (
      (kind !== "genre" && kind !== "decade" && kind !== "bpm" && kind !== "artist_tag" && kind !== "missing_metadata" && kind !== "missing_musicbrainz" && kind !== "daypart") ||
      updated === undefined
    )
      return null;
    return formatRelativeDate(updated);
  });
</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<!-- svelte-ignore a11y_click_events_have_key_events -->
<div
  onclick={onClick}
  {oncontextmenu}
  class="group flex items-center gap-3 px-3 py-2.5 rounded-lg bg-brand-sidebar border border-brand-border/60 outline-2 -outline-offset-2 outline-transparent hover:outline-brand-accent transition-[outline-color,border-color] duration-200 select-none"
>
  <div
    class="relative shrink-0 w-11 h-11 flex items-center justify-center overflow-hidden border {getPlaylistCardTheme(kind).tintClass}"
  >
    <PlaylistKindIcon {kind} sizeClass="w-5 h-5" />
  </div>

  <div class="min-w-0 flex-1">
    <p class="truncate text-sm font-semibold text-brand-text-primary" title={displayLabel}>{displayLabel}</p>
    <p class="truncate text-xs text-brand-text-secondary font-medium">{subtitleLabel}</p>
  </div>

  <div class="shrink-0 max-w-40 text-right">
    <p class="text-xs text-brand-text-secondary font-medium tabular-nums truncate">
      {i18n.plural("playlists.songsCount", trackCount)}
    </p>
    {#if updatedLabel}
      <p class="text-xs text-brand-text-secondary truncate">{updatedLabel}</p>
    {/if}
  </div>
</div>
