import "@testing-library/jest-dom";
import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, fireEvent } from "@testing-library/svelte";
import Miniplayer from "./Miniplayer.svelte";
import { playerStore } from "../stores/player.svelte";
import { collectionStore } from "../stores/collection.svelte";
import type { Song } from "../types";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn().mockResolvedValue(null),
}));

vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn().mockResolvedValue(() => {}),
}));

vi.mock("@tauri-apps/api/window", () => ({
  getCurrentWindow: vi.fn().mockReturnValue({
    startResizing: vi.fn().mockResolvedValue(undefined),
    startDragging: vi.fn().mockResolvedValue(undefined),
  }),
}));

describe("Miniplayer.svelte", () => {
  const mockSong: Song = {
    id: 101,
    source: "local_file",
    filetype: "FLAC",
    path: "/music/ambient.flac",
    title: "Starlight Echoes",
    artist: "Lunar Drift",
    album: "Solaris",
    album_artist: "Lunar Drift",
    genre: "Ambient",
    compilation: false,
    length_nanosec: 240_000_000_000,
    beginning_nanosec: 0,
    end_nanosec: 240_000_000_000,
    rating: 5,
    playcount: 12,
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
    collectionStore.isMiniplayer = true;
  });

  it("renders idle layout with current song title and artist", () => {
    playerStore.currentSong = mockSong;
    const { getAllByText } = render(Miniplayer);
    expect(getAllByText("Starlight Echoes").length).toBeGreaterThan(0);
    expect(getAllByText("Lunar Drift").length).toBeGreaterThan(0);
  });

  it("toggles play/pause when play button is clicked", async () => {
    playerStore.currentSong = mockSong;
    playerStore.state = "paused";
    const resumeSpy = vi.spyOn(playerStore, "resume").mockResolvedValue();

    const { getByTitle } = render(Miniplayer);
    const playBtn = getByTitle("Play");
    await fireEvent.click(playBtn);

    expect(resumeSpy).toHaveBeenCalled();
  });

  it("cycles shuffle and repeat modes displaying badges", async () => {
    playerStore.currentSong = mockSong;
    const { getByTitle, getByText } = render(Miniplayer);

    const shuffleBtn = getByTitle(/Shuffle:/i);
    const repeatBtn = getByTitle(/Repeat:/i);

    // Click shuffle -> all (no badge) -> inside_album (IA badge)
    await fireEvent.click(shuffleBtn); // all
    await fireEvent.click(shuffleBtn); // inside_album
    expect(getByText("IA")).toBeInTheDocument();

    // Click repeat -> track (Repeat1 icon) -> album (AL badge)
    await fireEvent.click(repeatBtn); // track
    await fireEvent.click(repeatBtn); // album
    expect(getByText("AL")).toBeInTheDocument();
  });

  it("renders the song rating widget and rates the current song", async () => {
    playerStore.currentSong = mockSong; // rating: 5 -> favorited under the default heart style
    const rateSpy = vi.spyOn(playerStore, "rateCurrent").mockResolvedValue(undefined as any);

    const { getByTitle } = render(Miniplayer);
    const heartBtn = getByTitle("Remove from favorites");
    await fireEvent.click(heartBtn);

    expect(rateSpy).toHaveBeenCalledWith(-1);
  });

  it("exits miniplayer mode when restore button is clicked or Escape is pressed", async () => {
    playerStore.currentSong = mockSong;
    const exitSpy = vi.spyOn(collectionStore, "exitMiniplayerMode");

    const { getByTitle, getByRole } = render(Miniplayer);
    const restoreBtn = getByTitle(/Restore Full Window/i);
    await fireEvent.click(restoreBtn);

    expect(exitSpy).toHaveBeenCalled();

    const region = getByRole("region");
    await fireEvent.keyDown(region, { key: "Escape" });
    expect(exitSpy).toHaveBeenCalledTimes(2);
  });
});
