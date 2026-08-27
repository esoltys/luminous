import { describe, it, expect, beforeEach, vi } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type { AlbumItem, ArtistItem } from "../types";

vi.mock("@tauri-apps/api/window", () => ({
  getCurrentWindow: vi.fn(() => ({
    onResized: vi.fn(() => Promise.resolve(() => {})),
    onMoved: vi.fn(() => Promise.resolve(() => {})),
  })),
}));

import { collectionStore } from "./collection.svelte";

describe("CollectionStore - search and tag filtering", () => {
  beforeEach(() => {
    vi.clearAllMocks();

    vi.mocked(listen).mockImplementation(async () => () => {});

    vi.mocked(invoke).mockImplementation(async (cmd: string, args?: any) => {
      switch (cmd) {
        case "geometry_capture_supported":
          return true;
        case "get_all_app_settings":
          return {};
        case "search_songs":
          if (args?.query === "Rock") {
            return [{ id: 1, title: "Rock Track 1", artist: "Rock Band", album: "Rock Album", filetype: "MP3" }];
          }
          return [];
        default:
          return null;
      }
    });
  });

  it("executes FTS search and updates search results and loading state", async () => {
    await collectionStore.search("Rock");
    expect(invoke).toHaveBeenCalledWith("search_songs", { query: "Rock", limit: 500 });
    expect(collectionStore.searchQuery).toBe("Rock");
    expect(collectionStore.searchResults).toHaveLength(1);
    expect(collectionStore.searchResults[0].title).toBe("Rock Track 1");

    await collectionStore.search("");
    expect(collectionStore.searchResults).toHaveLength(0);
  });

  it("filters albums and artists by search query", () => {
    collectionStore.albums = [
      { album: "Dark Side", artist: "Pink Floyd" } as AlbumItem,
      { album: "Abbey Road", artist: "The Beatles" } as AlbumItem
    ];
    collectionStore.artists = [
      { name: "Pink Floyd" } as ArtistItem,
      { name: "The Beatles" } as ArtistItem
    ];

    collectionStore.searchQuery = "beatles";
    expect(collectionStore.filteredAlbums).toHaveLength(1);
    expect(collectionStore.filteredAlbums[0].album).toBe("Abbey Road");

    expect(collectionStore.filteredArtists).toHaveLength(1);
    expect(collectionStore.filteredArtists[0].name).toBe("The Beatles");
  });

  it("filters artists by tags using artist-tag prefix and general search", () => {
    collectionStore.artists = [
      { name: "Shania Twain" } as ArtistItem,
      { name: "The Beatles" } as ArtistItem,
      { name: "Celine Dion" } as ArtistItem,
    ];
    collectionStore.artistProfiles = {
      "shania twain": {
        artist_key: "Shania Twain",
        tags: ["country", "canadian", "pop"],
        social_links: [],
      },
      "celine dion": {
        artist_key: "Celine Dion",
        tags: ["pop", "canadian", "ballad"],
        social_links: [],
      },
      "the beatles": {
        artist_key: "The Beatles",
        tags: ["rock", "british", "classic"],
        social_links: [],
      },
    };

    // Explicit tag search
    collectionStore.searchQuery = "artist-tag:canadian";
    expect(collectionStore.filteredArtists).toHaveLength(2);
    expect(collectionStore.filteredArtists.map((a) => a.name)).toEqual(["Shania Twain", "Celine Dion"]);

    collectionStore.searchQuery = "tag:country";
    expect(collectionStore.filteredArtists).toHaveLength(1);
    expect(collectionStore.filteredArtists[0].name).toBe("Shania Twain");

    // General keyword search matching tag
    collectionStore.searchQuery = "british";
    expect(collectionStore.filteredArtists).toHaveLength(1);
    expect(collectionStore.filteredArtists[0].name).toBe("The Beatles");
  });

  it("filters albums by artist tags using tag prefix and general search", () => {
    collectionStore.albums = [
      { album: "Come On Over", artist: "Shania Twain" } as AlbumItem,
      { album: "The Woman in Me", artist: "Shania Twain" } as AlbumItem,
      { album: "Let's Talk About Love", artist: "Celine Dion" } as AlbumItem,
      { album: "Abbey Road", artist: "The Beatles" } as AlbumItem,
    ];
    collectionStore.artistProfiles = {
      "shania twain": {
        artist_key: "Shania Twain",
        tags: ["country", "canadian", "pop"],
        social_links: [],
      },
      "celine dion": {
        artist_key: "Celine Dion",
        tags: ["pop", "canadian", "ballad"],
        social_links: [],
      },
      "the beatles": {
        artist_key: "The Beatles",
        tags: ["rock", "british", "classic"],
        social_links: [],
      },
    };

    // Explicit tag search with tag: prefix
    collectionStore.searchQuery = "tag:country";
    expect(collectionStore.filteredAlbums).toHaveLength(2);
    expect(collectionStore.filteredAlbums.map((a) => a.album)).toEqual([
      "Come On Over",
      "The Woman in Me",
    ]);

    // Explicit tag search with artist-tag: prefix
    collectionStore.searchQuery = "artist-tag:canadian";
    expect(collectionStore.filteredAlbums).toHaveLength(3);
    expect(collectionStore.filteredAlbums.map((a) => a.album)).toEqual([
      "Come On Over",
      "The Woman in Me",
      "Let's Talk About Love",
    ]);

    // General keyword search matching artist tag
    collectionStore.searchQuery = "british";
    expect(collectionStore.filteredAlbums).toHaveLength(1);
    expect(collectionStore.filteredAlbums[0].album).toBe("Abbey Road");

    // General keyword search matching album title directly
    collectionStore.searchQuery = "over";
    expect(collectionStore.filteredAlbums).toHaveLength(1);
    expect(collectionStore.filteredAlbums[0].album).toBe("Come On Over");
  });

  it("manages recent searches state, deduplication, and persistence", () => {
    collectionStore.clearRecentSearches();
    expect(collectionStore.recentSearches).toHaveLength(0);

    collectionStore.addRecentSearch({
      kind: "query",
      title: "Pink Floyd",
      query: "Pink Floyd"
    });
    expect(collectionStore.recentSearches).toHaveLength(1);
    expect(collectionStore.recentSearches[0].title).toBe("Pink Floyd");

    // Deduplication test
    collectionStore.addRecentSearch({
      kind: "artist",
      title: "The Beatles",
      subtitle: "Artist"
    });
    collectionStore.addRecentSearch({
      kind: "query",
      title: "Pink Floyd",
      query: "Pink Floyd"
    });
    expect(collectionStore.recentSearches).toHaveLength(2);
    expect(collectionStore.recentSearches[0].title).toBe("Pink Floyd");

    // Capacity cap (max 10) test
    for (let i = 1; i <= 12; i++) {
      collectionStore.addRecentSearch({
        kind: "query",
        title: `Search ${i}`,
        query: `Search ${i}`
      });
    }
    expect(collectionStore.recentSearches).toHaveLength(10);
    expect(collectionStore.recentSearches[0].title).toBe("Search 12");

    // Remove single item
    const itemToRemove = collectionStore.recentSearches[0];
    collectionStore.removeRecentSearch(itemToRemove.id);
    expect(collectionStore.recentSearches).toHaveLength(9);
    expect(collectionStore.recentSearches.some(r => r.id === itemToRemove.id)).toBe(false);

    // Clear all
    collectionStore.clearRecentSearches();
    expect(collectionStore.recentSearches).toHaveLength(0);
    expect(localStorage.getItem("luminous_recent_searches")).toBe("[]");
  });
});
