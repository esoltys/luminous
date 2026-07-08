import "@testing-library/jest-dom";
import { vi } from "vitest";

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
