// Frontend TypeScript types matching Rust models in models.rs

import { isWindows } from "../platform";

export type SongSource =
  | "unknown"
  | "local_file"
  | "collection"
  | "stream"
  | "tidal"
  | "subsonic"
  | "qobuz"
  | "soma_fm"
  | "radio_paradise"
  | "spotify"
  | "radio_browser";

export type FileType =
  | "UNKNOWN"
  | "MP3"
  | "FLAC"
  | "OGG_FLAC"
  | "OGG_VORBIS"
  | "OGG_OPUS"
  | "OGG_SPEEX"
  | "AAC"
  | "ALAC"
  | "AIFF"
  | "WAV"
  | "WAV_PACK"
  | "MPC"
  | "TRUE_AUDIO"
  | "APE"
  | "DSF"
  | "DSDIFF"
  | "ASF"
  | "STREAM";

export interface Song {
  id: number;
  source: SongSource;
  filetype: FileType;

  // Paths & URLs
  path?: string;
  url?: string;
  stream_url?: string;

  // Core metadata
  title?: string;
  titlesort?: string;
  artist?: string;
  artistsort?: string;
  album?: string;
  albumsort?: string;
  album_artist?: string;
  album_artist_sort?: string;
  composer?: string;
  composersort?: string;
  performer?: string;
  performersort?: string;
  grouping?: string;
  comment?: string;
  lyrics?: string;

  // Track info
  track?: number;
  disc?: number;
  year?: number;
  originalyear?: number;
  genre?: string;
  genresort?: string;
  compilation: boolean;

  // Extended tags
  bpm?: number;
  initial_key?: string;

  // Audio properties
  length_nanosec?: number;
  beginning_nanosec: number;
  end_nanosec: number;
  bitrate?: number;
  is_vbr?: boolean;
  samplerate?: number;
  bitdepth?: number;
  channels?: number;
  filesize?: number;
  mtime?: number;

  // Play statistics
  rating: number;
  playcount: number;
  skipcount: number;
  lastplayed?: number;
  lastseen?: number;
  added?: number;

  // Album art
  art_embedded: boolean;
  art_automatic?: string;
  art_manual?: string;
  art_unset: boolean;

  // CUE support
  cue_path?: string;

  // AcoustID / fingerprint
  acoustid_id?: string;
  acoustid_fingerprint?: string;
  fingerprint?: string;

  // MusicBrainz IDs
  musicbrainz_album_artist_id?: string;
  musicbrainz_artist_id?: string;
  musicbrainz_original_artist_id?: string;
  musicbrainz_album_id?: string;
  musicbrainz_original_album_id?: string;
  musicbrainz_recording_id?: string;
  musicbrainz_track_id?: string;
  musicbrainz_disc_id?: string;
  musicbrainz_release_group_id?: string;
  musicbrainz_work_id?: string;

  // EBU R128 loudness
  ebur128_integrated_loudness_lufs?: number;
  ebur128_loudness_range_lu?: number;

  // ReplayGain 2.0 tag fallback (#77)
  replaygain_track_gain?: number;
  replaygain_album_gain?: number;

  // Streaming service IDs
  artist_id?: string;
  album_id?: string;
  song_id?: string;

  /** True when the file is missing from disk (soft-deleted). Playlist items retain metadata. */
  unavailable: boolean;
  /** True when track is marked instrumental (online lyrics fetch bypassed). */
  is_instrumental?: boolean;
}

export type PlaylistItemType = "song" | "stream" | "streaming_service";

export interface PlaylistItem {
  id: number;
  playlist_id: number;
  position: number;
  uuid: string;
  item_type: PlaylistItemType;
  song?: Song;
  url?: string;
  stream_url?: string;
  additional_metadata?: string;
}

export interface Playlist {
  id: number;
  name: string;
  dynamic_enabled: boolean;
  dynamic_spec?: string;
  population_mode?: QueuePopulationMode;
  last_played_row?: number;
  created: number;
  updated: number;
  track_count: number;
  /** True for the app's single built-in Queue playlist (backend-computed —
   * never derive this from the playlist name). */
  is_queue: boolean;
}

/** Queue population bias — see #120. Tab order: All, Favourites, Familiar, Discover, Deep Cuts. */
export type QueuePopulationMode = "all" | "favourites" | "familiar" | "discover" | "deep_cuts";

export type ShuffleMode = "off" | "all" | "inside_album" | "albums" | "artists";
export type RepeatMode = "off" | "track" | "album" | "playlist" | "intro";
export type PlayState = "stopped" | "playing" | "paused";
export type LoudnessGainSource = "disabled" | "analyzed" | "replay_gain" | "fallback";

export interface PlaybackState {
  state: PlayState;
  current_song?: Song;
  playlist_id?: number;
  playlist_item_uuid?: string;
  position_nanosec: number;
  volume: number;
  shuffle_mode: ShuffleMode;
  repeat_mode: RepeatMode;
  stop_after_current: boolean;
  loudness_source: LoudnessGainSource;
  loudness_gain_db?: number;
  remaining_playlist_items?: number;
}

export interface MusicDirectory {
  id: number;
  path: string;
  subdirs: boolean;
  is_available?: boolean;
}

export type ScanPhase = "discovering" | "reading_tags" | "updating" | "done";

export interface ScanProgress {
  phase: ScanPhase;
  scanned: number;
  total: number;
  current_path?: string;
  silent: boolean;
}

export type BatchPhase = "removing" | "adding" | "done";

export interface BatchProgress {
  batch_id: number;
  current_count: number;
  total_count: number;
  phase: BatchPhase;
}

export interface LibraryStats {
  total_songs: number;
  total_artists: number;
  total_albums: number;
  total_duration_nanosec: number;
  total_filesize_bytes: number;
}

/** Whether the on-disk database's schema is ahead of what this build understands —
 *  e.g. a newer build of Luminous opened it previously. When true, song queries that
 *  name a since-added/removed column fail, making the library look empty even though
 *  it isn't; see LibraryWelcome.svelte. */
export interface DbSchemaStatus {
  db_version: number;
  app_version: number;
  db_newer_than_app: boolean;
}

export interface AlbumItem {
  artist: string | null;
  artist_sort?: string | null;
  album: string | null;
  albumsort?: string | null;
  year: number | null;
  track_count: number;
  disc_count: number;
  art_embedded: boolean;
  art_automatic: string | null;
  art_manual: string | null;
  genre?: string | null;
  sample_song_id?: number | null;
  /** Independent album rating (-1 = unrated, else 0.5–5.0), separate from any song's rating. */
  rating: number;
  added?: number | null;
  /** Sum of every track's length_nanosec; 0 for AlbumItems built outside get_albums() (e.g. Home carousels). */
  total_duration_nanosec: number;
}

/** One album's entry in the Home "Top Albums" weekly chart (#662). */
export interface TopAlbumItem {
  album: AlbumItem;
  rank: number;
  /** Rank in the prior UTC calendar week, or null if not in last week's chart ("new"). */
  previous_rank: number | null;
  /** Best (lowest) rank this album has ever held, including the current week. */
  peak_rank: number;
  /** Distinct weeks this album has appeared in the chart, including the current one. */
  weeks_on_chart: number;
  movement: "new" | "rising" | "falling" | "steady";
}

export interface ArtistItem {
  name: string | null;
  sort_artist?: string | null;
  album_count: number;
  song_count: number;
  total_playcount?: number;
  genre?: string | null;
}

export interface ArtistSocialLink {
  platform: string;
  handle_or_url: string;
}

export interface ArtistProfile {
  artist_key: string;
  website?: string | null;
  tags: string[];
  social_links: ArtistSocialLink[];
  bio?: string | null;
}

/** A Luminous-native song tag (#224), independent of the embedded `Song.genre`. */
export interface Tag {
  name: string;
  song_count: number;
}

/** One child entry under a {@link GenreGroup}'s main tag. */
export interface TagCount {
  name: string;
  song_count: number;
}

/**
 * One node of the emergent tag-hierarchy graph — a tag that has appeared as
 * a song's first ("main category") tag on at least one song, with every tag
 * seen as a subgenre of it. A tag can appear as a child under more than one
 * group if different songs disagree about its main category.
 */
export interface GenreGroup {
  main_tag: string;
  song_count: number;
  children: TagCount[];
}

/** One sub-genre chip assigned under a {@link TagGroup} (#545). A tag that's
 * also the name of some (other) top-level group elsewhere in the library
 * never appears here — the backend strips that link on sight. */
export interface TagGroupChild {
  name: string;
  song_count: number;
}

/** One primary-genre "card" in the persisted Genres curation hierarchy
 * (#545) — distinct from the emergent, per-song-order {@link GenreGroup}. */
export interface TagGroup {
  name: string;
  color_index: number;
  song_count: number;
  children: TagGroupChild[];
}

/**
 * Converts a custom luminous-art protocol URI (e.g. luminous-art://...)
 * to a platform-appropriate URL (e.g. http://luminous-art.localhost/ on Windows).
 */
export function getCoverArtUrl(uri: string | null | undefined): string | null {
  if (!uri || typeof uri !== "string") return null;
  if (uri.startsWith("luminous-art://")) {
    const isMock = typeof window !== "undefined" && (
      (window as any).__LUMINOUS_MOCK_LIBRARY__ || 
      (window as any).mockSettings
    );
    if (isMock) {
      let cleanPath = uri.replace("luminous-art://", "");
      if (cleanPath.startsWith("local/")) {
        cleanPath = cleanPath.slice(6);
      }
      if (cleanPath.includes(":/") || cleanPath.includes(":\\") || cleanPath.startsWith("/")) {
        return `/local-art/${encodeURIComponent(cleanPath)}`;
      }
      if (cleanPath.startsWith("album-")) {
        // Real DB rows already include the extension (see covermanager.rs); the
        // dev server's /covers/ route falls back to trying .jpg/.png if not.
        return `/covers/${cleanPath}`;
      }
      return `/fixtures/${cleanPath}`;
    }
    if (isWindows) {
      return uri.replace("luminous-art://", "http://luminous-art.localhost/");
    }
  }
  return uri;
}

/**
 * Resolves an art_manual or art_automatic string (which may be a remote HTTP URL,
 * a cached embedded art filename like "album-123.jpg", or an absolute local file path)
 * into a proper platform webview URL.
 */
export function resolveArtUrl(art: string | null | undefined): string | null {
  if (!art || typeof art !== "string") return null;
  if (art.startsWith("http://") || art.startsWith("https://")) {
    return art;
  }
  if (art.startsWith("luminous-art://")) {
    return getCoverArtUrl(art);
  }
  if (art.startsWith("album-")) {
    return getCoverArtUrl(`luminous-art://${art}`);
  }
  return getCoverArtUrl(`luminous-art://local/${art}`);
}

export type HomeItem =
  | { type: "song"; song: Song }
  | { type: "album"; album: AlbumItem; chart?: TopAlbumChartInfo }
  | { type: "playlist"; playlist: Playlist };

/** Chart metadata attached to an Album {@link HomeItem} for `HomeRowList`'s
 * `variant="chart"` (the Home "Top Albums" row, #662) — carries the same
 * fields as {@link TopAlbumItem} minus the (redundant) nested `album`. */
export type TopAlbumChartInfo = Omit<TopAlbumItem, "album">;

/** A user-pinned Home-shelf entry (#222) — a superset of {@link HomeItem} that
 * also allows Artist, since pins are explicitly user-curated across all four
 * browsable entity types. */
export type PinnedItemType = "song" | "album" | "artist" | "playlist" | "auto_playlist";

/** A pinned auto-playlist (genre/decade/BPM/artist-tag/Favourites/Recently
 * Added/Most Played/History) — enough of {@link AutoPlaylistRef}'s shape for
 * `AutoPlaylistCard` to render it directly. Favourites/Recently Added/Most
 * Played/History have no backing playlist row, so `playlistId`/`updated` are
 * absent for those kinds. */
export interface AutoPlaylistItem {
  kind: "favourites" | "recently_added" | "most_played" | "history" | "genre" | "decade" | "bpm" | "artist_tag" | "missing_metadata";
  genre?: string;
  artistTag?: string;
  decade?: string;
  bpm?: string;
  playlistId?: number;
  updated?: number;
  trackCount: number;
}

export type PinnedItem =
  | { type: "song"; song: Song }
  | { type: "album"; album: AlbumItem }
  | { type: "artist"; artist: ArtistItem }
  | { type: "playlist"; playlist: Playlist }
  | { type: "auto_playlist"; autoPlaylist: AutoPlaylistItem };

/** The stable ref_key for a pinned auto-playlist: the bare kind for
 * Favourites/Recently Added/Most Played/History (no backing row to key on),
 * or `kind:selector` for genre/decade/bpm/artist_tag — keyed by the selector
 * value (genre name, decade, bpm spec, artist tag) rather than the
 * materialized playlist's id, since that row can be dropped and recreated by
 * a background sync while the selector stays stable. */
export function autoPlaylistRefKeyFor(ref: {
  kind: string;
  genre?: string;
  decade?: string;
  bpm?: string;
  artistTag?: string;
}): string {
  switch (ref.kind) {
    case "genre":
      return `genre:${ref.genre ?? ""}`;
    case "decade":
      return `decade:${ref.decade ?? ""}`;
    case "bpm":
      return `bpm:${ref.bpm ?? ""}`;
    case "artist_tag":
      return `artist_tag:${ref.artistTag ?? ""}`;
    default:
      return ref.kind;
  }
}

/** The `(item_type, ref_key)` identity Luminous stores for a pin — song/playlist
 * id as a string, bare album title, or effective-artist name. Shared by the
 * pinned store and every pin/unpin entry point so they agree on how to key
 * each item type. */
export function pinnedRefKeyFor(item: PinnedItem): string {
  switch (item.type) {
    case "song":
      return String(item.song.id);
    case "album":
      return item.album.album ?? "";
    case "artist":
      return item.artist.name ?? "";
    case "playlist":
      return String(item.playlist.id);
    case "auto_playlist":
      return autoPlaylistRefKeyFor(item.autoPlaylist);
  }
}

// What the user was inside when a play started — lets "Recently Played"
// show an Album/Playlist card instead of always collapsing to a Song.
export type PlayContext =
  | { type: "song" }
  | { type: "album"; album: string; albumArtist?: string }
  | { type: "playlist"; playlistId: number };

export type RecentSearchKind = "query" | "artist" | "album" | "song" | "playlist";

export interface RecentSearchItem {
  id: string;
  kind: RecentSearchKind;
  title: string;
  subtitle?: string;
  query?: string;
  artUrl?: string | null;
  entityId?: string | number;
  timestamp: number;
}

