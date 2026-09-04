import "@testing-library/jest-dom";
import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, fireEvent } from "@testing-library/svelte";
import { invoke } from "@tauri-apps/api/core";
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
    collectionStore.artists = [];
    collectionStore.searchQuery = "";
    collectionStore.extendedArtworkByArtist = {};
    collectionStore.recentSearches = [];
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

  it("hides the right-panel toggle at the same breakpoint that auto-hides the panel itself", () => {
    windowLayoutStore.viewportWidth = 1280;
    expect(windowLayoutStore.isRightPanelAutoHidden).toBe(false);
    const { queryByTitle } = render(TopNavigation);
    expect(queryByTitle("Show Info Panel")).not.toBeNull();

    windowLayoutStore.viewportWidth = 800;
    expect(windowLayoutStore.isRightPanelAutoHidden).toBe(true);
    const { queryByTitle: queryByTitleNarrow } = render(TopNavigation);
    expect(queryByTitleNarrow("Show Info Panel")).toBeNull();
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

  it("shows a discovered artist portrait instead of the generic icon in search suggestions (#98/#761)", async () => {
    collectionStore.artists = [{ name: "Dave Hawkins", album_count: 1, song_count: 6 }];
    vi.mocked(invoke).mockImplementation(async (cmd: string) => {
      if (cmd === "get_extended_artwork_for_artist") {
        return {
          count: 1,
          primary_uri: null,
          artist_portrait_uri: "luminous-art://local/C:/Music/Dave Hawkins/artist.jpg",
          band_logo_uri: null,
          fanart_uri: null,
          items: [],
        };
      }
      return null;
    });

    const { getByPlaceholderText, findByAltText } = render(TopNavigation);
    const searchInput = getByPlaceholderText(/search/i) as HTMLInputElement;
    await fireEvent.focus(searchInput);
    // The artist-suggestions section only renders once there's a non-empty
    // query (an empty-query focus shows Recent Searches instead).
    await fireEvent.input(searchInput, { target: { value: "Dave" } });

    const portrait = await findByAltText("Dave Hawkins");
    expect(portrait.tagName).toBe("IMG");
    expect(portrait.getAttribute("src")).toBe("luminous-art://local/C:/Music/Dave Hawkins/artist.jpg");
  });

  it("shows a discovered artist portrait instead of the generic icon in Recent Searches (#98/#761)", async () => {
    collectionStore.recentSearches = [
      { id: "1", kind: "artist", title: "Dave Hawkins", subtitle: "Artist", timestamp: Date.now() },
    ];
    vi.mocked(invoke).mockImplementation(async (cmd: string) => {
      if (cmd === "get_extended_artwork_for_artist") {
        return {
          count: 1,
          primary_uri: null,
          artist_portrait_uri: "luminous-art://local/C:/Music/Dave Hawkins/artist.jpg",
          band_logo_uri: null,
          fanart_uri: null,
          items: [],
        };
      }
      return null;
    });

    const { getByPlaceholderText, findByAltText } = render(TopNavigation);
    const searchInput = getByPlaceholderText(/search/i) as HTMLInputElement;
    // Empty-query focus shows Recent Searches, not live suggestions.
    await fireEvent.focus(searchInput);

    const portrait = await findByAltText("Dave Hawkins");
    expect(portrait.tagName).toBe("IMG");
    expect(portrait.getAttribute("src")).toBe("luminous-art://local/C:/Music/Dave Hawkins/artist.jpg");
  });

  it("falls back to the stored artUrl for a Recent Searches artist with no discovered portrait", async () => {
    collectionStore.recentSearches = [
      {
        id: "1",
        kind: "artist",
        title: "Old Style Artist",
        subtitle: "Artist",
        artUrl: "luminous-art://album-abc123.jpg",
        timestamp: Date.now(),
      },
    ];
    vi.mocked(invoke).mockImplementation(async (cmd: string) => {
      if (cmd === "get_extended_artwork_for_artist") {
        return {
          count: 0,
          primary_uri: null,
          artist_portrait_uri: null,
          band_logo_uri: null,
          fanart_uri: null,
          items: [],
        };
      }
      return null;
    });

    const { getByPlaceholderText, findByAltText } = render(TopNavigation);
    const searchInput = getByPlaceholderText(/search/i) as HTMLInputElement;
    await fireEvent.focus(searchInput);

    const img = await findByAltText(/Album Art/i);
    expect(img.tagName).toBe("IMG");
  });
});
