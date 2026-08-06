import "@testing-library/jest-dom";
import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, fireEvent } from "@testing-library/svelte";
import AlbumDetailView from "./AlbumDetailView.svelte";
import { collectionStore } from "../stores/collection.svelte";
import { playerStore } from "../stores/player.svelte";
import { playlistsStore } from "../stores/playlists.svelte";
import { invoke } from "@tauri-apps/api/core";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn().mockResolvedValue([]),
}));

vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn().mockResolvedValue(() => {}),
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
    collectionStore.selectedAlbumName = mockAlbumName;
    collectionStore.activeTab = "collection";
    collectionStore.albums = [];
    playlistsStore.playlists = [
      { id: 99, name: "Queue", track_count: 0, created: 100, updated: 100, dynamic_enabled: false },
    ];

    vi.mocked(invoke).mockImplementation((cmd: string) => {
      if (cmd === "get_songs_by_album") {
        return Promise.resolve(mockSongs);
      }
      if (cmd === "get_all_app_settings") {
        return Promise.resolve({});
      }
      if (cmd === "get_playlists") {
        return Promise.resolve([{ id: 99, name: "Queue", track_count: 0, created: 100, updated: 100, dynamic_enabled: false }]);
      }
      if (cmd === "create_playlist") {
        return Promise.resolve({ id: 99, name: "Queue", track_count: 0, created: 100, updated: 100, dynamic_enabled: false });
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
    const viewPlaylistSpy = vi.spyOn(collectionStore, "viewPlaylist");
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
    expect(playSongsSpy).toHaveBeenCalledWith([1, 2], 0, 99, undefined, "Queue");
    expect(viewPlaylistSpy).not.toHaveBeenCalled();
    expect(collectionStore.activeTab).toBe("collection");
  });

  it("switches view to Queue when user clicks Shuffle Play", async () => {
    const viewPlaylistSpy = vi.spyOn(collectionStore, "viewPlaylist");
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

    expect(setShuffleSpy).toHaveBeenCalledWith("all");
    expect(playSongsSpy).toHaveBeenCalled();
    expect(viewPlaylistSpy).toHaveBeenCalledWith(99);
  });
});
