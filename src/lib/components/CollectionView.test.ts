import "@testing-library/jest-dom";
import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, fireEvent } from "@testing-library/svelte";
import CollectionView from "./CollectionView.svelte";
import { collectionStore } from "../stores/collection.svelte";
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

vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn().mockResolvedValue(() => {}),
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
      art_embedded: false,
      art_automatic: null,
      art_manual: null,
    },
  ];

  const mockArtists: ArtistItem[] = [
    {
      name: "Band A",
      album_count: 1,
      song_count: 1,
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
      total_artists: 1,
      total_duration_nanosec: 380_000_000_000,
      total_filesize_bytes: 10_000_000,
    };
    collectionStore.activeSubTab = "songs";
    collectionStore.selectedAlbumName = null;
    collectionStore.selectedArtistName = null;
    collectionStore.searchQuery = "";
  });

  it("renders sub-tab filter pills for Artists, Albums, and Songs", () => {
    const { getByRole } = render(CollectionView);
    expect(getByRole("button", { name: /artists/i })).toBeInTheDocument();
    expect(getByRole("button", { name: /albums/i })).toBeInTheDocument();
    expect(getByRole("button", { name: /songs/i })).toBeInTheDocument();
  });

  it("switches active sub-tab when a filter pill is clicked", async () => {
    const { getByRole } = render(CollectionView);

    const albumsBtn = getByRole("button", { name: /albums/i });
    await fireEvent.click(albumsBtn);
    expect(collectionStore.activeSubTab).toBe("albums");

    const artistsBtn = getByRole("button", { name: /artists/i });
    await fireEvent.click(artistsBtn);
    expect(collectionStore.activeSubTab).toBe("artists");
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

  it("renders artists in Artists view sub-tab", () => {
    collectionStore.activeSubTab = "artists";
    const { getByText } = render(CollectionView);

    expect(getByText("Band A")).toBeInTheDocument();
  });

  it("displays empty state when no songs match search query", () => {
    collectionStore.activeSubTab = "songs";
    collectionStore.songs = [];
    collectionStore.searchQuery = "NonexistentTrack";

    const { getByText } = render(CollectionView);
    expect(getByText(/no songs found/i)).toBeInTheDocument();
  });
});
