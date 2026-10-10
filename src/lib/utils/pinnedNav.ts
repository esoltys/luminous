import type { Component } from "svelte";
import type {
  AutoPlaylistItem,
  PinnedItem,
  PinnedItemType,
  ArtistItem,
  Playlist,
} from "../types";
import { pinnedRefKeyFor } from "../types";
import { navigationStore } from "../stores/navigation.svelte";
import { playerStore } from "../stores/player.svelte";
import { playlistsStore } from "../stores/playlists.svelte";
import { i18n } from "../stores/i18n.svelte";
import { getPlaylistDisplayName } from "./playlist";

/**
 * Whether a pinned item has content to display.
 * Empty playlists stay pinned in storage but aren't shown until they have songs.
 */
export function hasPinnedContent(item: PinnedItem): boolean {
  if (item.type === "playlist") return item.playlist.track_count > 0;
  if (item.type === "auto_playlist") return item.autoPlaylist.trackCount > 0;
  return true;
}

import {
  DiscIcon as DiscAlbum,
  MicrophoneStageIcon as Mic2,
  MusicNotesIcon as Music,
  PlaylistIcon as ListMusic,
} from "phosphor-svelte";
import { getPlaylistCardTheme } from "./playlistCardTheme";

/**
 * Presentation-ready projection of a pinned entity for navigation components.
 */
export interface NavigablePin {
  /** Unique stable key for keyed rendering (e.g. "album:Abbey Road"). */
  readonly id: string;

  /** The underlying raw pinned item. */
  readonly rawItem: PinnedItem;

  /** Entity type discriminator. */
  readonly type: PinnedItemType;

  /** Primary user-facing label (song title, album name, artist name, playlist name). */
  readonly title: string;

  /** Optional secondary text (e.g. artist name for songs/albums). */
  readonly subtitle?: string;

  /** Phosphor icon component for fallback or entity representation. */
  readonly icon: Component<any>;

  /** Cover art resolution properties when applicable (songs and albums). */
  readonly coverArt?: {
    songId?: number;
    artEmbedded?: boolean;
    artAutomatic?: string | null;
    artManual?: string | null;
  };

  /** Backing artist entity if type === 'artist'. */
  readonly artist?: ArtistItem;

  /** Backing playlist entity if type === 'playlist'. */
  readonly playlist?: Playlist;

  /** Backing auto-playlist entity if type === 'auto_playlist'. */
  readonly autoPlaylist?: AutoPlaylistItem;

  /** Whether this entity is currently active (playing or open in detail view). */
  readonly isActive: boolean;

  /** Activates the entity (starts playback or routes to detail view). */
  open(): void;
}

/**
 * Returns the localized or dynamically resolved display label for an auto-playlist.
 */
export function autoPlaylistLabel(ap: AutoPlaylistItem): string {
  switch (ap.kind) {
    case "favourites":
      return i18n.t("playlists.autoFavourites");
    case "recently_added":
      return i18n.t("playlists.autoRecentlyAdded");
    case "most_played":
      return i18n.t("playlists.autoMostPlayed");
    case "history":
      return i18n.t("playlists.autoHistory");
    case "daypart": {
      const pl = playlistsStore.playlists.find((p) => p.id === ap.playlistId);
      return pl ? getPlaylistDisplayName(pl) : i18n.t("playlists.daypartAutoPlaylist");
    }
    default: {
      const pl = playlistsStore.playlists.find((p) => p.id === ap.playlistId);
      return pl ? getPlaylistDisplayName(pl) : (ap.genre ?? ap.decade ?? ap.bpm ?? ap.artistTag ?? ap.kind);
    }
  }
}

function isAutoPlaylistActive(ap: AutoPlaylistItem): boolean {
  if (navigationStore.activeTab !== "playlists") return false;
  const sel = navigationStore.selectedAutoPlaylist;
  if (!sel) return false;
  if (sel.kind !== ap.kind) return false;
  if (ap.genre !== undefined && sel.genre !== ap.genre) return false;
  if (ap.decade !== undefined && sel.decade !== ap.decade) return false;
  if (ap.bpm !== undefined && sel.bpm !== ap.bpm) return false;
  if (ap.artistTag !== undefined && sel.artistTag !== ap.artistTag) return false;
  return true;
}

/**
 * Converts a raw PinnedItem into a NavigablePin projection.
 */
export function toNavigablePin(item: PinnedItem): NavigablePin {
  const id = `${item.type}:${pinnedRefKeyFor(item)}`;

  switch (item.type) {
    case "song": {
      const title =
        item.song.title ||
        (item.song.path ? item.song.path.split(/[/\\]/).pop() : null) ||
        i18n.t("collection.unknownSong");
      const subtitle = item.song.artist || undefined;
      const isActive = playerStore.currentSong?.id === item.song.id;
      return {
        id,
        rawItem: item,
        type: "song",
        title,
        subtitle,
        icon: Music,
        coverArt: {
          songId: item.song.id,
          artEmbedded: item.song.art_embedded,
          artAutomatic: item.song.art_automatic,
          artManual: item.song.art_manual,
        },
        isActive,
        open: () => playerStore.playSong(item.song.id),
      };
    }

    case "album": {
      const title = item.album.album || i18n.t("collection.unknownAlbum");
      const subtitle = item.album.artist || i18n.t("collection.variousArtists");
      const isActive =
        navigationStore.activeTab === "collection" &&
        navigationStore.selectedAlbumName === (item.album.album ?? "");
      return {
        id,
        rawItem: item,
        type: "album",
        title,
        subtitle,
        icon: DiscAlbum,
        coverArt: {
          songId: item.album.sample_song_id ?? undefined,
          artEmbedded: item.album.art_embedded,
          artAutomatic: item.album.art_automatic,
          artManual: item.album.art_manual,
        },
        isActive,
        open: () => navigationStore.viewAlbum(item.album.album || ""),
      };
    }

    case "artist": {
      const title = item.artist.name || i18n.t("collection.unknownArtist");
      const isActive =
        navigationStore.activeTab === "collection" &&
        navigationStore.selectedArtistName === (item.artist.name ?? "");
      return {
        id,
        rawItem: item,
        type: "artist",
        title,
        icon: Mic2,
        artist: item.artist,
        isActive,
        open: () => navigationStore.viewArtist(item.artist.name || ""),
      };
    }

    case "playlist": {
      const title = getPlaylistDisplayName(item.playlist);
      const isActive =
        navigationStore.activeTab === "playlists" &&
        navigationStore.selectedPlaylistId === item.playlist.id;
      return {
        id,
        rawItem: item,
        type: "playlist",
        title,
        icon: ListMusic,
        playlist: item.playlist,
        isActive,
        open: () => navigationStore.viewPlaylist(item.playlist.id),
      };
    }

    case "auto_playlist": {
      const title = autoPlaylistLabel(item.autoPlaylist);
      const theme = getPlaylistCardTheme(item.autoPlaylist.kind);
      const isActive = isAutoPlaylistActive(item.autoPlaylist);
      return {
        id,
        rawItem: item,
        type: "auto_playlist",
        title,
        icon: theme.icon,
        autoPlaylist: item.autoPlaylist,
        isActive,
        open: () =>
          navigationStore.viewAutoPlaylist({
            kind: item.autoPlaylist.kind,
            genre: item.autoPlaylist.genre,
            artistTag: item.autoPlaylist.artistTag,
            decade: item.autoPlaylist.decade,
            bpm: item.autoPlaylist.bpm,
            playlistId: item.autoPlaylist.playlistId,
            updated: item.autoPlaylist.updated,
          }),
      };
    }
  }
}

/**
 * Projects an array of PinnedItems into NavigablePin objects, filtering out
 * empty or non-materialized entities.
 */
export function getNavigablePins(items: PinnedItem[]): NavigablePin[] {
  return items.filter(hasPinnedContent).map(toNavigablePin);
}
