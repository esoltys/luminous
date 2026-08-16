import "@testing-library/jest-dom";
import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, fireEvent } from "@testing-library/svelte";
import PlayerBar from "./PlayerBar.svelte";
import { playerStore } from "../stores/player.svelte";
import { collectionStore } from "../stores/collection.svelte";
import { playlistsStore } from "../stores/playlists.svelte";
import type { Song, Playlist } from "../types";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn().mockResolvedValue(null),
}));

vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn().mockResolvedValue(() => {}),
}));

if (typeof Element !== "undefined" && !Element.prototype.animate) {
  Element.prototype.animate = vi.fn().mockReturnValue({
    finished: Promise.resolve(),
    cancel: () => {},
  }) as any;
}

describe("PlayerBar.svelte", () => {
  const mockSong: Song = {
    id: 42,
    source: "local_file",
    filetype: "MP3",
    path: "/music/test.mp3",
    title: "Test Track Title",
    artist: "Test Artist",
    album: "Test Album",
    album_artist: "Test Artist",
    composer: "Composer",
    genre: "Rock",
    track: 1,
    disc: 1,
    year: 2024,
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
  };

  beforeEach(() => {
    vi.clearAllMocks();
    playerStore.state = "stopped";
    playerStore.currentSong = undefined;
    playerStore.volume = 0.8;
    playerStore.shuffleMode = "off";
    playerStore.repeatMode = "off";
  });

  it("renders 'Not Playing' state when currentSong is undefined", () => {
    const { getByText } = render(PlayerBar);
    expect(getByText(/nothing playing/i)).toBeInTheDocument();
  });

  it("renders song title, album title, and artist when a song is active", () => {
    playerStore.currentSong = mockSong;
    playerStore.state = "playing";

    const { getByText } = render(PlayerBar);
    expect(getByText("Test Track Title")).toBeInTheDocument();
    expect(getByText("Test Album")).toBeInTheDocument();
    expect(getByText("Test Artist")).toBeInTheDocument();
  });

  it("navigates to album when album title is clicked", async () => {
    playerStore.currentSong = mockSong;
    playerStore.state = "playing";
    const viewAlbumSpy = vi.spyOn(collectionStore, "viewAlbum").mockImplementation(() => {});

    const { getByText } = render(PlayerBar);
    const albumLink = getByText("Test Album");
    await fireEvent.click(albumLink);

    expect(viewAlbumSpy).toHaveBeenCalledWith("Test Album");
  });

  it("exits immersive mode and navigates to the queue when the album cover is clicked", async () => {
    playerStore.currentSong = mockSong;
    playerStore.state = "playing";
    collectionStore.immersiveMode = true;

    const mockQueue: Playlist = {
      id: 1,
      name: "Queue",
      dynamic_enabled: false,
      created: 0,
      updated: 0,
      track_count: 0,
      is_queue: true,
    };
    vi.spyOn(playlistsStore, "requireQueue").mockResolvedValue(mockQueue);
    const selectPlaylistSpy = vi.spyOn(playlistsStore, "selectPlaylist").mockImplementation(async () => {});
    const exitImmersiveModeSpy = vi.spyOn(collectionStore, "exitImmersiveMode");
    const viewPlaylistSpy = vi.spyOn(collectionStore, "viewPlaylist").mockImplementation(() => {});

    const { getByTitle } = render(PlayerBar);
    const coverButton = getByTitle("Queue");
    await fireEvent.click(coverButton);

    expect(exitImmersiveModeSpy).toHaveBeenCalled();
    expect(collectionStore.immersiveMode).toBe(false);
    expect(selectPlaylistSpy).toHaveBeenCalledWith(mockQueue.id);
    expect(viewPlaylistSpy).toHaveBeenCalledWith(mockQueue.id);
  });

  it("enters immersive mode when the album cover is clicked outside immersive mode", async () => {
    playerStore.currentSong = mockSong;
    playerStore.state = "playing";
    collectionStore.immersiveMode = false;

    const toggleImmersiveModeSpy = vi.spyOn(collectionStore, "toggleImmersiveMode");

    const { getByTitle } = render(PlayerBar);
    const coverButton = getByTitle("Toggle Immersive Album Art Screen");
    await fireEvent.click(coverButton);

    expect(toggleImmersiveModeSpy).toHaveBeenCalled();
    expect(collectionStore.immersiveMode).toBe(true);
  });

  it("calls playerStore.resume() when play button is clicked in paused/stopped state", async () => {
    playerStore.currentSong = mockSong;
    playerStore.state = "stopped";
    const resumeSpy = vi.spyOn(playerStore, "resume").mockImplementation(async () => {});

    const { getByTitle } = render(PlayerBar);
    const playBtn = getByTitle("Play");
    await fireEvent.click(playBtn);

    expect(resumeSpy).toHaveBeenCalled();
  });

  it("calls playerStore.pause() when pause button is clicked during playback", async () => {
    playerStore.currentSong = mockSong;
    playerStore.state = "playing";
    const pauseSpy = vi.spyOn(playerStore, "pause").mockImplementation(async () => {});

    const { getByTitle } = render(PlayerBar);
    const pauseBtn = getByTitle(/pause/i);
    await fireEvent.click(pauseBtn);

    expect(pauseSpy).toHaveBeenCalled();
  });

  it("triggers previous and next track navigation", async () => {
    playerStore.currentSong = mockSong;
    const prevSpy = vi.spyOn(playerStore, "previous").mockImplementation(async () => {});
    const nextSpy = vi.spyOn(playerStore, "next").mockImplementation(async () => {});

    const { getByTitle } = render(PlayerBar);
    await fireEvent.click(getByTitle(/previous song/i));
    expect(prevSpy).toHaveBeenCalled();

    await fireEvent.click(getByTitle(/next song/i));
    expect(nextSpy).toHaveBeenCalled();
  });

  it("cycles shuffle modes on shuffle button click", async () => {
    const shuffleSpy = vi.spyOn(playerStore, "setShuffleMode").mockImplementation(async () => {});
    const { getAllByTitle } = render(PlayerBar);

    const shuffleBtn = getAllByTitle(/shuffle/i).find(el => el.querySelector("svg"))!;
    await fireEvent.click(shuffleBtn);

    expect(shuffleSpy).toHaveBeenCalledWith("all");
  });

  it("cycles repeat modes on repeat button click", async () => {
    const repeatSpy = vi.spyOn(playerStore, "setRepeatMode").mockImplementation(async () => {});
    const { getAllByTitle } = render(PlayerBar);

    const repeatBtn = getAllByTitle(/repeat/i).find(el => el.querySelector("svg"))!;
    await fireEvent.click(repeatBtn);

    expect(repeatSpy).toHaveBeenCalledWith("track");
  });

  it("pairs a mode-type icon beside the Shuffle/Repeat icon for disambiguated modes", () => {
    playerStore.shuffleMode = "all";
    playerStore.repeatMode = "off";
    const { getAllByTitle, rerender } = render(PlayerBar);

    const shuffleBtn = getAllByTitle(/shuffle/i).find(el => el.querySelector("svg"))!;
    const repeatBtn = getAllByTitle(/repeat/i).find(el => el.querySelector("svg"))!;

    // "all" shuffle and "off" repeat show only the base transport icon
    expect(shuffleBtn.querySelectorAll("svg").length).toBe(1);
    expect(repeatBtn.querySelectorAll("svg").length).toBe(1);

    playerStore.shuffleMode = "inside_album";
    playerStore.repeatMode = "track";
    rerender({});

    // Disambiguated modes pair a full-size type icon next to the base icon
    expect(shuffleBtn.querySelectorAll("svg").length).toBe(2);
    expect(shuffleBtn.textContent).not.toContain("IA");
    expect(repeatBtn.querySelectorAll("svg").length).toBe(2);
    expect(repeatBtn.textContent).not.toContain("AL");
  });

  it("shows the user-guide description for the active shuffle/repeat mode in the tooltip", () => {
    playerStore.shuffleMode = "artists";
    playerStore.repeatMode = "playlist";
    const { getAllByTitle } = render(PlayerBar);

    expect(getAllByTitle(/before moving to a new, randomly selected artist/i).length).toBeGreaterThan(0);
    expect(getAllByTitle(/loop the current queue or playlist indefinitely/i).length).toBeGreaterThan(0);
  });

  it("handles mute toggle correctly", async () => {
    playerStore.volume = 0.8;
    const volSpy = vi.spyOn(playerStore, "setVolume").mockImplementation(async (v) => { playerStore.volume = v; });

    const { getByRole } = render(PlayerBar);
    const volumeBtn = getByRole("button", { name: /^volume$/i });

    // Mute
    await fireEvent.click(volumeBtn);
    expect(volSpy).toHaveBeenCalledWith(0.0);

    // Unmute
    await fireEvent.click(volumeBtn);
    expect(volSpy).toHaveBeenCalledWith(0.8);
  });
});
