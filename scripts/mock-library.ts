// Loads the data the Tauri IPC mock serves: either the small bundled fixture
// library (mock-data.ts) or, if configured, a live read from a real Luminous
// SQLite database. See mock-config.example.json for the config shape.
import { existsSync, readFileSync } from "node:fs";
import * as os from "node:os";
import * as path from "node:path";
import { fileURLToPath } from "node:url";
import type { AlbumItem, ArtistItem, Playlist, Song } from "../src/lib/types/index";
import { FALLBACK_LYRICS, FALLBACK_PLAYLISTS, FALLBACK_SONGS } from "./mock-data";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const CONFIG_PATH = path.join(__dirname, "mock-config.json");
const LOCAL_CONFIG_PATH = path.join(__dirname, "mock-config.local.json");
const EXAMPLE_CONFIG_PATH = path.join(__dirname, "mock-config.example.json");
const TAURI_CONF_PATH = path.join(__dirname, "../src-tauri/tauri.conf.json");

/**
 * The real app's db lives at `{tauri app_data_dir}/luminous.db` (see
 * src-tauri/src/db.rs). Tauri's `app_data_dir()` resolves to the *Roaming*
 * AppData folder on Windows (not Local — a common mix-up), Application
 * Support on macOS, and XDG_DATA_HOME on Linux. Reading the identifier from
 * tauri.conf.json instead of hardcoding it keeps this in sync automatically.
 */
function defaultDbPath(): string | undefined {
  let identifier: string;
  try {
    identifier = JSON.parse(readFileSync(TAURI_CONF_PATH, "utf8")).identifier;
  } catch {
    return undefined;
  }
  if (!identifier) return undefined;

  const home = os.homedir();
  let appDataDir: string;
  switch (process.platform) {
    case "win32":
      appDataDir = path.join(process.env.APPDATA ?? path.join(home, "AppData", "Roaming"), identifier);
      break;
    case "darwin":
      appDataDir = path.join(home, "Library", "Application Support", identifier);
      break;
    default:
      appDataDir = path.join(process.env.XDG_DATA_HOME ?? path.join(home, ".local", "share"), identifier);
  }
  return path.join(appDataDir, "luminous.db");
}

// Ordinal -> serde string, mirroring the #[serde(rename_all = ...)] enums in
// src-tauri/src/models.rs. The real backend does this conversion for us; a
// raw SQLite read has to do it by hand.
const SONG_SOURCES = [
  "unknown", "local_file", "collection", "stream", "tidal", "subsonic",
  "qobuz", "soma_fm", "radio_paradise", "spotify", "radio_browser",
] as const;

const FILE_TYPES = [
  "UNKNOWN", "MP3", "FLAC", "OGG_FLAC", "OGG_VORBIS", "OGG_OPUS", "OGG_SPEEX",
  "AAC", "ALAC", "AIFF", "WAV", "WAV_PACK", "MPC", "TRUE_AUDIO", "APE",
  "DSF", "DSDIFF", "ASF", "STREAM",
] as const;

/** The subset of per-screenshot settings that also have a run-wide fallback under `default`. */
export interface MockConfigDefaults {
  language?: string;
  theme?: string;
  sidebarOpen?: boolean;
  rightPanelOpen?: boolean;
  sidebarWidth?: number;
  positionSeconds?: number;
  /** Song title to feature in the mock "now playing" state and screenshots. */
  featuredSong?: string;
  /** Artist name to feature in screenshots (e.g. the artist-detail view). */
  featuredArtist?: string;
}

export interface ScreenshotConfig extends MockConfigDefaults {
  name: string;
  tab: string;
  subTab: string;
  filename: string;
  action?: string;
  isImmersive?: boolean;
}

export interface MockConfig {
  /** Absolute path to a real luminous.db. When set (and readable), overrides the bundled fixture data. Not overridable per-screenshot. */
  dbPath?: string;
  /** Cap on how many songs to pull from a real database. Defaults to 2000. Not overridable per-screenshot. */
  songLimit?: number;
  /** Run-wide fallback values for the settings each screenshot entry may override. */
  default?: MockConfigDefaults;
  screenshots?: ScreenshotConfig[];
}

/** Effective settings for a single screenshot: `screenshot.*` wins, falling back to `config.default.*`, then a hardcoded default. */
export interface ResolvedScreenshotSettings {
  language: string;
  theme: string;
  sidebarOpen: boolean;
  rightPanelOpen: boolean;
  sidebarWidth: number;
  positionSeconds: number;
  featuredSong?: string;
  featuredArtist?: string;
}

export function resolveScreenshotSettings(
  config: MockConfig,
  screenshot: Partial<ScreenshotConfig> = {}
): ResolvedScreenshotSettings {
  const d = config.default ?? {};
  return {
    language: screenshot.language ?? d.language ?? "en",
    theme: screenshot.theme ?? d.theme ?? "nordic-blue",
    sidebarOpen: screenshot.sidebarOpen ?? d.sidebarOpen ?? true,
    rightPanelOpen: screenshot.rightPanelOpen ?? d.rightPanelOpen ?? false,
    sidebarWidth: screenshot.sidebarWidth ?? d.sidebarWidth ?? 64,
    positionSeconds: screenshot.positionSeconds ?? d.positionSeconds ?? 122,
    featuredSong: screenshot.featuredSong ?? d.featuredSong,
    featuredArtist: screenshot.featuredArtist ?? d.featuredArtist,
  };
}

export interface MockLibrary {
  songs: Song[];
  albums: AlbumItem[];
  artists: ArtistItem[];
  playlists: Playlist[];
  playlistTracks: Record<number, Song[]>;
  lyrics: string;
  source: "database" | "fallback";
  /** The db file actually used, when source is "database" — covers/ lives alongside it. */
  dbPath?: string;
}

function readJsonConfig(configPath: string): MockConfig {
  try {
    return JSON.parse(readFileSync(configPath, "utf8"));
  } catch (err) {
    console.warn(`[Mock Library] Failed to parse ${configPath}:`, err);
    return {};
  }
}

/**
 * Reads scripts/mock-config.local.json (gitignored) if present. Otherwise
 * falls back to the "featured" defaults from mock-config.example.json, minus
 * its placeholder dbPath, so a fresh clone still gets a sensible screenshot
 * without warning about a database that was never configured.
 */
export function loadMockConfig(): MockConfig {
  if (existsSync(CONFIG_PATH)) return readJsonConfig(CONFIG_PATH);
  if (existsSync(LOCAL_CONFIG_PATH)) return readJsonConfig(LOCAL_CONFIG_PATH);
  if (existsSync(EXAMPLE_CONFIG_PATH)) {
    const { dbPath: _dbPath, ...defaults } = readJsonConfig(EXAMPLE_CONFIG_PATH);
    return defaults;
  }
  return {};
}

export function deriveAlbums(songs: Song[]): AlbumItem[] {
  const byKey = new Map<string, AlbumItem>();
  for (const song of songs) {
    if (!song.album) continue;
    const artist = song.album_artist || song.artist || null;
    const key = `${song.album}::${artist ?? ""}`;
    const existing = byKey.get(key);
    if (existing) {
      existing.track_count += 1;
      existing.year = existing.year ?? song.year ?? null;
      existing.art_embedded = existing.art_embedded || song.art_embedded;
      existing.art_automatic = existing.art_automatic ?? song.art_automatic ?? null;
      existing.art_manual = existing.art_manual ?? song.art_manual ?? null;
    } else {
      byKey.set(key, {
        album: song.album,
        artist,
        year: song.year ?? null,
        track_count: 1,
        art_embedded: song.art_embedded,
        art_automatic: song.art_automatic ?? null,
        art_manual: song.art_manual ?? null,
      });
    }
  }
  return [...byKey.values()];
}

export function deriveArtists(songs: Song[]): ArtistItem[] {
  const albumsByArtist = new Map<string, Set<string>>();
  const songCountByArtist = new Map<string, number>();
  for (const song of songs) {
    const artist = song.album_artist || song.artist;
    if (!artist) continue;
    songCountByArtist.set(artist, (songCountByArtist.get(artist) ?? 0) + 1);
    if (song.album) {
      if (!albumsByArtist.has(artist)) albumsByArtist.set(artist, new Set());
      albumsByArtist.get(artist)!.add(song.album);
    }
  }
  return [...songCountByArtist.keys()]
    .sort((a, b) => a.localeCompare(b))
    .map((name) => ({
      name,
      album_count: albumsByArtist.get(name)?.size ?? 0,
      song_count: songCountByArtist.get(name) ?? 0,
    }));
}

function rowToSong(row: Record<string, unknown>): Song {
  return {
    ...(row as unknown as Song),
    source: SONG_SOURCES[Number(row.source)] ?? "unknown",
    filetype: FILE_TYPES[Number(row.filetype)] ?? "UNKNOWN",
    compilation: !!row.compilation,
    art_embedded: !!row.art_embedded,
    art_unset: !!row.art_unset,
    unavailable: !!row.unavailable,
  };
}

function rowToPlaylist(row: Record<string, unknown>): Playlist {
  return {
    ...(row as unknown as Playlist),
    dynamic_enabled: !!row.dynamic_enabled,
  };
}

interface DbLibrary {
  songs: Song[];
  playlists: Playlist[];
  playlistTracks: Record<number, Song[]>;
}

async function loadFromDatabase(dbPath: string, limit: number, silentIfMissing = false): Promise<DbLibrary | null> {
  if (!existsSync(dbPath)) {
    if (!silentIfMissing) {
      console.warn(`[Mock Library] dbPath "${dbPath}" does not exist; using bundled fixture data.`);
    }
    return null;
  }
  try {
    // node:sqlite is experimental (Node 22+); imported lazily so this file
    // still works in runtimes without it — it just falls back to fixtures.
    const { DatabaseSync } = await import("node:sqlite");
    const db = new DatabaseSync(dbPath, { readOnly: true });
    try {
      const songRows = db
        .prepare("SELECT * FROM songs WHERE unavailable = 0 ORDER BY artist, album, disc, track LIMIT ?")
        .all(limit) as Record<string, unknown>[];
      const songs = songRows.map(rowToSong);

      const playlistRows = db
        .prepare(
          `SELECT id, name, dynamic_enabled, dynamic_spec, last_played_row, created,
                  (SELECT COUNT(*) FROM playlist_items WHERE playlist_id = playlists.id) AS track_count
           FROM playlists`
        )
        .all() as Record<string, unknown>[];
      const playlists = playlistRows.map(rowToPlaylist);

      const trackStmt = db.prepare(
        `SELECT songs.* FROM playlist_items
         JOIN songs ON songs.id = playlist_items.song_id
         WHERE playlist_items.playlist_id = ?
         ORDER BY playlist_items.position`
      );
      const playlistTracks: Record<number, Song[]> = {};
      for (const playlist of playlists) {
        playlistTracks[playlist.id] = (trackStmt.all(playlist.id) as Record<string, unknown>[]).map(rowToSong);
      }

      return { songs, playlists, playlistTracks };
    } finally {
      db.close();
    }
  } catch (err) {
    console.warn(`[Mock Library] Could not read local database (${dbPath}):`, (err as Error).message);
    return null;
  }
}

export async function loadMockLibrary(config: MockConfig = loadMockConfig()): Promise<MockLibrary> {
  const limit = config.songLimit ?? 2000;
  // An explicit dbPath is a firm request — warn if it's wrong. Falling back
  // to the auto-detected Tauri app-data location is best-effort and should
  // stay quiet when nothing's there (e.g. CI, or no desktop app installed).
  const dbPath = config.dbPath || defaultDbPath();
  const fromDb = dbPath ? await loadFromDatabase(dbPath, limit, !config.dbPath) : null;

  const songs = fromDb?.songs ?? FALLBACK_SONGS;
  const playlists = fromDb?.playlists ?? FALLBACK_PLAYLISTS;

  return {
    songs,
    albums: deriveAlbums(songs),
    artists: deriveArtists(songs),
    playlists,
    playlistTracks: fromDb?.playlistTracks ?? {},
    lyrics: FALLBACK_LYRICS,
    source: fromDb ? "database" : "fallback",
    dbPath: fromDb ? dbPath : undefined,
  };
}

export interface FeaturedSelection {
  song?: Song;
  artist?: string;
}

export function resolveFeatured(
  library: MockLibrary,
  selection: { featuredSong?: string; featuredArtist?: string }
): FeaturedSelection {
  const song =
    (selection.featuredSong && library.songs.find((s) => s.title === selection.featuredSong)) || library.songs[0];
  const artist =
    (selection.featuredArtist && library.artists.some((a) => a.name === selection.featuredArtist)
      ? selection.featuredArtist
      : library.artists[0]?.name) ?? undefined;
  return { song, artist };
}
