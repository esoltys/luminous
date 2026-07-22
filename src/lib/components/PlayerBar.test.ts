import "@testing-library/jest-dom";
import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, fireEvent } from "@testing-library/svelte";
import PlayerBar from "./PlayerBar.svelte";
import { playerStore } from "../stores/player.svelte";
import type { Song } from "../types";

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

  it("renders song title and artist when a song is active", () => {
    playerStore.currentSong = mockSong;
    playerStore.state = "playing";

    const { getByText } = render(PlayerBar);
    expect(getByText("Test Track Title")).toBeInTheDocument();
    expect(getByText("Test Artist")).toBeInTheDocument();
  });

  it("calls playerStore.resume() when play button is clicked in paused/stopped state", async () => {
    playerStore.currentSong = mockSong;
    playerStore.state = "stopped";
    const resumeSpy = vi.spyOn(playerStore, "resume").mockImplementation(async () => {});

    const { getByTitle } = render(PlayerBar);
    const playBtn = getByTitle(/play/i);
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
    await fireEvent.click(getByTitle(/previous track/i));
    expect(prevSpy).toHaveBeenCalled();

    await fireEvent.click(getByTitle(/next track/i));
    expect(nextSpy).toHaveBeenCalled();
  });

  it("cycles shuffle modes on shuffle button click", async () => {
    const shuffleSpy = vi.spyOn(playerStore, "setShuffleMode").mockImplementation(async () => {});
    const { getByTitle } = render(PlayerBar);

    const shuffleBtn = getByTitle(/shuffle/i);
    await fireEvent.click(shuffleBtn);

    expect(shuffleSpy).toHaveBeenCalledWith("all");
  });

  it("cycles repeat modes on repeat button click", async () => {
    const repeatSpy = vi.spyOn(playerStore, "setRepeatMode").mockImplementation(async () => {});
    const { getByTitle } = render(PlayerBar);

    const repeatBtn = getByTitle(/repeat/i);
    await fireEvent.click(repeatBtn);

    expect(repeatSpy).toHaveBeenCalledWith("track");
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
