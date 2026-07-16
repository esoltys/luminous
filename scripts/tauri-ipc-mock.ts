// Mocks the Tauri IPC bridge (window.__TAURI_INTERNALS__) so the SvelteKit
// frontend can run in a plain browser — no Rust backend required. Used by
// scripts/take-screenshots.ts (via Playwright's addInitScript) and by the
// Vite dev server at /tauri-ipc-mock.js for manual browser testing.
//
// Library data isn't embedded here: the host environment injects it as
// `window.__LUMINOUS_MOCK_LIBRARY__` / `window.__LUMINOUS_MOCK_FEATURED__`
// before this script runs (see scripts/mock-library.ts). The tiny dataset
// below only covers the case where this file is loaded completely standalone.
import type {
  AlbumItem,
  ArtistItem,
  FileType,
  Playlist,
  PlayState,
  RepeatMode,
  ShuffleMode,
  Song,
} from "../src/lib/types/index";

interface AppSettings {
  active_theme_id: string;
  custom_themes: string;
  active_tab: string;
  active_sub_tab: string;
  excluded_formats: string;
  [key: string]: string;
}

interface EqualizerState {
  enabled: boolean;
  preamp: number;
  gains: number[];
}

interface MockLibrary {
  songs: Song[];
  albums: AlbumItem[];
  artists: ArtistItem[];
  playlists: Playlist[];
  playlistTracks: Record<number, Song[]>;
  lyrics: string;
}

type IpcCallback = (data?: unknown) => void;

declare global {
  interface Window {
    mockSettings?: AppSettings;
    mockPlaybackPositionSec?: number;
    __LUMINOUS_MOCK_LIBRARY__?: MockLibrary;
    __LUMINOUS_MOCK_FEATURED__?: { song?: Song; artist?: string };
    __TAURI_INTERNALS__?: {
      transformCallback: (callback: IpcCallback, once?: boolean) => number;
      unregisterCallback: (id: number) => void;
      invoke: (cmd: string, args?: Record<string, unknown>) => Promise<unknown>;
      ipc: (message: { cmd?: string; params?: Record<string, unknown>; callback?: number; error?: number }) => void;
    };
  }
}

/** Tauri's real IPC glue stashes numbered `_<id>` callback functions on `window`. */
function getIpcCallback(id: number | undefined): IpcCallback | undefined {
  if (id === undefined) return undefined;
  return (window as unknown as Record<string, IpcCallback | undefined>)[`_${id}`];
}

(function () {
  console.log("[Tauri Mock] Initializing Tauri IPC Mock layer...");

  window.mockSettings = window.mockSettings || {
    active_theme_id: "nordic-blue",
    custom_themes: "[]",
    active_tab: "collection",
    active_sub_tab: "songs",
    excluded_formats: "[]",
  };

  const STANDALONE_FALLBACK_SONG: Song = {
    id: 1,
    source: "local_file",
    filetype: "FLAC" as FileType,
    path: "/Music/Placeholder Artist/Placeholder Album/01 Placeholder Song.flac",
    title: "Placeholder Song",
    artist: "Placeholder Artist",
    album: "Placeholder Album",
    genre: "Ambient",
    year: 2025,
    track: 1,
    disc: 1,
    compilation: false,
    length_nanosec: 180_000_000_000,
    beginning_nanosec: 0,
    end_nanosec: 0,
    bitrate: 900,
    samplerate: 44100,
    channels: 2,
    filesize: 20_000_000,
    rating: -1,
    playcount: 0,
    skipcount: 0,
    added: 1783727350,
    art_embedded: false,
    art_unset: false,
    unavailable: false,
  };

  const library: MockLibrary = window.__LUMINOUS_MOCK_LIBRARY__ ?? {
    songs: [STANDALONE_FALLBACK_SONG],
    albums: [],
    artists: [],
    playlists: [],
    playlistTracks: {},
    lyrics: "",
  };
  const featured = window.__LUMINOUS_MOCK_FEATURED__ ?? {};
  const featuredSong = featured.song ?? library.songs[0];

  const callbacks: Record<number, (data: unknown) => void> = {};
  let nextCallbackId = 1;
  const eventListeners: Record<string, number[]> = {};

  const EQ_PRESETS: Record<string, number[]> = {
    Rock: [4.0, 3.0, 2.0, -1.0, -2.0, -1.0, 1.0, 2.0, 3.0, 4.0],
    Pop: [-2.0, -1.0, 0.0, 2.0, 4.0, 4.0, 2.0, 0.0, -1.0, -2.0],
    Classical: [5.0, 3.0, 2.0, 2.0, -1.0, -1.0, 0.0, 2.0, 3.0, 4.0],
    Jazz: [3.0, 2.0, 1.0, 2.0, -1.0, -1.0, 0.0, 1.0, 2.0, 3.0],
    "Bass Boost": [6.0, 5.0, 4.0, 2.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0],
    "Vocal Boost": [-2.0, -2.0, -1.0, 1.0, 3.0, 4.0, 3.0, 1.0, -1.0, -2.0],
  };

  function makeWaveform(): number[] {
    const peaks: number[] = [];
    for (let i = 0; i < 150; i++) {
      const angle = (i / 150) * Math.PI * 6;
      const wave = Math.sin(angle) * Math.cos(angle * 2.3) * 0.4 + 0.5;
      const noise = Math.random() * 0.15;
      peaks.push(Math.round(Math.min(1, Math.max(0.1, wave + noise)) * 255));
    }
    return peaks;
  }

  const noop = async () => null;

  const commands: Record<string, (args: Record<string, unknown>) => unknown> = {
    get_all_app_settings: () => window.mockSettings,

    get_playback_state: () => {
      const posSec = window.mockPlaybackPositionSec ?? 122;
      return {
        state: "playing" as PlayState,
        current_song: featuredSong,
        playlist_id: 1,
        playlist_item_uuid: "item-uuid-1",
        position_nanosec: posSec * 1_000_000_000,
        volume: 0.75,
        shuffle_mode: "off" as ShuffleMode,
        repeat_mode: "all" as RepeatMode,
        stop_after_current: false,
      };
    },

    get_directories: () => [
      { id: 1, path: "C:\\Users\\ericj\\Music\\Retro Hits", subdirs: true },
      { id: 2, path: "C:\\Users\\ericj\\Music\\Studio Masters", subdirs: true },
    ],

    get_library_stats: () => ({
      total_songs: library.songs.length,
      total_artists: library.artists.length,
      total_albums: library.albums.length,
      total_duration_nanosec: library.songs.reduce((acc, s) => acc + (s.length_nanosec || 0), 0),
      total_filesize_bytes: library.songs.reduce((acc, s) => acc + (s.filesize || 0), 0),
    }),

    get_songs: () => library.songs,

    get_recently_played: (args) =>
      library.songs
        .filter((s) => s.lastplayed)
        .sort((a, b) => (b.lastplayed || 0) - (a.lastplayed || 0))
        .slice(0, (args.limit as number) || 10),

    get_most_frequently_played: (args) =>
      [...library.songs].sort((a, b) => (b.playcount || 0) - (a.playcount || 0)).slice(0, (args.limit as number) || 10),

    get_recently_added: (args) =>
      library.songs
        .filter((s) => s.added)
        .sort((a, b) => (b.added || 0) - (a.added || 0))
        .slice(0, (args.limit as number) || 10),

    get_albums: () => library.albums,
    get_artists: () => library.artists,

    get_songs_by_album: (args) => library.songs.filter((s) => s.album === args.album),
    get_songs_by_artist: (args) =>
      library.songs.filter((s) => s.artist === args.artist || s.album_artist === args.artist),

    get_playlists_by_artist: () => [],
    get_playlists: () => library.playlists,

    get_playlist_tracks: (args) => {
      const playlistId = args.playlistId as number;
      const tracks = library.playlistTracks[playlistId] ?? library.songs.slice(0, 3);
      return tracks.map((song, i) => ({
        id: i + 1,
        playlist_id: playlistId,
        position: i,
        uuid: `uuid-${i}`,
        item_type: "song",
        song,
      }));
    },

    get_waveform_data: () => makeWaveform(),
    get_lyrics: () => library.lyrics,

    get_equalizer_state: (): EqualizerState => ({
      enabled: true,
      preamp: 3.0,
      gains: [10.0, 8.0, 5.0, -3.0, -6.0, -4.0, 3.0, 6.0, 8.0, 10.0],
    }),

    load_equalizer_preset: (args): EqualizerState => ({
      enabled: true,
      preamp: 3.0,
      gains: EQ_PRESETS[args.presetName as string] ?? Array(10).fill(0.0),
    }),

    set_app_setting: (args) => {
      if (args.key && window.mockSettings) {
        window.mockSettings[args.key as string] = args.value as string;
      }
      return null;
    },

    "plugin:event|listen": (args) => {
      const event = args.event as string;
      const handler = args.handler as number;
      (eventListeners[event] ??= []).push(handler);
      return handler;
    },

    "plugin:event|unlisten": (args) => {
      const event = args.event as string;
      const eventId = args.eventId as number;
      if (eventListeners[event]) {
        eventListeners[event] = eventListeners[event].filter((h) => h !== eventId);
      }
      return null;
    },
  };

  const NOOP_COMMANDS = [
    "set_equalizer_enabled", "set_equalizer_preamp", "set_equalizer_band", "set_spectrum_enabled",
    "play_song", "play_songs", "play_playlist_item", "pause", "resume", "stop",
    "next_track", "previous_track", "seek_to", "set_volume", "set_shuffle_mode", "set_repeat_mode",
  ];
  for (const cmd of NOOP_COMMANDS) commands[cmd] = noop;

  async function invoke(cmd: string, args: Record<string, unknown> = {}): Promise<unknown> {
    console.log(`[Tauri Mock Invoke] cmd: ${cmd}`, args);
    const handler = commands[cmd];
    if (!handler) {
      console.warn(`[Tauri Mock] Unhandled command: ${cmd}`, args);
      return null;
    }
    return handler(args);
  }

  window.__TAURI_INTERNALS__ = {
    transformCallback(callback, once = false) {
      const id = nextCallbackId++;
      callbacks[id] = (data) => {
        if (once) delete callbacks[id];
        callback(data);
      };
      return id;
    },

    unregisterCallback(id) {
      delete callbacks[id];
    },

    invoke,

    ipc(message) {
      console.log("[Tauri Mock IPC] message:", message);
      if (message?.cmd === "plugin:event|listen") {
        invoke(message.cmd, message.params ?? {});
        getIpcCallback(message.callback)?.();
        return;
      }

      if (message?.cmd) {
        invoke(message.cmd, message.params ?? {})
          .then((res) => getIpcCallback(message.callback)?.(res))
          .catch((err) => getIpcCallback(message.error)?.(err));
      }
    },
  };

  // Simulate spectral FFT visualizer events periodically.
  setInterval(() => {
    const handlers = eventListeners["spectrum-data"];
    if (!handlers || handlers.length === 0) return;

    // 32 bars, biased toward bass energy, with a rhythmic bounce + jitter.
    const mockFFT = Array.from({ length: 32 }, (_, i) => {
      const energy = i < 6 ? 0.7 : i < 18 ? 0.45 : 0.2;
      const bounce = Math.sin(Date.now() / 150 + i) * 0.15;
      const jitter = Math.random() * 0.15;
      return Math.min(1.0, Math.max(0.02, energy + bounce + jitter));
    });

    for (const handlerId of handlers) {
      callbacks[handlerId]?.({ event: "spectrum-data", payload: mockFFT });
    }
  }, 80); // ~12 FPS is great for screenshots without loading CPU
})();
