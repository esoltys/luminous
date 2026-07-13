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

// Mock Tauri core API
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
      return null;
    }),
  };
});

// Mock Tauri event API
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
