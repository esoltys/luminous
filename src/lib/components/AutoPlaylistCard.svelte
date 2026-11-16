<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import type { PlaylistItem, Song } from "../types";
  import { songsToCoverStack } from "../utils/covers";
  import { i18n } from "../stores/i18n.svelte";
  import { formatRelativeDate } from "../utils/date";
  import CoverStack from "./CoverStack.svelte";
  import PlaylistCardShell from "./PlaylistCardShell.svelte";
  import PlaylistCoverFrame from "./PlaylistCoverFrame.svelte";
  import PlaylistKindIcon from "./PlaylistKindIcon.svelte";
  import { getPlaylistCardTheme } from "../utils/playlistCardTheme";
  import { playlistsStore } from "../stores/playlists.svelte";
  import {
    getAutoPlaylistSubtitle,
    getPlaylistDisplayName,
    hasBackingRow,
    hasFillMode,
  } from "../utils/playlist";
  import { countryDisplayName } from "../utils/country";
  import { getLanguageDisplayName } from "../utils/language";
  import { toTitleCase } from "../utils/formatters";

  interface Props {
    label: string;
    kind:
      | "favourites"
      | "recently_added"
      | "most_played"
      | "history"
      | "genre"
      | "decade"
      | "bpm"
      | "artist_tag"
      | "missing_metadata"
      | "missing_musicbrainz"
      | "daypart"
      | "language"
      | "country";
    selector?: string;
    /** For materialized kinds: the backing playlist row id (refreshed at most every 24h). */
    playlistId?: number;
    /** For materialized kinds: when this playlist's songs were last (re)generated. */
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
    selector,
    playlistId,
    updated,
    trackCount,
    onClick,
    oncontextmenu,
    onContextMenu,
    widthClass = "w-full",
  }: Props = $props();

  let displayLabel = $derived.by(() => {
    if (hasBackingRow(kind) && playlistId !== undefined) {
      const pl = playlistsStore.playlists.find((p) => p.id === playlistId);
      if (pl) return getPlaylistDisplayName(pl);
    }
    if (kind === "language") return getLanguageDisplayName(selector || label);
    if (kind === "country") return countryDisplayName(selector, i18n.currentLocale) || label;
    if (kind === "daypart") return label || i18n.t("playlists.daypartAutoPlaylist");
    return kind === "artist_tag" ? toTitleCase(label) : label;
  });

  let subtitleLabel = $derived(getAutoPlaylistSubtitle({ kind, selector }));

  let songs = $state<Song[]>([]);

  $effect(() => {
    const k = kind;
    const s = selector;
    const pid = playlistId;
    const currentTrackCount = trackCount;

    const request =
      hasFillMode(k) && pid !== undefined
        ? invoke<PlaylistItem[]>("get_playlist_tracks", { playlistId: pid }).then((items) =>
            items.filter((item) => !!item.song).map((item) => item.song as Song),
          )
        : k === "daypart"
          ? Promise.resolve([])
          : k === "missing_musicbrainz"
            ? pid !== undefined
              ? invoke<PlaylistItem[]>("get_playlist_tracks", { playlistId: pid }).then((items) =>
                  items.filter((item) => !!item.song).map((item) => item.song as Song),
                )
              : invoke<Song[]>("get_songs_missing_musicbrainz_id", { limit: 50 })
            : k === "missing_metadata"
              ? pid !== undefined
                ? invoke<PlaylistItem[]>("get_playlist_tracks", { playlistId: pid }).then((items) =>
                    items.filter((item) => !!item.song).map((item) => item.song as Song),
                  )
                : invoke<Song[]>("get_songs_missing_metadata", { limit: 50 })
              : k === "favourites"
                ? invoke<Song[]>("get_favourite_songs")
                : k === "recently_added"
                  ? invoke<Song[]>("get_recently_added_songs", { limit: 50 })
                  : k === "most_played"
                    ? invoke<Song[]>("get_most_played_songs", { limit: 50 })
                    : k === "history"
                      ? invoke<Song[]>("get_recently_played_songs", { limit: 50 })
                      : k === "decade"
                        ? invoke<Song[]>("get_songs_by_decade", { decade: s ?? "", limit: 50 })
                        : k === "bpm"
                          ? invoke<Song[]>("get_songs_by_bpm", { spec: s ?? "", limit: 50 })
                          : k === "artist_tag"
                            ? invoke<Song[]>("get_songs_by_artist_tag", {
                                tag: s ?? "",
                                limit: 50,
                              })
                            : k === "country"
                              ? invoke<Song[]>("get_songs_by_artist_country", {
                                  country: s ?? "",
                                  limit: 50,
                                })
                              : k === "language"
                                ? invoke<Song[]>("get_songs_by_language", {
                                    language: s ?? "",
                                    limit: 50,
                                  })
                                : invoke<Song[]>("get_songs_by_curated_tag", {
                                    tagName: s ?? "",
                                    limit: 50,
                                  });

    request
      .then((res) => {
        if (
          kind === k &&
          selector === s &&
          playlistId === pid &&
          trackCount === currentTrackCount
        ) {
          songs = res;
        }
      })
      .catch((err) => {
        console.error("Failed to load auto-playlist songs for card:", err);
      });
  });

  // Missing Metadata keeps its fixed icon cover — it's a to-do list rather
  // than a set of songs whose covers say anything about the playlist.
  let topCovers = $derived(kind === "missing_metadata" ? [] : songsToCoverStack(songs));

  let updatedLabel = $derived.by(() => {
    if (!hasBackingRow(kind) || updated === undefined) return null;
    return formatRelativeDate(updated);
  });
</script>

{#snippet cover()}
  {#if topCovers.length > 0}
    {@const t = getPlaylistCardTheme(kind)}
    <PlaylistCoverFrame gradientClass={t.gradientClass}>
      <CoverStack covers={topCovers} hoverEffect sizeClass="w-[82%] h-[82%]" />
    </PlaylistCoverFrame>
  {:else}
    {@const t = getPlaylistCardTheme(kind)}
    <PlaylistCoverFrame gradientClass={t.gradientClass}>
      <PlaylistKindIcon {kind} sizeClass="w-10 h-10" />
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
