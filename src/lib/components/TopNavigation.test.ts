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
