import "@testing-library/jest-dom";
import { vi } from "vitest";

// Mock localStorage for jsdom/happy-dom
const localStorageMock = (() => {
  let store: Record<string, string> = {};
  return {
    getItem: (key: string) => store[key] || null,
    setItem: (key: string, value: string) => { store[key] = value.toString(); },
    removeItem: (key: string) => { delete store[key]; },
    clear: () => { store = {}; },
    key: (index: number) => Object.keys(store)[index] || null,
    get length() { return Object.keys(store).length; }
  };
})();

Object.defineProperty(globalThis, "localStorage", {
  value: localStorageMock,
  writable: true,
  configurable: true
});

if (typeof window !== "undefined") {
  Object.defineProperty(window, "localStorage", {
    value: localStorageMock,
    writable: true,
    configurable: true
  });
}

// Mock ResizeObserver for jsdom
if (typeof globalThis.ResizeObserver === "undefined") {
  globalThis.ResizeObserver = class {
    observe() {}
    unobserve() {}
    disconnect() {}
  };
}

if (typeof window !== "undefined" && typeof (window as any).ResizeObserver === "undefined") {
  (window as any).ResizeObserver = globalThis.ResizeObserver;
}

vi.mock("@tauri-apps/api/core", () => {
  return {
    invoke: vi.fn().mockImplementation(async (cmd, args) => {
      console.log(`[Tauri Mock Invoke] ${cmd}`, args);
      // Return default values for typical player/theme queries
      if (cmd === "get_playback_state") {
        return {
          state: "stopped",
          current_song: null,
          playlist_id: null,
          playlist_item_uuid: null,
          position_nanosec: 0,
          volume: 1.0,
          shuffle_mode: "off",
          repeat_mode: "off",
          stop_after_current: false,
        };
      }
      if (cmd === "has_acoustid_env_key") {
        return false;
      }
      if (cmd === "validate_playlist_name") {
        return { valid: true, reason: null };
      }
      if (cmd === "get_ui_preferences") {
        return {
          rating_style: "heart",
          seekbar_mode: "waveform",
          acoustid_api_key: "",
          albums_view_mode: "cards",
          artists_view_mode: "cards",
          playlists_auto_view_mode: "cards",
          playlists_custom_view_mode: "cards",
        };
      }
      if (cmd === "open_and_play") {
        return { played: 1, skipped: 0 };
      }
      if (cmd === "add_paths_to_queue") {
        return { added: 1, skipped: 0 };
      }
      if (cmd === "is_shift_key_held") {
        return false;
      }
      // The backend bootstraps the built-in Queue playlist at startup, so a
      // playlist listing always contains it — model that guarantee here.
      if (cmd === "get_playlists") {
        return [
          { id: 1, name: "Queue", dynamic_enabled: false, created: 0, updated: 0, track_count: 0, is_queue: true },
        ];
      }
      if (cmd === "get_library_snapshot") {
        return { songs: [], albums: [], artists: [] };
      }
      if (cmd === "get_all_artist_profiles") {
        return [];
      }
      if (cmd === "get_directories") {
        return [];
      }
      if (cmd === "get_library_stats") {
        return { song_count: 0, album_count: 0, artist_count: 0, total_duration_nanosec: 0 };
      }
      if (cmd === "get_db_schema_status") {
        return { is_ready: true, schema_version: 1 };
      }
      if (
        cmd === "get_favourite_songs" ||
        cmd === "get_recently_added_songs" ||
        cmd === "get_recently_played_songs" ||
        cmd === "get_songs_by_artist" ||
        cmd === "get_songs_by_album" ||
        cmd === "get_songs_by_genre" ||
        cmd === "get_songs_by_ids" ||
        cmd === "search_songs" ||
        cmd === "get_playlists_by_artist" ||
        cmd === "get_compilations_by_artist"
      ) {
        return [];
      }
      if (cmd === "get_artist_profile") {
        return null;
      }
      if (cmd === "get_playlist_tracks") {
        return [];
      }
      if (cmd === "get_all_app_settings") {
        return {};
      }
      return null;
    }),
  };
});

vi.mock("@tauri-apps/api/event", () => {
  return {
    listen: vi.fn().mockImplementation(async (event, callback) => {
      console.log(`[Tauri Mock Listen] registered for event: ${event}`);
      // Return a mock unlisten function
      return () => {
        console.log(`[Tauri Mock Unlisten] for event: ${event}`);
      };
    }),
  };
});

// Mock Tauri updater plugin — defaults to "no update available"; tests override with
// vi.mocked(check).mockResolvedValueOnce(...) to simulate an available update.
vi.mock("@tauri-apps/plugin-updater", () => {
  return {
    check: vi.fn().mockResolvedValue(null),
  };
});

// Mock Tauri process plugin (used to restart the app after an in-place update install)
vi.mock("@tauri-apps/plugin-process", () => {
  return {
    relaunch: vi.fn().mockResolvedValue(undefined),
    exit: vi.fn().mockResolvedValue(undefined),
  };
});

// Mock Tauri window API (geometry tracking, window controls)
vi.mock("@tauri-apps/api/window", () => {
  const mockWindow = {
    onResized: vi.fn().mockResolvedValue(() => {}),
    onMoved: vi.fn().mockResolvedValue(() => {}),
    onCloseRequested: vi.fn().mockResolvedValue(() => {}),
    onFocusChanged: vi.fn().mockResolvedValue(() => {}),
    onScaleFactorChanged: vi.fn().mockResolvedValue(() => {}),
    onThemeChanged: vi.fn().mockResolvedValue(() => {}),
    innerSize: vi.fn().mockResolvedValue({ width: 1200, height: 800 }),
    outerSize: vi.fn().mockResolvedValue({ width: 1200, height: 800 }),
    innerPosition: vi.fn().mockResolvedValue({ x: 100, y: 100 }),
    outerPosition: vi.fn().mockResolvedValue({ x: 100, y: 100 }),
    isMaximized: vi.fn().mockResolvedValue(false),
    isMinimized: vi.fn().mockResolvedValue(false),
    isFullscreen: vi.fn().mockResolvedValue(false),
    isVisible: vi.fn().mockResolvedValue(true),
    isFocused: vi.fn().mockResolvedValue(true),
    setSize: vi.fn().mockResolvedValue(undefined),
    setPosition: vi.fn().mockResolvedValue(undefined),
    maximize: vi.fn().mockResolvedValue(undefined),
    unmaximize: vi.fn().mockResolvedValue(undefined),
    minimize: vi.fn().mockResolvedValue(undefined),
    unminimize: vi.fn().mockResolvedValue(undefined),
    setFullscreen: vi.fn().mockResolvedValue(undefined),
    show: vi.fn().mockResolvedValue(undefined),
    hide: vi.fn().mockResolvedValue(undefined),
    close: vi.fn().mockResolvedValue(undefined),
    setFocus: vi.fn().mockResolvedValue(undefined),
  };
  return {
    getCurrentWindow: vi.fn(() => mockWindow),
    Window: vi.fn(() => mockWindow),
  };
});

// jsdom doesn't implement canvas rendering, so components that draw to a
// <canvas> (e.g. WaveformSeekBar) spam "not implemented" console noise on
// every render. Tests here only mount components, they don't assert on
// pixels, so a minimal no-op 2D context is enough to keep draw() calls from
// throwing/warning without pulling in the native `canvas` package.
if (typeof HTMLCanvasElement !== "undefined") {
  HTMLCanvasElement.prototype.getContext = vi.fn(() => ({
    save: vi.fn(),
    restore: vi.fn(),
    scale: vi.fn(),
    clearRect: vi.fn(),
    beginPath: vi.fn(),
    fill: vi.fn(),
    fillRect: vi.fn(),
    roundRect: vi.fn(),
    createLinearGradient: vi.fn(() => ({ addColorStop: vi.fn() })),
    fillStyle: "",
    globalAlpha: 1,
  })) as unknown as typeof HTMLCanvasElement.prototype.getContext;
}

// jsdom doesn't implement the Web Animations API, so components that call
// element.animate() for Svelte transitions/animations throw. Tests here
// don't assert on animation timing, so a minimal no-op stub is enough.
if (typeof Element !== "undefined" && !Element.prototype.animate) {
  Element.prototype.animate = vi.fn().mockReturnValue({
    finished: Promise.resolve(),
    cancel: () => {},
    addEventListener: () => {},
    removeEventListener: () => {},
  }) as any;
}
