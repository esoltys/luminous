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
  });

  it("reflects correct toggle active class for immersive mode when off vs on", async () => {
    const { getByTitle } = render(TopNavigation);
    const immersiveBtn = getByTitle("Toggle Immersive Album Art Screen");

    // When immersiveMode is false, button should NOT have the active class 'bg-brand-accent text-white'
    expect(immersiveBtn.className).not.toContain("bg-brand-accent text-white");
    expect(immersiveBtn.className).toContain("text-brand-text-secondary");

    // Toggle immersiveMode to true
    await fireEvent.click(immersiveBtn);

    expect(collectionStore.immersiveMode).toBe(true);
    expect(immersiveBtn.className).toContain("bg-brand-accent text-white");
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
});
