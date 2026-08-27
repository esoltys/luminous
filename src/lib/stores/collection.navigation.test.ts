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

    collectionStore.viewArtist("Pink Floyd");
    expect(collectionStore.selectedArtistName).toBe("Pink Floyd");
    expect(collectionStore.activeTab).toBe("collection");
    expect(collectionStore.activeSubTab).toBe("artists");
    expect(collectionStore.searchQuery).toBe("");
    expect(collectionStore.searchResults).toHaveLength(0);

    collectionStore.searchQuery = "another search";
    collectionStore.viewAlbum("Dark Side");
    expect(collectionStore.selectedAlbumName).toBe("Dark Side");
    expect(collectionStore.searchQuery).toBe("");
  });

  it("persists the selected album/artist detail view to localStorage so a relaunch restores it", () => {
    collectionStore.viewAlbum("Dark Side of the Moon");
    expect(localStorage.getItem("navigation_selectedAlbumName")).toBe("Dark Side of the Moon");
    expect(localStorage.getItem("navigation_selectedArtistName")).toBeNull();

    collectionStore.viewArtist("Pink Floyd");
    expect(localStorage.getItem("navigation_selectedArtistName")).toBe("Pink Floyd");
    expect(localStorage.getItem("navigation_selectedAlbumName")).toBeNull();

    collectionStore.selectedArtistName = null;
    expect(localStorage.getItem("navigation_selectedArtistName")).toBeNull();
  });

  it("supports Back/Forward navigation through viewArtist/viewAlbum history", async () => {
    // Flush the microtask-coalesced history record from any earlier test's navigation
    // before establishing our own baseline entry (history is a shared singleton across tests).
    await Promise.resolve();
    collectionStore.selectedArtistName = null;
    collectionStore.selectedAlbumName = null;
    await Promise.resolve();

    collectionStore.viewArtist("History Test Artist");
    await Promise.resolve();

    collectionStore.viewAlbum("History Test Album");
    await Promise.resolve();

    expect(collectionStore.selectedAlbumName).toBe("History Test Album");
    expect(collectionStore.canGoBack).toBe(true);

    collectionStore.goBack();
    expect(collectionStore.selectedArtistName).toBe("History Test Artist");
    expect(collectionStore.selectedAlbumName).toBeNull();
    expect(collectionStore.canGoForward).toBe(true);

    collectionStore.goForward();
    expect(collectionStore.selectedAlbumName).toBe("History Test Album");
    expect(collectionStore.canGoForward).toBe(false);
  });

  it("truncates forward history when navigating anew from a Back'd-into state", async () => {
    await Promise.resolve();

    collectionStore.viewArtist("Artist A");
    await Promise.resolve();
    collectionStore.viewArtist("Artist B");
    await Promise.resolve();

    collectionStore.goBack();
    expect(collectionStore.selectedArtistName).toBe("Artist A");
    expect(collectionStore.canGoForward).toBe(true);

    collectionStore.viewArtist("Artist C");
    await Promise.resolve();

    expect(collectionStore.selectedArtistName).toBe("Artist C");
    expect(collectionStore.canGoForward).toBe(false);

    collectionStore.goBack();
    expect(collectionStore.selectedArtistName).toBe("Artist A");
  });
});
