import { describe, it, expect, beforeEach, afterEach, vi } from "vitest";
import { flushSync } from "svelte";

vi.mock("@tauri-apps/api/window", () => ({
  getCurrentWindow: vi.fn(() => ({
    onResized: vi.fn(() => Promise.resolve(() => {})),
    onMoved: vi.fn(() => Promise.resolve(() => {})),
  })),
}));

import { windowLayoutStore } from "./windowLayout.svelte";
import { immersiveDock } from "./immersiveDock.svelte";
import { PLAYER_DOCK_CONTENT_RESERVE_PX } from "../constants";

function createPointerMoveEvent(clientY: number): Event {
  return Object.assign(new Event("pointermove"), { clientY });
}

describe("ImmersiveDockStore", () => {
  beforeEach(() => {
    vi.useFakeTimers();
    windowLayoutStore.viewportWidth = 1280;
    windowLayoutStore.viewportHeight = 800;
    windowLayoutStore.exitImmersiveMode();
    immersiveDock.reset();
    flushSync();
  });

  afterEach(() => {
    immersiveDock.reset();
    windowLayoutStore.exitImmersiveMode();
    flushSync();
    vi.useRealTimers();
  });

  it("drops the playbar out of view after 2.5s in immersive mode (cover or visualizer) and pops it back up on dock-zone activity (#2236)", () => {
    expect(immersiveDock.isHidden).toBe(false);

    // 1. Enter immersive visualizer view: dock starts visible, then drops out after 2.5s
    windowLayoutStore.toggleImmersiveMode("visualizer");
    flushSync();
    expect(immersiveDock.isHidden).toBe(false);
    vi.advanceTimersByTime(2499);
    expect(immersiveDock.isHidden).toBe(false);
    vi.advanceTimersByTime(1);
    expect(immersiveDock.isHidden).toBe(true);

    // 2. While the cursor rests inside the dock area, the playbar stays visible indefinitely;
    // leaving the dock area restarts the 2.5s countdown.
    immersiveDock.setHovered(true);
    expect(immersiveDock.isHidden).toBe(false);
    vi.advanceTimersByTime(10000);
    expect(immersiveDock.isHidden).toBe(false);
    immersiveDock.setHovered(false);
    vi.advanceTimersByTime(2499);
    expect(immersiveDock.isHidden).toBe(false);
    vi.advanceTimersByTime(1);
    expect(immersiveDock.isHidden).toBe(true);

    // 3. Holding the dock (e.g. pointer drag on seek/volume) reveals it and prevents hiding until released
    immersiveDock.setHeld(true);
    expect(immersiveDock.isHidden).toBe(false);
    vi.advanceTimersByTime(5000);
    expect(immersiveDock.isHidden).toBe(false);
    immersiveDock.setHeld(false);
    vi.advanceTimersByTime(2500);
    expect(immersiveDock.isHidden).toBe(true);

    // 4. Switching to cover view reveals the dock, then drops it out after 2.5s; exiting immersive restores it
    windowLayoutStore.toggleImmersiveMode("cover");
    flushSync();
    expect(windowLayoutStore.immersiveView).toBe("cover");
    expect(immersiveDock.isHidden).toBe(false);
    vi.advanceTimersByTime(2500);
    expect(immersiveDock.isHidden).toBe(true);

    windowLayoutStore.exitImmersiveMode();
    flushSync();
    expect(immersiveDock.isHidden).toBe(false);
  });

  it("wires dock pointer, focus, and window bottom-zone listeners via attach() and cleans up on destroy", () => {
    const dockEl = document.createElement("div");
    const action = immersiveDock.attach(dockEl);

    try {
      windowLayoutStore.toggleImmersiveMode("cover");
      flushSync();
      vi.advanceTimersByTime(2500);
      expect(immersiveDock.isHidden).toBe(true);

      // Window pointermove into the bottom reserve zone reveals and holds hover
      Object.defineProperty(window, "innerHeight", { value: 800, configurable: true });
      window.dispatchEvent(createPointerMoveEvent(800 - PLAYER_DOCK_CONTENT_RESERVE_PX));
      expect(immersiveDock.isHidden).toBe(false);
      vi.advanceTimersByTime(5000);
      expect(immersiveDock.isHidden).toBe(false);

      // Moving pointer above the dock zone clears hover and starts the 2.5s countdown
      window.dispatchEvent(createPointerMoveEvent(800 - PLAYER_DOCK_CONTENT_RESERVE_PX - 1));
      vi.advanceTimersByTime(2500);
      expect(immersiveDock.isHidden).toBe(true);

      // Element pointerenter / pointerleave
      dockEl.dispatchEvent(new Event("pointerenter"));
      expect(immersiveDock.isHidden).toBe(false);
      dockEl.dispatchEvent(new Event("pointerleave"));
      vi.advanceTimersByTime(2500);
      expect(immersiveDock.isHidden).toBe(true);

      // focusin reveals and restarts countdown
      dockEl.dispatchEvent(new Event("focusin"));
      expect(immersiveDock.isHidden).toBe(false);
      vi.advanceTimersByTime(2500);
      expect(immersiveDock.isHidden).toBe(true);

      // pointerdown holds until window pointerup
      dockEl.dispatchEvent(new Event("pointerdown"));
      expect(immersiveDock.isHidden).toBe(false);
      vi.advanceTimersByTime(5000);
      expect(immersiveDock.isHidden).toBe(false);
      window.dispatchEvent(new Event("pointerup"));
      vi.advanceTimersByTime(2500);
      expect(immersiveDock.isHidden).toBe(true);

      // Unmounting while held releases the hold and removes listeners
      dockEl.dispatchEvent(new Event("pointerdown"));
      expect(immersiveDock.isHidden).toBe(false);
      action.destroy();
      vi.advanceTimersByTime(2500);
      expect(immersiveDock.isHidden).toBe(true);

      // Listeners no longer fire after destroy
      dockEl.dispatchEvent(new Event("pointerenter"));
      expect(immersiveDock.isHidden).toBe(true);
      window.dispatchEvent(createPointerMoveEvent(800));
      expect(immersiveDock.isHidden).toBe(true);
    } finally {
      action.destroy();
    }
  });
});
