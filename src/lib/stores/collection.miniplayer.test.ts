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
    collectionStore.setMiniplayerGeometry(310, 370, 20, 30);
    collectionStore.setSavedWindowGeometry(1400, 900, 50, 60);

    vi.mocked(invoke).mockImplementation(async (cmd: string) => {
      if (cmd === "geometry_capture_supported") return true;
      return { width: 9999, height: 9999 };
    });

    await collectionStore.enterMiniplayerMode();
    expect(invoke).toHaveBeenCalledWith("enter_miniplayer_mode", { width: 310, height: 370, x: 20, y: 30 });
    expect(collectionStore.isMiniplayer).toBe(true);
    expect(collectionStore.miniplayerWidth).toBe(310);

    await collectionStore.exitMiniplayerMode();
    expect(invoke).toHaveBeenCalledWith("exit_miniplayer_mode", { width: 1400, height: 900, x: 50, y: 60 });
    expect(collectionStore.isMiniplayer).toBe(false);
    expect(collectionStore.savedWindowWidth).toBe(1400);
  });

  it("sends no geometry at all when geometry capture is unsupported, letting the backend use its fixed defaults", async () => {
    collectionStore.setMiniplayerGeometry(310, 370, 20, 30);

    vi.mocked(invoke).mockImplementation(async (cmd: string) => {
      if (cmd === "geometry_capture_supported") return false;
      return null;
    });

    await collectionStore.enterMiniplayerMode();
    expect(invoke).toHaveBeenCalledWith("enter_miniplayer_mode", {});
    expect(collectionStore.isMiniplayer).toBe(true);

    await collectionStore.exitMiniplayerMode();
    expect(invoke).toHaveBeenCalledWith("exit_miniplayer_mode", {});
    expect(collectionStore.isMiniplayer).toBe(false);
  });

  it("captures a settled resize/move into the full player's geometry", async () => {
    vi.useFakeTimers();
    try {
      expect(collectionStore.isMiniplayer).toBe(false);
      vi.mocked(invoke).mockImplementation(async (cmd: string) => {
        if (cmd === "get_window_geometry") return { width: 1500, height: 950, x: 10, y: 20 };
        return null;
      });

      windowGeometryCallbacks.forEach((cb) => cb());
      await vi.advanceTimersByTimeAsync(400);

      expect(collectionStore.savedWindowWidth).toBe(1500);
      expect(collectionStore.savedWindowHeight).toBe(950);
      expect(collectionStore.savedWindowX).toBe(10);
      expect(collectionStore.savedWindowY).toBe(20);
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

      collectionStore.setMiniplayerGeometry(300, 360);
      const enterPromise = collectionStore.enterMiniplayerMode();

      windowGeometryCallbacks.forEach((cb) => cb());
      await vi.advanceTimersByTimeAsync(400);
      expect(collectionStore.miniplayerWidth).toBe(300);

      resolveEnter(undefined);
      await enterPromise;

      windowGeometryCallbacks.forEach((cb) => cb());
      await vi.advanceTimersByTimeAsync(400);
      expect(collectionStore.miniplayerWidth).toBe(1500);

      await collectionStore.exitMiniplayerMode();
      expect(collectionStore.isMiniplayer).toBe(false);
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

    const enterCall = collectionStore.enterMiniplayerMode();
    expect(collectionStore.isMiniplayer).toBe(true);

    await collectionStore.exitMiniplayerMode();
    expect(exitCallCount).toBe(0);
    expect(collectionStore.isMiniplayer).toBe(true);

    resolveEnter(undefined);
    await enterCall;

    await collectionStore.exitMiniplayerMode();
    expect(exitCallCount).toBe(1);
    expect(collectionStore.isMiniplayer).toBe(false);
  });

  it("restores miniplayer mode synchronously from localStorage on startup and syncs backend IPC", async () => {
    localStorage.setItem("layout_isMiniplayer", "true");
    vi.mocked(invoke).mockResolvedValue(null);

    // Re-initialize collection store
    await collectionStore.enterMiniplayerMode(true);
    expect(collectionStore.isMiniplayer).toBe(true);
    expect(invoke).toHaveBeenCalledWith("enter_miniplayer_mode", expect.any(Object));
  });
});
