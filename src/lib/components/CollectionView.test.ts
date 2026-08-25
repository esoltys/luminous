import "@testing-library/jest-dom";
import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, fireEvent } from "@testing-library/svelte";
import CollectionView from "./CollectionView.svelte";
import { collectionStore } from "../stores/collection.svelte";
import { prefs } from "../stores/prefs.svelte";
import type { Song, AlbumItem, ArtistItem } from "../types";

vi.mock("svelte-virtual-list-ts", async () => {
  const mod = await import("./__mocks__/VirtualList.svelte");
  return {
    VirtualList: mod.default,
  };
});

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn().mockResolvedValue([]),
}));

describe("CollectionView.svelte", () => {
  const mockSongs: Song[] = [
    {
      id: 1,
      source: "local_file",
      filetype: "MP3",
      path: "/music/song1.mp3",
      title: "Alpha Song",
      artist: "Band A",
      album: "Album 1",
      album_artist: "Band A",
      composer: undefined,
      genre: "Rock",
      track: 1,
      disc: 1,
      year: 2021,
      compilation: false,
      length_nanosec: 200_000_000_000,
      beginning_nanosec: 0,
      end_nanosec: 200_000_000_000,
      rating: 5,
      playcount: 10,
      skipcount: 0,
      art_embedded: false,
      art_unset: false,
      unavailable: false,
    },
    {
      id: 2,
      source: "local_file",
      filetype: "MP3",
      path: "/music/song2.mp3",
      title: "Beta Song",
      artist: "Band B",
      album: "Album 2",
      album_artist: "Band B",
      composer: undefined,
      genre: "Pop",
      track: 2,
      disc: 1,
      year: 2022,
      compilation: false,
      length_nanosec: 180_000_000_000,
      beginning_nanosec: 0,
      end_nanosec: 180_000_000_000,
      rating: 3,
      playcount: 2,
      skipcount: 0,
      art_embedded: false,
      art_unset: false,
      unavailable: false,
    },
  ];

  const mockAlbums: AlbumItem[] = [
    {
      album: "Album 1",
      artist: "Band A",
      year: 2021,
      track_count: 1,
      disc_count: 1,
      art_embedded: false,
      art_automatic: null,
      art_manual: null,
      rating: -1,
      total_duration_nanosec: 0,
    },
  ];

  const mockArtists: ArtistItem[] = [
    {
      name: "Band A",
      album_count: 1,
      song_count: 1,
      genre: "Rock",
    },
    {
      name: "Band B",
      album_count: 1,
      song_count: 1,
      genre: "Pop",
    },
  ];

  beforeEach(() => {
    vi.clearAllMocks();
    collectionStore.songs = mockSongs;
    collectionStore.albums = mockAlbums;
    collectionStore.artists = mockArtists;
    collectionStore.stats = {
      total_songs: 2,
      total_albums: 1,
      total_artists: 2,
      total_duration_nanosec: 380_000_000_000,
      total_filesize_bytes: 10_000_000,
    };
    collectionStore.activeSubTab = "songs";
    collectionStore.selectedAlbumName = null;
    collectionStore.selectedArtistName = null;
    collectionStore.searchQuery = "";
    prefs.albumsViewMode = "cards";
    prefs.artistsViewMode = "cards";
  });

  it("does not render top category filter pills in CollectionView", () => {
    const { queryByRole } = render(CollectionView);
    expect(queryByRole("button", { name: /artists \(/i })).toBeNull();
    expect(queryByRole("button", { name: /albums \(/i })).toBeNull();
    expect(queryByRole("button", { name: /songs \(/i })).toBeNull();
  });

  it("renders songs in Songs view sub-tab", () => {
    collectionStore.activeSubTab = "songs";
    const { getByText } = render(CollectionView);

    expect(getByText("Alpha Song")).toBeInTheDocument();
    expect(getByText("Beta Song")).toBeInTheDocument();
  });

  it("renders albums in Albums view sub-tab", () => {
    collectionStore.activeSubTab = "albums";
    const { getByText } = render(CollectionView);

    expect(getByText("Album 1")).toBeInTheDocument();
  });

  it("renders artists and their genre in Artists view sub-tab", () => {
    collectionStore.activeSubTab = "artists";
    const { getByText } = render(CollectionView);

    expect(getByText("Band A")).toBeInTheDocument();
    expect(getByText("Rock")).toBeInTheDocument();
    expect(getByText("Band B")).toBeInTheDocument();
    expect(getByText("Pop")).toBeInTheDocument();
  });

  it("renders Sort: Genre options in Artists sub-tab", () => {
    collectionStore.activeSubTab = "artists";
    const { getByText } = render(CollectionView);

    expect(getByText("▲ Genre")).toBeInTheDocument();
    expect(getByText("▼ Genre")).toBeInTheDocument();
  });

  it("displays empty state when no songs match search query", () => {
    collectionStore.activeSubTab = "songs";
    collectionStore.songs = [];
    collectionStore.searchQuery = "NonexistentTrack";

    const { getByText } = render(CollectionView);
    expect(getByText(/all results filtered out|no songs found/i)).toBeInTheDocument();
  });

  it("opens context menu on right-click of a song row", async () => {
    collectionStore.activeSubTab = "songs";
    const { getByText, getAllByText } = render(CollectionView);

    const songRow = getByText("Alpha Song").closest("div[ondblclick], div.grid")!;
    await fireEvent.contextMenu(songRow);

    expect(getByText("Play Song")).toBeInTheDocument();
    expect(getAllByText("Add to Active Playlist")[0]).toBeInTheDocument();
  });

  it("defaults to Cards view in Albums sub-tab", () => {
    collectionStore.activeSubTab = "albums";
    const { getByText } = render(CollectionView);

    // AlbumCard renders the title as a clickable LinkButton; the row view doesn't.
    expect(getByText("Album 1").closest("button")).not.toBeNull();
  });

  it("switches Albums sub-tab to the compact Rows view when the Rows toggle is clicked", async () => {
    collectionStore.activeSubTab = "albums";
    const { getByText, getByTitle } = render(CollectionView);

    await fireEvent.click(getByTitle("Row view"));

    expect(prefs.albumsViewMode).toBe("rows");
    expect(getByText("Album 1").closest("button")).toBeNull();
  });

  it("switches Artists sub-tab to the compact Rows view independently of Albums", async () => {
    collectionStore.activeSubTab = "artists";
    const { getByTitle } = render(CollectionView);

    await fireEvent.click(getByTitle("Row view"));

    expect(prefs.artistsViewMode).toBe("rows");
    expect(prefs.albumsViewMode).toBe("cards");
  });

  it("renders Sort: Date Added options in Albums sub-tab", () => {
    collectionStore.activeSubTab = "albums";
    const { getByText } = render(CollectionView);

    expect(getByText("▲ Date Added")).toBeInTheDocument();
    expect(getByText("▼ Date Added")).toBeInTheDocument();
  });

  it("renders each artist in a multi-value credit as its own clickable name", async () => {
    collectionStore.activeSubTab = "songs";
    collectionStore.songs = [
      {
        ...mockSongs[0],
        artist: "Evergrey; Mikael Stanne",
        album_artist: "Evergrey",
      },
    ];

    const { getByText } = render(CollectionView);

    expect(getByText("Evergrey")).toBeInTheDocument();
    const stanneButton = getByText("Mikael Stanne").closest("button")!;
    expect(stanneButton).not.toBeNull();

    // Clicking the second name navigates using just that name, not the
    // album artist and not the full "Evergrey; Mikael Stanne" credit.
    await fireEvent.click(stanneButton);
    expect(collectionStore.selectedArtistName).toBe("Mikael Stanne");
  });

  it("does not use font-mono for the date added column in songs view", () => {
    collectionStore.activeSubTab = "songs";
    collectionStore.visibleColumns.added = true;
    mockSongs[0].added = 1700000000;
    // formatDateAdded falls back to Date.toLocaleDateString() for anything
    // more than 6 days old, which renders in whatever locale the test
    // environment defaults to (not necessarily en-US) — compute the
    // expected string the same way rather than hardcoding one locale's format.
    const expectedDate = new Date(1700000000 * 1000).toLocaleDateString();
    const { getByText } = render(CollectionView);

    const addedCell = getByText(expectedDate).closest("div");
    expect(addedCell).not.toBeNull();
    expect(addedCell).not.toHaveClass("font-mono");
  });
});
