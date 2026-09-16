<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import {
    HeartIcon as Heart,
    ClockIcon as Clock,
    HourglassIcon as Hourglass,
    CalendarIcon as Calendar,
    MusicNotesIcon as Music,
    GaugeIcon as Gauge,
    TagIcon as Tag,
    TrendUpIcon as TrendingUp,
    WarningIcon as AlertTriangle,
    SunHorizonIcon as SunHorizon
  } from "phosphor-svelte";
  import type { PlaylistItem, Song } from "../types";
  import { songsToCoverStack } from "../utils/covers";
  import { i18n } from "../stores/i18n.svelte";
  import { formatRelativeDate } from "../utils/date";
  import CoverStack from "./CoverStack.svelte";
  import PlaylistCardShell from "./PlaylistCardShell.svelte";
  import PlaylistCoverFrame from "./PlaylistCoverFrame.svelte";
  import { getPlaylistCardTheme } from "../utils/playlistCardTheme";
  import { playlistsStore } from "../stores/playlists.svelte";
  import { getPlaylistDisplayName } from "../utils/playlist";
  import { toTitleCase } from "../utils/formatters";

  interface Props {
    label: string;
    kind: "favourites" | "recently_added" | "most_played" | "history" | "genre" | "decade" | "bpm" | "artist_tag" | "missing_metadata" | "missing_musicbrainz" | "daypart";
    genre?: string;
    artistTag?: string;
    decade?: string;
    /** For kind "bpm": the bucket's dynamic_spec suffix, e.g. "60-90" or the open-ended "150-". */
    bpm?: string;
    /** For kind "genre", "decade", "bpm" or "artist_tag": the materialized playlist row backing it (refreshed at most every 24h). */
    playlistId?: number;
    /** For kind "genre", "decade", "bpm" or "artist_tag": when this playlist's songs were last (re)generated. */
    updated?: number;
    trackCount: number;
    onClick: () => void;
    oncontextmenu?: (e: MouseEvent) => void;
    onContextMenu?: (e: MouseEvent) => void;
    widthClass?: string;
  }

  let {
    label,
    kind,
    genre,
    artistTag,
    decade,
    bpm,
    playlistId,
    updated,
    trackCount,
    onClick,
    oncontextmenu,
    onContextMenu,
    widthClass = "w-full",
  }: Props = $props();

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

  let songs = $state<Song[]>([]);

  $effect(() => {
    const k = kind;
    const g = genre;
    const at = artistTag;
    const d = decade;
    const b = bpm;
    const pid = playlistId;

    const request =
      (k === "genre" || k === "decade" || k === "bpm" || k === "artist_tag" || k === "daypart") && pid !== undefined
        ? invoke<PlaylistItem[]>("get_playlist_tracks", { playlistId: pid }).then((items) =>
            items.filter((item) => !!item.song).map((item) => item.song as Song)
          )
        : k === "missing_musicbrainz"
          ? (pid !== undefined
              ? invoke<PlaylistItem[]>("get_playlist_tracks", { playlistId: pid }).then((items) =>
                  items.filter((item) => !!item.song).map((item) => item.song as Song)
                )
              : invoke<Song[]>("get_songs_missing_musicbrainz_id", { limit: 50 }))
          : k === "missing_metadata"
            ? (pid !== undefined
                ? invoke<PlaylistItem[]>("get_playlist_tracks", { playlistId: pid }).then((items) =>
                    items.filter((item) => !!item.song).map((item) => item.song as Song)
                  )
                : invoke<Song[]>("get_songs_missing_metadata", { limit: 50 }))
        : k === "favourites"
          ? invoke<Song[]>("get_favourite_songs")
          : k === "recently_added"
            ? invoke<Song[]>("get_recently_added_songs", { limit: 50 })
            : k === "most_played"
              ? invoke<Song[]>("get_most_played_songs", { limit: 50 })
              : k === "history"
                ? invoke<Song[]>("get_recently_played_songs", { limit: 50 })
                : k === "decade"
                ? invoke<Song[]>("get_songs_by_decade", { decade: d ?? "", limit: 50 })
                : k === "bpm"
                  ? invoke<Song[]>("get_songs_by_bpm", { spec: b ?? "", limit: 50 })
                  : k === "artist_tag"
                    ? invoke<Song[]>("get_songs_by_artist_tag", { tag: at ?? "", limit: 50 })
                    : invoke<Song[]>("get_songs_by_curated_tag", { tagName: g ?? "", limit: 50 });

    request
      .then((res) => {
        if (kind === k && genre === g && artistTag === at && decade === d && bpm === b && playlistId === pid) {
          songs = res;
        }
      })
      .catch((err) => {
        console.error("Failed to load auto-playlist songs for card:", err);
      });
  });

  // Favourites/Recently Added/History use a fixed icon cover instead of a CoverStack —
  // they're rebuilt from the whole library on every load, so a coverstack of
  // whichever songs happen to be in them right now reads as arbitrary rather
  // than representative (unlike a genre, decade, BPM, or user playlist).
  let topCovers = $derived(
    kind === "genre" || kind === "decade" || kind === "bpm" || kind === "artist_tag" || kind === "daypart" ? songsToCoverStack(songs) : []
  );

  let updatedLabel = $derived.by(() => {
    if (
      (kind !== "genre" && kind !== "decade" && kind !== "bpm" && kind !== "artist_tag" && kind !== "missing_metadata" && kind !== "missing_musicbrainz" && kind !== "daypart") ||
      updated === undefined
    )
      return null;
    return formatRelativeDate(updated);
  });
</script>

{#snippet cover()}
  {#if (kind === "genre" || kind === "decade" || kind === "bpm" || kind === "artist_tag" || kind === "daypart") && topCovers.length > 0}
    {@const t = getPlaylistCardTheme(kind)}
    <PlaylistCoverFrame gradientClass={t.gradientClass}>
      <CoverStack covers={topCovers} hoverEffect={true} sizeClass="w-[82%] h-[82%]" />
    </PlaylistCoverFrame>
  {:else}
    {@const t = getPlaylistCardTheme(kind)}
    <PlaylistCoverFrame gradientClass={t.gradientClass}>
      {#if kind === "favourites"}
        <Heart class="w-10 h-10 {t.iconColorClass} fill-current" />
      {:else if kind === "recently_added"}
        <Clock class="w-10 h-10 {t.iconColorClass}" />
      {:else if kind === "most_played"}
        <TrendingUp class="w-10 h-10 {t.iconColorClass}" />
      {:else if kind === "history"}
        <Hourglass class="w-10 h-10 {t.iconColorClass}" />
      {:else if kind === "decade"}
        <Calendar class="w-10 h-10 {t.iconColorClass}" />
      {:else if kind === "genre"}
        <Music class="w-10 h-10 {t.iconColorClass}" />
      {:else if kind === "artist_tag"}
        <Tag class="w-10 h-10 {t.iconColorClass}" />
      {:else if kind === "bpm"}
        <Gauge class="w-10 h-10 {t.iconColorClass}" />
      {:else if kind === "missing_metadata"}
        <AlertTriangle class="w-10 h-10 {t.iconColorClass}" />
      {:else if kind === "missing_musicbrainz"}
        <img src="/picard-icon.png" alt="Picard" class="w-10 h-10 object-contain" />
      {:else}
        <SunHorizon class="w-10 h-10 {t.iconColorClass}" />
      {/if}
    </PlaylistCoverFrame>
  {/if}
{/snippet}

<PlaylistCardShell
  {widthClass}
  {onClick}
  {oncontextmenu}
  {onContextMenu}
  title={displayLabel}
  {subtitleLabel}
  updatedLabel={updatedLabel ?? ""}
  {trackCount}
  {cover}
/>
