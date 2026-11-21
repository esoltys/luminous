import { describe, it, expect, beforeEach, vi } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

vi.mock("@tauri-apps/api/window", () => ({
  getCurrentWindow: vi.fn(() => ({
    onResized: vi.fn(() => Promise.resolve(() => {})),
    onMoved: vi.fn(() => Promise.resolve(() => {})),
  })),
}));

import { collectionStore } from "./collection.svelte";
import { windowLayoutStore } from "./windowLayout.svelte";

describe("CollectionStore - sidebar/right-panel layout and responsive breakpoints", () => {
  beforeEach(() => {
    vi.clearAllMocks();

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

  describe("info sidebar pinning", () => {
    beforeEach(() => {
      localStorage.clear();
      windowLayoutStore.setRightPanelPinned(true);
    });

    it("toggleRightPanel hides a docked sidebar to the rail, without an overlay", () => {
      windowLayoutStore.toggleRightPanel();
      expect(windowLayoutStore.rightPanelPinned).toBe(false);
      expect(windowLayoutStore.rightPanelPeeking).toBe(false);
      expect(windowLayoutStore.rightPanelVisible).toBe(false);
    });

    it("toggleRightPanel docks the sidebar from the rail or an overlay", () => {
      windowLayoutStore.setRightPanelPinned(false);
      windowLayoutStore.setRightPanelPeeking(false);
      windowLayoutStore.toggleRightPanel();
      expect(windowLayoutStore.rightPanelPinned).toBe(true);

      windowLayoutStore.setRightPanelPinned(false);
      expect(windowLayoutStore.rightPanelPeeking).toBe(true);
      windowLayoutStore.toggleRightPanel();
      expect(windowLayoutStore.rightPanelPinned).toBe(true);
      expect(windowLayoutStore.rightPanelPeeking).toBe(false);
    });

    it("unpinning keeps the sidebar up as an overlay", () => {
      windowLayoutStore.setRightPanelPinned(false);
      expect(windowLayoutStore.rightPanelPeeking).toBe(true);
      expect(windowLayoutStore.rightPanelVisible).toBe(true);
    });

    it("treats the legacy rightPanelOpen name as docked", () => {
      windowLayoutStore.rightPanelOpen = false;
      expect(windowLayoutStore.rightPanelPinned).toBe(false);
      expect(windowLayoutStore.rightPanelPeeking).toBe(false);
      windowLayoutStore.rightPanelOpen = true;
      expect(windowLayoutStore.rightPanelPinned).toBe(true);
    });

    it("persists the pinned state", () => {
      windowLayoutStore.setRightPanelPinned(false);
      expect(localStorage.getItem("layout_rightPanelPinned")).toBe("false");
      windowLayoutStore.setRightPanelPinned(true);
      expect(localStorage.getItem("layout_rightPanelPinned")).toBe("true");
    });

    it("reports hasRightNotch only when unpinned and above the right-panel breakpoint", () => {
      windowLayoutStore.viewportWidth = 1400;
      windowLayoutStore.setRightPanelPinned(true);
      expect(windowLayoutStore.hasRightNotch).toBe(false);

      windowLayoutStore.setRightPanelPinned(false);
      expect(windowLayoutStore.hasRightNotch).toBe(true);

      windowLayoutStore.viewportWidth = 500;
      expect(windowLayoutStore.hasRightNotch).toBe(false);

      windowLayoutStore.viewportWidth = 1400;
      windowLayoutStore.setRightPanelPinned(true);
    });
  });

  it("toggles and persists layout states (sidebar, right panel, immersive mode)", () => {
    windowLayoutStore.sidebarOpen = false;
    windowLayoutStore.setSidebarWidth(300);
    windowLayoutStore.toggleSidebarCompact();
    expect(windowLayoutStore.sidebarOpen).toBe(true);
    expect(windowLayoutStore.sidebarWidth).toBeLessThan(300);
    windowLayoutStore.toggleSidebarCompact();
    expect(windowLayoutStore.sidebarWidth).toBe(300);

    const initialRight = windowLayoutStore.rightPanelVisible;
    windowLayoutStore.toggleRightPanel();
    expect(windowLayoutStore.rightPanelVisible).toBe(!initialRight);

    windowLayoutStore.setRightPanelWidth(320);
    expect(windowLayoutStore.rightPanelWidth).toBe(320);

    windowLayoutStore.immersiveMode = true;
    windowLayoutStore.exitImmersiveMode();
    expect(windowLayoutStore.immersiveMode).toBe(false);
  });

  it("switches between cover and visualizer immersive views in place and toggles off when clicking the active view (#2236)", () => {
    localStorage.clear();
    windowLayoutStore.immersiveMode = false;
    windowLayoutStore.setImmersiveView("cover");

    // 1. Entering via visualizer from normal view opens immersive visualizer
    windowLayoutStore.toggleImmersiveMode("visualizer");
    expect(windowLayoutStore.immersiveMode).toBe(true);
    expect(windowLayoutStore.immersiveView).toBe("visualizer");
    expect(localStorage.getItem("layout_immersiveMode")).toBe("true");
    expect(localStorage.getItem("layout_immersiveView")).toBe("visualizer");

    // 2. Clicking cover while in immersive visualizer switches view in place without exiting immersive mode
    windowLayoutStore.toggleImmersiveMode("cover");
    expect(windowLayoutStore.immersiveMode).toBe(true);
    expect(windowLayoutStore.immersiveView).toBe("cover");
    expect(localStorage.getItem("layout_immersiveView")).toBe("cover");

    // 3. Clicking visualizer while in immersive cover switches back to visualizer in place
    windowLayoutStore.toggleImmersiveMode("visualizer");
    expect(windowLayoutStore.immersiveMode).toBe(true);
    expect(windowLayoutStore.immersiveView).toBe("visualizer");

    // 4. Clicking visualizer again while already in immersive visualizer exits immersive mode
    windowLayoutStore.toggleImmersiveMode("visualizer");
    expect(windowLayoutStore.immersiveMode).toBe(false);
    expect(localStorage.getItem("layout_immersiveMode")).toBe("false");

    // 5. Argument-less toggle (e.g. Ctrl+2 hotkey) flips immersiveMode while preserving the last immersiveView
    windowLayoutStore.toggleImmersiveMode();
    expect(windowLayoutStore.immersiveMode).toBe(true);
    expect(windowLayoutStore.immersiveView).toBe("visualizer");
    windowLayoutStore.toggleImmersiveMode();
    expect(windowLayoutStore.immersiveMode).toBe(false);
  });

  it("drops the playbar out of view after 2.5s in immersive mode (cover or visualizer) and pops it back up on dock-zone activity (#2236)", () => {
    vi.useFakeTimers();
    try {
      windowLayoutStore.viewportWidth = 1280;
      windowLayoutStore.exitImmersiveMode();
      expect(windowLayoutStore.isImmersiveDockHidden).toBe(false);

      // 1. Enter immersive visualizer view: dock starts visible, then drops out after 2.5s
      windowLayoutStore.toggleImmersiveMode("visualizer");
      expect(windowLayoutStore.isImmersiveDockHidden).toBe(false);
      vi.advanceTimersByTime(2499);
      expect(windowLayoutStore.isImmersiveDockHidden).toBe(false);
      vi.advanceTimersByTime(1);
      expect(windowLayoutStore.isImmersiveDockHidden).toBe(true);

      // 2. While the cursor rests inside the dock area, the playbar stays visible indefinitely;
      // leaving the dock area restarts the 2.5s countdown.
      windowLayoutStore.setImmersiveDockHovered(true);
      expect(windowLayoutStore.isImmersiveDockHidden).toBe(false);
      vi.advanceTimersByTime(10000);
      expect(windowLayoutStore.isImmersiveDockHidden).toBe(false);
      windowLayoutStore.setImmersiveDockHovered(false);
      vi.advanceTimersByTime(2499);
      expect(windowLayoutStore.isImmersiveDockHidden).toBe(false);
      vi.advanceTimersByTime(1);
      expect(windowLayoutStore.isImmersiveDockHidden).toBe(true);

      // 3. Holding the dock (e.g. pointer drag on seek/volume) reveals it and prevents hiding until released
      windowLayoutStore.setImmersiveDockHeld(true);
      expect(windowLayoutStore.isImmersiveDockHidden).toBe(false);
      vi.advanceTimersByTime(5000);
      expect(windowLayoutStore.isImmersiveDockHidden).toBe(false);
      windowLayoutStore.setImmersiveDockHeld(false);
      vi.advanceTimersByTime(2500);
      expect(windowLayoutStore.isImmersiveDockHidden).toBe(true);

      // 4. Switching to cover view reveals the dock, then drops it out after 2.5s; exiting immersive restores it
      windowLayoutStore.toggleImmersiveMode("cover");
      expect(windowLayoutStore.immersiveView).toBe("cover");
      expect(windowLayoutStore.isImmersiveDockHidden).toBe(false);
      vi.advanceTimersByTime(2500);
      expect(windowLayoutStore.isImmersiveDockHidden).toBe(true);

      windowLayoutStore.exitImmersiveMode();
      expect(windowLayoutStore.isImmersiveDockHidden).toBe(false);
    } finally {
      vi.useRealTimers();
    }
  });

  it("derives responsive breakpoint flags from viewportWidth/viewportHeight without touching stored preferences", () => {
    windowLayoutStore.sidebarOpen = true;
    windowLayoutStore.setSidebarWidth(300);
    windowLayoutStore.rightPanelOpen = true;
    windowLayoutStore.immersiveMode = false;

    // Large: nothing auto-collapsed.
    windowLayoutStore.viewportWidth = 1200;
    windowLayoutStore.viewportHeight = 800;
    expect(windowLayoutStore.isSidebarAutoCollapsed).toBe(false);
    expect(windowLayoutStore.isRightPanelAutoHidden).toBe(false);
    expect(windowLayoutStore.isImmersiveForced).toBe(false);
    expect(windowLayoutStore.effectiveImmersiveMode).toBe(false);
    expect(windowLayoutStore.isPlaybarOnlyMode).toBe(false);

    // Medium (between 768px and 1024px): sidebar auto-collapses, but right panel and playbar controls stay visible.
    windowLayoutStore.viewportWidth = 800;
    expect(windowLayoutStore.isSidebarAutoCollapsed).toBe(true);
    expect(windowLayoutStore.isRightPanelAutoHidden).toBe(false);
    expect(windowLayoutStore.isImmersiveForced).toBe(false);
    expect(windowLayoutStore.sidebarWidth).toBe(300);
    expect(windowLayoutStore.rightPanelOpen).toBe(true);

    // Below 768px: right panel auto-hides. 700px and 640px are still Full tier for PlayerBar.
    windowLayoutStore.viewportWidth = 700;
    expect(windowLayoutStore.isSidebarAutoCollapsed).toBe(true);
    expect(windowLayoutStore.isRightPanelAutoHidden).toBe(true);
    expect(windowLayoutStore.isPlayerBarCompact).toBe(false);
    expect(windowLayoutStore.isImmersiveForced).toBe(false);

    windowLayoutStore.viewportWidth = 640;
    expect(windowLayoutStore.isPlayerBarCompact).toBe(false);
    expect(windowLayoutStore.isImmersiveForced).toBe(false);

    // Below 640px: PlayerBar transitions to Compact tier at the exact same breakpoint where Immersive Mode force-engages.
    windowLayoutStore.viewportWidth = 639;
    expect(windowLayoutStore.isPlayerBarCompact).toBe(true);
    expect(windowLayoutStore.isImmersiveForced).toBe(true);
    expect(windowLayoutStore.effectiveImmersiveMode).toBe(true);

    // Small: immersive force-engages via the derived flag only.
    windowLayoutStore.viewportWidth = 500;
    expect(windowLayoutStore.isPlayerBarCompact).toBe(true);
    expect(windowLayoutStore.isImmersiveForced).toBe(true);
    expect(windowLayoutStore.effectiveImmersiveMode).toBe(true);
    expect(windowLayoutStore.immersiveMode).toBe(false);

    // Widening back out restores the large-tier flags without any stored
    // preference having been mutated along the way.
    windowLayoutStore.viewportWidth = 1200;
    expect(windowLayoutStore.isSidebarAutoCollapsed).toBe(false);
    expect(windowLayoutStore.isRightPanelAutoHidden).toBe(false);
    expect(windowLayoutStore.effectiveImmersiveMode).toBe(false);

    // Height is independent of width: < 160px collapses to playbar-only mode.
    windowLayoutStore.viewportHeight = 140;
    expect(windowLayoutStore.isPlaybarOnlyMode).toBe(true);
    windowLayoutStore.viewportHeight = 160;
    expect(windowLayoutStore.isPlaybarOnlyMode).toBe(false);
    windowLayoutStore.viewportHeight = 800;
    expect(windowLayoutStore.isPlaybarOnlyMode).toBe(false);
  });

  it("toggles sidebar compact state between 64px and expanded width", () => {
    windowLayoutStore.sidebarOpen = true;
    windowLayoutStore.setSidebarWidth(256);
    expect(windowLayoutStore.sidebarWidth).toBe(256);

    // Toggle compact -> collapses to 64px
    windowLayoutStore.toggleSidebarCompact();
    expect(windowLayoutStore.sidebarWidth).toBe(64);

    // Toggle compact again -> expands to 256px
    windowLayoutStore.toggleSidebarCompact();
    expect(windowLayoutStore.sidebarWidth).toBe(256);

    // If sidebar was hidden, toggleSidebarCompact opens and expands it
    windowLayoutStore.sidebarOpen = false;
    windowLayoutStore.sidebarWidth = 64;
    windowLayoutStore.toggleSidebarCompact();
    expect(windowLayoutStore.sidebarOpen).toBe(true);
    expect(windowLayoutStore.sidebarWidth).toBe(256);
  });
});
