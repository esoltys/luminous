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
      // The backend bootstraps the built-in Queue playlist at startup, so a
      // playlist listing always contains it — model that guarantee here.
      if (cmd === "get_playlists") {
        return [
          { id: 1, name: "Queue", dynamic_enabled: false, created: 0, updated: 0, track_count: 0, is_queue: true },
        ];
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
