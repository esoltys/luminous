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

  it("toggles and persists layout states (sidebar, right panel, immersive mode)", () => {
    const initialSidebar = windowLayoutStore.sidebarOpen;
    windowLayoutStore.toggleSidebar();
    expect(windowLayoutStore.sidebarOpen).toBe(!initialSidebar);

    windowLayoutStore.setSidebarWidth(300);
    expect(windowLayoutStore.sidebarWidth).toBe(300);

    const initialRight = windowLayoutStore.rightPanelOpen;
    windowLayoutStore.toggleRightPanel();
    expect(windowLayoutStore.rightPanelOpen).toBe(!initialRight);

    windowLayoutStore.setRightPanelWidth(320);
    expect(windowLayoutStore.rightPanelWidth).toBe(320);

    windowLayoutStore.immersiveMode = true;
    windowLayoutStore.exitImmersiveMode();
    expect(windowLayoutStore.immersiveMode).toBe(false);
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

    // Medium: sidebar/right-panel auto-collapse, real preferences untouched.
    windowLayoutStore.viewportWidth = 800;
    expect(windowLayoutStore.isSidebarAutoCollapsed).toBe(true);
    expect(windowLayoutStore.isRightPanelAutoHidden).toBe(true);
    expect(windowLayoutStore.isImmersiveForced).toBe(false);
    expect(windowLayoutStore.sidebarWidth).toBe(300);
    expect(windowLayoutStore.rightPanelOpen).toBe(true);

    // Small: immersive force-engages via the derived flag only.
    windowLayoutStore.viewportWidth = 500;
    expect(windowLayoutStore.isImmersiveForced).toBe(true);
    expect(windowLayoutStore.effectiveImmersiveMode).toBe(true);
    expect(windowLayoutStore.immersiveMode).toBe(false);

    // Widening back out restores the large-tier flags without any stored
    // preference having been mutated along the way.
    windowLayoutStore.viewportWidth = 1200;
    expect(windowLayoutStore.isSidebarAutoCollapsed).toBe(false);
    expect(windowLayoutStore.isRightPanelAutoHidden).toBe(false);
    expect(windowLayoutStore.effectiveImmersiveMode).toBe(false);

    // Height is independent of width.
    windowLayoutStore.viewportHeight = 250;
    expect(windowLayoutStore.isPlaybarOnlyMode).toBe(true);
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
