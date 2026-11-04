import "@testing-library/jest-dom";
import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, fireEvent } from "@testing-library/svelte";
import { invoke } from "@tauri-apps/api/core";
import TopNavigation from "./TopNavigation.svelte";
import { collectionStore } from "../stores/collection.svelte";
import { windowLayoutStore } from "../stores/windowLayout.svelte";
import { navigationStore } from "../stores/navigation.svelte";
import { filesStore } from "../stores/files.svelte";


describe("TopNavigation.svelte", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    windowLayoutStore.immersiveMode = false;
    windowLayoutStore.sidebarOpen = true;
    windowLayoutStore.rightPanelOpen = false;
    windowLayoutStore.viewportWidth = 1280;
    collectionStore.artists = [];
    collectionStore.searchQuery = "";
    collectionStore.folderSearchResults = [];
    collectionStore.extendedArtworkByArtist = {};
    collectionStore.recentSearches = [];
    navigationStore.activeTab = "collection";
    navigationStore.filesLocation = null;
    filesStore.sources = [
      { source: { kind: "folder", id: 1 }, name: "Lossless archive", songCount: 15, available: true },
    ];
    filesStore.filterQuery = "";
    filesStore.filterMatchCount = 0;
    filesStore.filterTotalCount = 0;
    filesStore.activeFolderTitle = "";
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
      { id: "1", kind: "artist", title: "Dave Hawkins", subtitle: "Artist", timestamp: 1_700_000_000_000 },
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
        timestamp: 1_700_000_000_000,
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

  it("switches to folder filter mode when a folder is open in Files and escapes to whole-library search (#1886)", async () => {
    navigationStore.activeTab = "files";
    navigationStore.filesLocation = { source: { kind: "folder", id: 1 }, path: ["Katatonia", "2006 - The Great Cold Distance"] };
    filesStore.activeFolderTitle = "2006 - The Great Cold Distance";
    filesStore.filterMatchCount = 2;
    filesStore.filterTotalCount = 15;

    const { getByPlaceholderText, getByTestId, getByText } = render(TopNavigation);
    const input = getByPlaceholderText("Filter 2006 - The Great Cold Distance…") as HTMLInputElement;

    await fireEvent.focus(input);
    expect(getByText("Filter this folder by name, type or library status")).toBeInTheDocument();

    await fireEvent.click(getByText("ext:flac"));
    expect(filesStore.filterQuery).toBe("ext:flac ");

    await fireEvent.input(input, { target: { value: "ext:flac it" } });
    expect(getByTestId("files-filter-badge")).toHaveTextContent("2 of 15");

    const escapeRow = getByTestId("files-search-whole-library");
    expect(escapeRow).toHaveTextContent('Search your whole library for "it"');

    await fireEvent.click(escapeRow);
    expect(navigationStore.activeTab).toBe("collection");
    expect(collectionStore.searchQuery).toBe("it");
    expect(filesStore.filterQuery).toBe("");
  });

  it("focuses the search bar with Ctrl+F or / and clears the Files filter with Escape (#1886)", async () => {
    navigationStore.activeTab = "files";
    navigationStore.filesLocation = { source: { kind: "folder", id: 1 }, path: ["Rock"] };
    filesStore.activeFolderTitle = "Rock";
    filesStore.filterQuery = "flac";

    const { getByPlaceholderText } = render(TopNavigation);
    const input = getByPlaceholderText("Filter Rock…") as HTMLInputElement;

    await fireEvent.keyDown(window, { key: "/" });
    expect(document.activeElement).toBe(input);

    await fireEvent.keyDown(window, { key: "Escape" });
    expect(filesStore.filterQuery).toBe("");

    await fireEvent.keyDown(window, { key: "f", ctrlKey: true });
    expect(document.activeElement).toBe(input);
  });

  it("renders matching folders under Files & Folders in global search suggestions and opens the clicked folder (#1886)", async () => {
    collectionStore.folderSearchResults = [
      {
        source: { kind: "folder", id: 1 },
        path: ["Metal", "Katatonia"],
        name: "Katatonia",
        breadcrumb: "Lossless archive / Metal / Katatonia",
      },
    ];

    const { getByPlaceholderText, getByTestId, getByText } = render(TopNavigation);
    const input = getByPlaceholderText(/search/i) as HTMLInputElement;
    collectionStore.searchQuery = "katat";
    await fireEvent.focus(input);

    expect(getByText("Files & Folders")).toBeInTheDocument();
    const row = getByTestId("search-folder-result");
    expect(row).toHaveTextContent("Katatonia");
    expect(row).toHaveTextContent("Lossless archive / Metal / Katatonia");

    await fireEvent.click(row);
    expect(navigationStore.activeTab).toBe("files");
    expect(navigationStore.filesLocation).toEqual({
      source: { kind: "folder", id: 1 },
      path: ["Metal", "Katatonia"],
      selectedName: undefined,
    });
  });
});
