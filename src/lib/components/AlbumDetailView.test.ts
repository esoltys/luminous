import "@testing-library/jest-dom";
import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, fireEvent } from "@testing-library/svelte";
import AlbumDetailView from "./AlbumDetailView.svelte";
import { collectionStore } from "../stores/collection.svelte";
import { navigationStore } from "../stores/navigation.svelte";
import { playerStore } from "../stores/player.svelte";
import { playlistsStore } from "../stores/playlists.svelte";
import { picardStore } from "../stores/picard.svelte";
import { invoke } from "@tauri-apps/api/core";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn().mockResolvedValue([]),
}));

vi.mock("@tauri-apps/api/window", () => ({
  getCurrentWindow: () => ({
    innerSize: () => Promise.resolve({ width: 800, height: 600 }),
    outerSize: () => Promise.resolve({ width: 800, height: 600 }),
    listen: vi.fn().mockResolvedValue(() => {}),
    onResized: vi.fn().mockResolvedValue(() => {}),
    onMoved: vi.fn().mockResolvedValue(() => {}),
  }),
}));

describe("AlbumDetailView.svelte - Play vs Shuffle Play Queue navigation", () => {
  const mockAlbumName = "Abbey Road";
  const mockSongs = [
    {
      id: 1,
      title: "Come Together",
      artist: "The Beatles",
      album: "Abbey Road",
      length_nanosec: 259_000_000_000,
    },
    {
      id: 2,
      title: "Something",
      artist: "The Beatles",
      album: "Abbey Road",
      length_nanosec: 183_000_000_000,
    },
  ];

  beforeEach(() => {
    vi.clearAllMocks();
    navigationStore.selectedAlbumName = mockAlbumName;
    navigationStore.activeTab = "collection";
    collectionStore.albums = [];
    playlistsStore.playlists = [
      { id: 99, name: "Queue", track_count: 0, created: 100, updated: 100, dynamic_enabled: false, is_queue: true },
    ];

    vi.mocked(invoke).mockImplementation((cmd: string) => {
      if (cmd === "get_songs_by_album") {
        return Promise.resolve(mockSongs);
      }
      if (cmd === "get_all_app_settings") {
        return Promise.resolve({});
      }
      if (cmd === "get_playlists") {
        return Promise.resolve([{ id: 99, name: "Queue", track_count: 0, created: 100, updated: 100, dynamic_enabled: false, is_queue: true }]);
      }
      if (cmd === "create_playlist") {
        return Promise.resolve({ id: 99, name: "Queue", track_count: 0, created: 100, updated: 100, dynamic_enabled: false, is_queue: true });
      }
      if (cmd === "get_playlist_items") {
        return Promise.resolve([]);
      }
      if (cmd === "set_shuffle_mode" || cmd === "play_songs") {
        return Promise.resolve(undefined);
      }
      return Promise.resolve([]);
    });
  });

  it("does not switch views to Queue when user clicks Play", async () => {
    const viewPlaylistSpy = vi.spyOn(navigationStore, "viewPlaylist");
    const playSongsSpy = vi.spyOn(playerStore, "playSongs");
    const setShuffleSpy = vi.spyOn(playerStore, "setShuffleMode");

    const { getByText } = render(AlbumDetailView, {
      props: { albumName: mockAlbumName },
    });

    // Wait for songs to load
    await new Promise((resolve) => setTimeout(resolve, 50));

    // Find the header "Play" button exactly
    const playButton = getByText("Play").closest("button")!;
    await fireEvent.click(playButton);
    await new Promise((resolve) => setTimeout(resolve, 50));

    expect(setShuffleSpy).toHaveBeenCalledWith("off");
    expect(playSongsSpy).toHaveBeenCalledWith([1, 2], 0, undefined, {
      type: "album",
      album: mockAlbumName,
      albumArtist: "The Beatles",
    });
    expect(viewPlaylistSpy).not.toHaveBeenCalled();
    expect(navigationStore.activeTab).toBe("collection");
  });

  it("retains view on Shuffle Play while updating playback and queue", async () => {
    const viewPlaylistSpy = vi.spyOn(navigationStore, "viewPlaylist");
    const playSongsSpy = vi.spyOn(playerStore, "playSongs");
    const setShuffleSpy = vi.spyOn(playerStore, "setShuffleMode");

    const { getByText } = render(AlbumDetailView, {
      props: { albumName: mockAlbumName },
    });

    // Wait for songs to load
    await new Promise((resolve) => setTimeout(resolve, 50));

    // Find the "Shuffle Play" button exactly
    const shuffleButton = getByText("Shuffle Play").closest("button")!;
    await fireEvent.click(shuffleButton);
    await new Promise((resolve) => setTimeout(resolve, 50));

    expect(setShuffleSpy).toHaveBeenCalledWith("off");
    expect(playSongsSpy).toHaveBeenCalled();
    expect(viewPlaylistSpy).not.toHaveBeenCalled();
    expect(navigationStore.activeTab).toBe("collection");
  });

  it("adds all album songs to Queue when clicking the + button with no active custom playlist", async () => {
    playlistsStore.activeCustomPlaylist = null;
    const addSongsSpy = vi.spyOn(playlistsStore, "addSongsToQueue");

    const { getByTitle } = render(AlbumDetailView, {
      props: { albumName: mockAlbumName },
    });

    await new Promise((resolve) => setTimeout(resolve, 50));

    const addButton = getByTitle("Add all songs to Queue");
    await fireEvent.click(addButton);
    await new Promise((resolve) => setTimeout(resolve, 50));

    expect(addSongsSpy).toHaveBeenCalledWith([1, 2]);
    expect(invoke).toHaveBeenCalledWith("add_songs_to_queue", { songIds: [1, 2] });
  });

  it("renders genre chips on their own line and begins year on the next line", async () => {
    const songsWithGenreAndYear = [
      {
        id: 1,
        title: "Song 1",
        artist: "Krisu",
        album: "Oxygen for a Dying World",
        genre: "Electronic; Chill Out; Trip-Hop; Lounge",
        year: 2024,
        length_nanosec: 259_000_000_000,
      },
    ];

    vi.mocked(invoke).mockImplementation((cmd: string) => {
      if (cmd === "get_songs_by_album") {
        return Promise.resolve(songsWithGenreAndYear);
      }
      return Promise.resolve([]);
    });

    const { getByText, getAllByText } = render(AlbumDetailView, {
      props: { albumName: "Oxygen for a Dying World" },
    });

    await new Promise((resolve) => setTimeout(resolve, 50));

    // Verify genre chips are rendered
    const electronicChip = getByText("Electronic");
    const chillOutChip = getByText("Chill Out");
    expect(electronicChip).toBeInTheDocument();
    expect(chillOutChip).toBeInTheDocument();

    // Verify genre container is distinct from the metadata container with year
    const genreContainer = electronicChip.closest("div.flex.flex-wrap.gap-1");
    expect(genreContainer).not.toBeNull();

    const yearElements = getAllByText("2024");
    expect(yearElements.length).toBeGreaterThan(0);
    const headerYear = yearElements[0];
    expect(headerYear).toBeInTheDocument();
    expect(genreContainer?.contains(headerYear)).toBe(false);

    // The metadata row starts with the year
    const metadataRow = headerYear.closest("div.flex.flex-wrap.items-center");
    expect(metadataRow).not.toBeNull();
    expect(metadataRow?.firstElementChild).toBe(headerYear);
  });

  it("opens overflow menu with Edit album info and Open in Picard", async () => {
    picardStore.path = "/mock/picard";
    const { getByTitle, getByText, queryByText } = render(AlbumDetailView, {
      props: { albumName: mockAlbumName },
    });

    await new Promise((resolve) => setTimeout(resolve, 50));

    expect(queryByText("Edit album info")).toBeNull();
    expect(queryByText("Open in Picard")).toBeNull();

    const moreBtn = getByTitle("More actions");
    await fireEvent.click(moreBtn);
    await new Promise((resolve) => setTimeout(resolve, 50));

    const editItem = getByText("Edit album info");
    const picardItem = getByText("Open in Picard");
    expect(editItem).toBeInTheDocument();
    expect(picardItem).toBeInTheDocument();

    await fireEvent.click(picardItem);
    expect(invoke).toHaveBeenCalledWith("open_in_picard", { songIds: [1, 2] });
  });
});

