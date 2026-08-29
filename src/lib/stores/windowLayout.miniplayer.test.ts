import { describe, it, expect, beforeEach, vi } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

const windowGeometryCallbacks = vi.hoisted(() => [] as Array<() => void>);

vi.mock("@tauri-apps/api/window", () => ({
  getCurrentWindow: vi.fn(() => ({
    onResized: vi.fn((cb: () => void) => {
      windowGeometryCallbacks.push(cb);
      return Promise.resolve(() => {});
    }),
    onMoved: vi.fn((cb: () => void) => {
      windowGeometryCallbacks.push(cb);
      return Promise.resolve(() => {});
    }),
  })),
}));

import { collectionStore } from "./collection.svelte";
import { windowLayoutStore } from "./windowLayout.svelte";

describe("CollectionStore - miniplayer geometry and window-geometry IPC races", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    // windowGeometryCallbacks is populated once, when the collectionStore
    // singleton's constructor calls initWindowGeometryTracking() at module
    // import time — it must not be cleared per-test, there's no later call
    // that would repopulate it.

    vi.mocked(listen).mockImplementation(async () => () => {});

    vi.mocked(invoke).mockImplementation(async (cmd: string) => {
      switch (cmd) {
        case "geometry_capture_supported":
          return true;
        case "get_all_app_settings":
          return {};
        default:
          return null;
      }
    });
  });

  it("sends remembered size/position as the toggle target when geometry capture is supported, ignoring any command return value", async () => {
    windowLayoutStore.setMiniplayerGeometry(310, 370, 20, 30);
    windowLayoutStore.setSavedWindowGeometry(1400, 900, 50, 60);

    vi.mocked(invoke).mockImplementation(async (cmd: string) => {
      if (cmd === "geometry_capture_supported") return true;
      return { width: 9999, height: 9999 };
    });

    await windowLayoutStore.enterMiniplayerMode();
    expect(invoke).toHaveBeenCalledWith("enter_miniplayer_mode", { width: 310, height: 370, x: 20, y: 30 });
    expect(windowLayoutStore.isMiniplayer).toBe(true);
    expect(windowLayoutStore.miniplayerWidth).toBe(310);

    await windowLayoutStore.exitMiniplayerMode();
    expect(invoke).toHaveBeenCalledWith("exit_miniplayer_mode", { width: 1400, height: 900, x: 50, y: 60 });
    expect(windowLayoutStore.isMiniplayer).toBe(false);
    expect(windowLayoutStore.savedWindowWidth).toBe(1400);
  });

  it("sends no geometry at all when geometry capture is unsupported, letting the backend use its fixed defaults", async () => {
    windowLayoutStore.setMiniplayerGeometry(310, 370, 20, 30);

    vi.mocked(invoke).mockImplementation(async (cmd: string) => {
      if (cmd === "geometry_capture_supported") return false;
      return null;
    });

    await windowLayoutStore.enterMiniplayerMode();
    expect(invoke).toHaveBeenCalledWith("enter_miniplayer_mode", {});
    expect(windowLayoutStore.isMiniplayer).toBe(true);

    await windowLayoutStore.exitMiniplayerMode();
    expect(invoke).toHaveBeenCalledWith("exit_miniplayer_mode", {});
    expect(windowLayoutStore.isMiniplayer).toBe(false);
  });

  it("captures a settled resize/move into the full player's geometry", async () => {
    vi.useFakeTimers();
    try {
      expect(windowLayoutStore.isMiniplayer).toBe(false);
      vi.mocked(invoke).mockImplementation(async (cmd: string) => {
        if (cmd === "get_window_geometry") return { width: 1500, height: 950, x: 10, y: 20 };
        return null;
      });

      windowGeometryCallbacks.forEach((cb) => cb());
      await vi.advanceTimersByTimeAsync(400);

      expect(windowLayoutStore.savedWindowWidth).toBe(1500);
      expect(windowLayoutStore.savedWindowHeight).toBe(950);
      expect(windowLayoutStore.savedWindowX).toBe(10);
      expect(windowLayoutStore.savedWindowY).toBe(20);
    } finally {
      vi.useRealTimers();
    }
  });

  it("skips geometry capture while a mode toggle is in flight, then captures once settled", async () => {
    // Regression guard for the growth/shrink-per-toggle bug: a resize/move
    // event firing mid-transition (which includes the toggle's own
    // programmatic resize/reposition) must not be captured as genuine.
    vi.useFakeTimers();
    try {
      let resolveEnter: (value: unknown) => void = () => {};
      const pendingEnter = new Promise((resolve) => {
        resolveEnter = resolve;
      });

      vi.mocked(invoke).mockImplementation(async (cmd: string) => {
        if (cmd === "geometry_capture_supported") return true;
        if (cmd === "enter_miniplayer_mode") return pendingEnter;
        if (cmd === "get_window_geometry") return { width: 1500, height: 950, x: null, y: null };
        return null;
      });

      windowLayoutStore.setMiniplayerGeometry(300, 360);
      const enterPromise = windowLayoutStore.enterMiniplayerMode();

      windowGeometryCallbacks.forEach((cb) => cb());
      await vi.advanceTimersByTimeAsync(400);
      expect(windowLayoutStore.miniplayerWidth).toBe(300);

      resolveEnter(undefined);
      await enterPromise;

      windowGeometryCallbacks.forEach((cb) => cb());
      await vi.advanceTimersByTimeAsync(400);
      expect(windowLayoutStore.miniplayerWidth).toBe(1500);

      await windowLayoutStore.exitMiniplayerMode();
      expect(windowLayoutStore.isMiniplayer).toBe(false);
    } finally {
      vi.useRealTimers();
    }
  });

  it("blocks exitMiniplayerMode while a prior enterMiniplayerMode call is still in flight", async () => {
    // Rapid back-and-forth toggling (e.g. holding Ctrl+M) could otherwise let
    // exit start reading/resizing the window before enter's own resize IPC
    // call has resolved, corrupting the captured "current size" baseline and
    // compounding into runaway growth on repeated toggles.
    let resolveEnter: (value: unknown) => void = () => {};
    const pendingEnter = new Promise((resolve) => {
      resolveEnter = resolve;
    });
    let exitCallCount = 0;

    vi.mocked(invoke).mockImplementation(async (cmd: string) => {
      if (cmd === "enter_miniplayer_mode") return pendingEnter;
      if (cmd === "exit_miniplayer_mode") {
        exitCallCount++;
        return null;
      }
      return null;
    });

    const enterCall = windowLayoutStore.enterMiniplayerMode();
    expect(windowLayoutStore.isMiniplayer).toBe(true);

    await windowLayoutStore.exitMiniplayerMode();
    expect(exitCallCount).toBe(0);
    expect(windowLayoutStore.isMiniplayer).toBe(true);

    resolveEnter(undefined);
    await enterCall;

    await windowLayoutStore.exitMiniplayerMode();
    expect(exitCallCount).toBe(1);
    expect(windowLayoutStore.isMiniplayer).toBe(false);
  });

  it("restores miniplayer mode synchronously from localStorage on startup and syncs backend IPC", async () => {
    localStorage.setItem("layout_isMiniplayer", "true");
    vi.mocked(invoke).mockResolvedValue(null);

    // Re-initialize collection store
    await windowLayoutStore.enterMiniplayerMode(true);
    expect(windowLayoutStore.isMiniplayer).toBe(true);
    expect(invoke).toHaveBeenCalledWith("enter_miniplayer_mode", expect.any(Object));
  });
});
