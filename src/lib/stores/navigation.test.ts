import { describe, it, expect, beforeEach, vi } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type { Song } from "../types";

vi.mock("@tauri-apps/api/window", () => ({
  getCurrentWindow: vi.fn(() => ({
    onResized: vi.fn(() => Promise.resolve(() => {})),
    onMoved: vi.fn(() => Promise.resolve(() => {})),
  })),
}));

import { collectionStore } from "./collection.svelte";
import { navigationStore } from "./navigation.svelte";

describe("CollectionStore - artist/album navigation and history", () => {
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

  it("handles navigation helpers viewArtist and viewAlbum and clears search terms", () => {
    collectionStore.searchQuery = "some search";
    collectionStore.searchResults = [{ id: 1 } as Song];

    navigationStore.viewArtist("Pink Floyd");
    expect(navigationStore.selectedArtistName).toBe("Pink Floyd");
    expect(navigationStore.activeTab).toBe("collection");
    expect(navigationStore.activeSubTab).toBe("artists");
    expect(collectionStore.searchQuery).toBe("");
    expect(collectionStore.searchResults).toHaveLength(0);

    collectionStore.searchQuery = "another search";
    navigationStore.viewAlbum("Dark Side");
    expect(navigationStore.selectedAlbumName).toBe("Dark Side");
    expect(collectionStore.searchQuery).toBe("");
  });

  it("persists the selected album/artist detail view to localStorage so a relaunch restores it", () => {
    navigationStore.viewAlbum("Dark Side of the Moon");
    expect(localStorage.getItem("navigation_selectedAlbumName")).toBe("Dark Side of the Moon");
    expect(localStorage.getItem("navigation_selectedArtistName")).toBeNull();

    navigationStore.viewArtist("Pink Floyd");
    expect(localStorage.getItem("navigation_selectedArtistName")).toBe("Pink Floyd");
    expect(localStorage.getItem("navigation_selectedAlbumName")).toBeNull();

    navigationStore.selectedArtistName = null;
    expect(localStorage.getItem("navigation_selectedArtistName")).toBeNull();
  });

  it("supports Back/Forward navigation through viewArtist/viewAlbum history", async () => {
    // Flush the microtask-coalesced history record from any earlier test's navigation
    // before establishing our own baseline entry (history is a shared singleton across tests).
    await Promise.resolve();
    navigationStore.selectedArtistName = null;
    navigationStore.selectedAlbumName = null;
    await Promise.resolve();

    navigationStore.viewArtist("History Test Artist");
    await Promise.resolve();

    navigationStore.viewAlbum("History Test Album");
    await Promise.resolve();

    expect(navigationStore.selectedAlbumName).toBe("History Test Album");
    expect(navigationStore.canGoBack).toBe(true);

    navigationStore.goBack();
    expect(navigationStore.selectedArtistName).toBe("History Test Artist");
    expect(navigationStore.selectedAlbumName).toBeNull();
    expect(navigationStore.canGoForward).toBe(true);

    navigationStore.goForward();
    expect(navigationStore.selectedAlbumName).toBe("History Test Album");
    expect(navigationStore.canGoForward).toBe(false);
  });

  it("truncates forward history when navigating anew from a Back'd-into state", async () => {
    await Promise.resolve();

    navigationStore.viewArtist("Artist A");
    await Promise.resolve();
    navigationStore.viewArtist("Artist B");
    await Promise.resolve();

    navigationStore.goBack();
    expect(navigationStore.selectedArtistName).toBe("Artist A");
    expect(navigationStore.canGoForward).toBe(true);

    navigationStore.viewArtist("Artist C");
    await Promise.resolve();

    expect(navigationStore.selectedArtistName).toBe("Artist C");
    expect(navigationStore.canGoForward).toBe(false);

    navigationStore.goBack();
    expect(navigationStore.selectedArtistName).toBe("Artist A");
  });
});
