import "@testing-library/jest-dom";
import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, fireEvent } from "@testing-library/svelte";
import TopNavigation from "./TopNavigation.svelte";
import { collectionStore } from "../stores/collection.svelte";
import { windowLayoutStore } from "../stores/windowLayout.svelte";


describe("TopNavigation.svelte", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    windowLayoutStore.immersiveMode = false;
    windowLayoutStore.sidebarOpen = true;
    windowLayoutStore.rightPanelOpen = false;
    windowLayoutStore.viewportWidth = 1280;
  });

  it("toggles sidebar compact state when clicking the hamburger menu button", async () => {
    windowLayoutStore.sidebarOpen = true;
    windowLayoutStore.setSidebarWidth(256);
    const { getByTitle } = render(TopNavigation);
    const hamburgerBtn = getByTitle("Toggle sidebar (compact / expanded)");

    await fireEvent.click(hamburgerBtn);
    expect(windowLayoutStore.sidebarWidth).toBe(64);

    await fireEvent.click(hamburgerBtn);
    expect(windowLayoutStore.sidebarWidth).toBe(256);
  });

  it("hides the sidebar compact/expand toggle at the same breakpoint that auto-collapses it", () => {
    windowLayoutStore.viewportWidth = 1280;
    expect(windowLayoutStore.isSidebarAutoCollapsed).toBe(false);
    const { queryByTitle } = render(TopNavigation);
    expect(queryByTitle("Toggle sidebar (compact / expanded)")).not.toBeNull();

    windowLayoutStore.viewportWidth = 800;
    expect(windowLayoutStore.isSidebarAutoCollapsed).toBe(true);
    const { queryByTitle: queryByTitleNarrow } = render(TopNavigation);
    expect(queryByTitleNarrow("Toggle sidebar (compact / expanded)")).toBeNull();
  });
});
