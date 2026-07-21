// Frontend TypeScript types matching Rust models in models.rs

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
  compilation: boolean;

  // Extended tags
  bpm?: number;
  mood?: string;
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
  auto_play?: boolean;
  last_played_row?: number;
  created: number;
  updated: number;
  track_count: number;
}

export type ShuffleMode = "off" | "all" | "inside_album" | "albums" | "artists";
export type RepeatMode = "off" | "track" | "album" | "playlist" | "one_by_one" | "intro";
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
}

export type ScanPhase = "discovering" | "reading_tags" | "updating" | "done";

export interface ScanProgress {
  phase: ScanPhase;
  scanned: number;
  total: number;
  current_path?: string;
}

export interface LibraryStats {
  total_songs: number;
  total_artists: number;
  total_albums: number;
  total_duration_nanosec: number;
  total_filesize_bytes: number;
}

export interface AlbumItem {
  artist: string | null;
  album: string | null;
  year: number | null;
  track_count: number;
  art_embedded: boolean;
  art_automatic: string | null;
  art_manual: string | null;
}

export interface ArtistItem {
  name: string | null;
  album_count: number;
  song_count: number;
  genre?: string | null;
}

/**
 * Converts a custom luminous-art protocol URI (e.g. luminous-art://...)
 * to a platform-appropriate URL (e.g. http://luminous-art.localhost/ on Windows).
 */
export function getCoverArtUrl(uri: string | null | undefined): string | null {
  if (!uri) return null;
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
    const isWindows = typeof navigator !== "undefined" && navigator.userAgent.includes("Windows");
    if (isWindows) {
      return uri.replace("luminous-art://", "http://luminous-art.localhost/");
    }
  }
  return uri;
}

export type HomeItem =
  | { type: "song"; song: Song }
  | { type: "album"; album: AlbumItem };
