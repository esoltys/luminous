import "@testing-library/jest-dom";
import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, fireEvent } from "@testing-library/svelte";
import PlaylistView from "./PlaylistView.svelte";
import { playlistsStore } from "../stores/playlists.svelte";
import type { Playlist, PlaylistItem } from "../types";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn().mockResolvedValue([]),
}));

vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn().mockResolvedValue(() => {}),
}));

describe("PlaylistView.svelte", () => {
  const mockPlaylist: Playlist = {
    id: 1,
    name: "My Playlist",
    track_count: 3,
    created: 1700000000,
    dynamic_enabled: false,
  };

  const mockTracks: PlaylistItem[] = [
    {
      id: 10,
      playlist_id: 1,
      position: 0,
      item_type: "song",
      uuid: "uuid-1",
      song: {
        id: 101,
        source: "local_file",
        filetype: "MP3",
        path: "/music/track1.mp3",
        title: "Track One",
        artist: "Artist A",
        album: "Album A",
        album_artist: "Artist A",
        genre: "Rock",
        track: 1,
        disc: 1,
        year: 2021,
        compilation: false,
        length_nanosec: 180_000_000_000,
        beginning_nanosec: 0,
        end_nanosec: 180_000_000_000,
        rating: 4,
        playcount: 5,
        skipcount: 0,
        art_embedded: false,
        art_unset: false,
        unavailable: false,
      },
    },
    {
      id: 11,
      playlist_id: 1,
      position: 1,
      item_type: "song",
      uuid: "uuid-2",
      song: {
        id: 102,
        source: "local_file",
        filetype: "MP3",
        path: "/music/track2.mp3",
        title: "Track Two",
        artist: "Artist B",
        album: "Album B",
        album_artist: "Artist B",
        genre: "Pop",
        track: 2,
        disc: 1,
        year: 2022,
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
    },
    {
      id: 12,
      playlist_id: 1,
      position: 2,
      item_type: "song",
      uuid: "uuid-3",
      song: {
        id: 103,
        source: "local_file",
        filetype: "MP3",
        path: "/music/track3.mp3",
        title: "Track Three",
        artist: "Artist C",
        album: "Album C",
        album_artist: "Artist C",
        genre: "Jazz",
        track: 3,
        disc: 1,
        year: 2020,
        compilation: false,
        length_nanosec: 150_000_000_000,
        beginning_nanosec: 0,
        end_nanosec: 150_000_000_000,
        rating: 3,
        playcount: 2,
        skipcount: 0,
        art_embedded: false,
        art_unset: false,
        unavailable: false,
      },
    },
  ];

  beforeEach(() => {
    vi.clearAllMocks();
    playlistsStore.playlists = [mockPlaylist];
    playlistsStore.activePlaylistId = 1;
    playlistsStore.activePlaylistTracks = [...mockTracks];
  });

  it("renders playlist tracks", () => {
    const { getByText } = render(PlaylistView);
    expect(getByText("My Playlist")).toBeInTheDocument();
    expect(getByText("Track One")).toBeInTheDocument();
    expect(getByText("Track Two")).toBeInTheDocument();
    expect(getByText("Track Three")).toBeInTheDocument();
  });

  it("handles drag and drop reordering of playlist items", async () => {
    const reorderSpy = vi.spyOn(playlistsStore, "reorderItem");
    const { getByText } = render(PlaylistView);

    const rowOne = getByText("Track One").closest("tr")!;
    const rowThree = getByText("Track Three").closest("tr")!;

    const dataTransfer = {
      setData: vi.fn(),
      getData: vi.fn().mockReturnValue("0"),
      effectAllowed: "",
      dropEffect: "",
    };

    // Drag start on row 0
    await fireEvent.dragStart(rowOne, { dataTransfer });
    expect(dataTransfer.setData).toHaveBeenCalledWith("text/plain", "0");

    // Drag over row 2
    await fireEvent.dragOver(rowThree, { dataTransfer });
    expect(dataTransfer.dropEffect).toBe("move");

    // Drop on row 2
    await fireEvent.drop(rowThree, { dataTransfer });
    expect(reorderSpy).toHaveBeenCalledWith(1, 0, 2);
  });

  it("falls back to dataTransfer string data on drop if draggedIndex was cleared", async () => {
    const reorderSpy = vi.spyOn(playlistsStore, "reorderItem");
    const { getByText } = render(PlaylistView);

    const rowTwo = getByText("Track Two").closest("tr")!;

    const dataTransfer = {
      setData: vi.fn(),
      getData: vi.fn().mockReturnValue("0"),
      effectAllowed: "",
      dropEffect: "",
    };

    // Trigger drop directly with text/plain data = 0 on row index 1
    await fireEvent.drop(rowTwo, { dataTransfer });
    expect(reorderSpy).toHaveBeenCalledWith(1, 0, 1);
  });

  it("filters tracks by title or artist using the filter search input", async () => {
    const { getByPlaceholderText, getByText, queryByText } = render(PlaylistView);
    const input = getByPlaceholderText("Filter tracks...");

    await fireEvent.input(input, { target: { value: "Track One" } });
    expect(getByText("Track One")).toBeInTheDocument();
    expect(queryByText("Track Two")).not.toBeInTheDocument();
    expect(queryByText("Track Three")).not.toBeInTheDocument();
  });

  it("detects duplicate tracks and triggers deduplication", async () => {
    playlistsStore.activePlaylistTracks = [
      ...mockTracks,
      {
        id: 13,
        playlist_id: 1,
        position: 3,
        item_type: "song",
        uuid: "uuid-4-dup",
        song: { ...mockTracks[0].song! },
      },
    ];

    const dedupeSpy = vi.spyOn(playlistsStore, "deduplicatePlaylist");
    const { getByText } = render(PlaylistView);

    expect(getByText(/Remove 1 duplicate/)).toBeInTheDocument();
    const btn = getByText(/Remove 1 duplicate/).closest("button")!;
    await fireEvent.click(btn);

    expect(dedupeSpy).toHaveBeenCalledWith(1);
  });

  it("handles multi-selection with Shift+Click and shows batch floating bar", async () => {
    const { getByText, queryByText } = render(PlaylistView);

    const rowOne = getByText("Track One").closest("tr")!;
    const rowThree = getByText("Track Three").closest("tr")!;

    await fireEvent.click(rowOne);
    await fireEvent.click(rowThree, { shiftKey: true });

    expect(getByText("3 selected")).toBeInTheDocument();

    const removeBtn = getByText("Remove Selected").closest("button")!;
    const removeSpy = vi.spyOn(playlistsStore, "removeItemsFromPlaylist");
    await fireEvent.click(removeBtn);

    expect(removeSpy).toHaveBeenCalledWith(1, ["uuid-1", "uuid-2", "uuid-3"]);
  });

  it("opens context menu on right-click", async () => {
    const { getByText, getAllByText } = render(PlaylistView);
    const rowOne = getByText("Track One").closest("tr")!;

    await fireEvent.contextMenu(rowOne);

    expect(getAllByText("Play Selected").length).toBeGreaterThan(0);
    expect(getByText("Remove from Playlist")).toBeInTheDocument();
  });
});
