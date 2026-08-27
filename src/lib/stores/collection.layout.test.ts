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
    const initialSidebar = collectionStore.sidebarOpen;
    collectionStore.toggleSidebar();
    expect(collectionStore.sidebarOpen).toBe(!initialSidebar);

    collectionStore.setSidebarWidth(300);
    expect(collectionStore.sidebarWidth).toBe(300);

    const initialRight = collectionStore.rightPanelOpen;
    collectionStore.toggleRightPanel();
    expect(collectionStore.rightPanelOpen).toBe(!initialRight);

    collectionStore.setRightPanelWidth(320);
    expect(collectionStore.rightPanelWidth).toBe(320);

    collectionStore.immersiveMode = true;
    collectionStore.exitImmersiveMode();
    expect(collectionStore.immersiveMode).toBe(false);
  });

  it("derives responsive breakpoint flags from viewportWidth/viewportHeight without touching stored preferences", () => {
    collectionStore.sidebarOpen = true;
    collectionStore.setSidebarWidth(300);
    collectionStore.rightPanelOpen = true;
    collectionStore.immersiveMode = false;

    // Large: nothing auto-collapsed.
    collectionStore.viewportWidth = 1200;
    collectionStore.viewportHeight = 800;
    expect(collectionStore.isSidebarAutoCollapsed).toBe(false);
    expect(collectionStore.isRightPanelAutoHidden).toBe(false);
    expect(collectionStore.isImmersiveForced).toBe(false);
    expect(collectionStore.effectiveImmersiveMode).toBe(false);
    expect(collectionStore.isPlaybarOnlyMode).toBe(false);

    // Medium: sidebar/right-panel auto-collapse, real preferences untouched.
    collectionStore.viewportWidth = 800;
    expect(collectionStore.isSidebarAutoCollapsed).toBe(true);
    expect(collectionStore.isRightPanelAutoHidden).toBe(true);
    expect(collectionStore.isImmersiveForced).toBe(false);
    expect(collectionStore.sidebarWidth).toBe(300);
    expect(collectionStore.rightPanelOpen).toBe(true);

    // Small: immersive force-engages via the derived flag only.
    collectionStore.viewportWidth = 500;
    expect(collectionStore.isImmersiveForced).toBe(true);
    expect(collectionStore.effectiveImmersiveMode).toBe(true);
    expect(collectionStore.immersiveMode).toBe(false);

    // Widening back out restores the large-tier flags without any stored
    // preference having been mutated along the way.
    collectionStore.viewportWidth = 1200;
    expect(collectionStore.isSidebarAutoCollapsed).toBe(false);
    expect(collectionStore.isRightPanelAutoHidden).toBe(false);
    expect(collectionStore.effectiveImmersiveMode).toBe(false);

    // Height is independent of width.
    collectionStore.viewportHeight = 250;
    expect(collectionStore.isPlaybarOnlyMode).toBe(true);
    collectionStore.viewportHeight = 800;
    expect(collectionStore.isPlaybarOnlyMode).toBe(false);
  });

  it("toggles sidebar compact state between 64px and expanded width", () => {
    collectionStore.sidebarOpen = true;
    collectionStore.setSidebarWidth(256);
    expect(collectionStore.sidebarWidth).toBe(256);

    // Toggle compact -> collapses to 64px
    collectionStore.toggleSidebarCompact();
    expect(collectionStore.sidebarWidth).toBe(64);

    // Toggle compact again -> expands to 256px
    collectionStore.toggleSidebarCompact();
    expect(collectionStore.sidebarWidth).toBe(256);

    // If sidebar was hidden, toggleSidebarCompact opens and expands it
    collectionStore.sidebarOpen = false;
    collectionStore.sidebarWidth = 64;
    collectionStore.toggleSidebarCompact();
    expect(collectionStore.sidebarOpen).toBe(true);
    expect(collectionStore.sidebarWidth).toBe(256);
  });
});
