import "@testing-library/jest-dom";
import { describe, it, expect, vi, beforeEach } from "vitest";

if (typeof Element !== "undefined") {
  Element.prototype.animate = vi.fn().mockReturnValue({
    finished: Promise.resolve(),
    cancel: () => {},
    addEventListener: () => {},
    removeEventListener: () => {},
  }) as any;
}

import { render, fireEvent } from "@testing-library/svelte";
import TopNavigation from "./TopNavigation.svelte";
import { collectionStore } from "../stores/collection.svelte";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn().mockResolvedValue([]),
}));

vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn().mockResolvedValue(() => {}),
}));

describe("TopNavigation.svelte", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    collectionStore.immersiveMode = false;
    collectionStore.sidebarOpen = true;
    collectionStore.rightPanelOpen = false;
    collectionStore.viewportWidth = 1280;
  });

  it("toggles sidebar compact state when clicking the hamburger menu button", async () => {
    collectionStore.sidebarOpen = true;
    collectionStore.setSidebarWidth(256);
    const { getByTitle } = render(TopNavigation);
    const hamburgerBtn = getByTitle("Toggle sidebar (compact / expanded)");

    await fireEvent.click(hamburgerBtn);
    expect(collectionStore.sidebarWidth).toBe(64);

    await fireEvent.click(hamburgerBtn);
    expect(collectionStore.sidebarWidth).toBe(256);
  });

  it("hides the right-panel toggle at the same breakpoint that auto-hides the panel itself", () => {
    collectionStore.viewportWidth = 1280;
    expect(collectionStore.isRightPanelAutoHidden).toBe(false);
    const { queryByTitle } = render(TopNavigation);
    expect(queryByTitle("Show Info Panel")).not.toBeNull();

    collectionStore.viewportWidth = 800;
    expect(collectionStore.isRightPanelAutoHidden).toBe(true);
    const { queryByTitle: queryByTitleNarrow } = render(TopNavigation);
    expect(queryByTitleNarrow("Show Info Panel")).toBeNull();
  });

  it("hides the sidebar compact/expand toggle at the same breakpoint that auto-collapses it", () => {
    collectionStore.viewportWidth = 1280;
    expect(collectionStore.isSidebarAutoCollapsed).toBe(false);
    const { queryByTitle } = render(TopNavigation);
    expect(queryByTitle("Toggle sidebar (compact / expanded)")).not.toBeNull();

    collectionStore.viewportWidth = 800;
    expect(collectionStore.isSidebarAutoCollapsed).toBe(true);
    const { queryByTitle: queryByTitleNarrow } = render(TopNavigation);
    expect(queryByTitleNarrow("Toggle sidebar (compact / expanded)")).toBeNull();
  });
});
