import "@testing-library/jest-dom";
import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, fireEvent } from "@testing-library/svelte";
import ImmersiveSessionWrap from "./ImmersiveSessionWrap.svelte";
import { playerStore, type CompletedSession } from "../stores/player.svelte";
import { collectionStore } from "../stores/collection.svelte";
import { windowLayoutStore } from "../stores/windowLayout.svelte";
import type { Song } from "../types";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn().mockResolvedValue(null),
}));

describe("ImmersiveSessionWrap.svelte (#1380)", () => {
  const mockSession: CompletedSession = {
    contextName: "Queue",
    isQueue: true,
    songIds: [1, 2, 3],
    trackCount: 3,
    completedAt: Date.now(),
  };

  const mockSong: Song = {
    id: 1,
    source: "local_file",
    filetype: "FLAC",
    path: "/music/track1.flac",
    title: "Track One",
    artist: "Artist A",
    album: "Album A",
    album_artist: "Artist A",
    genre: "Rock",
    compilation: false,
    length_nanosec: 180_000_000_000,
    beginning_nanosec: 0,
    end_nanosec: 180_000_000_000,
    rating: 0,
    playcount: 1,
    skipcount: 0,
    art_embedded: false,
    art_unset: false,
    unavailable: false,
  };

  beforeEach(() => {
    vi.clearAllMocks();
    playerStore.completedSession = { ...mockSession };
    windowLayoutStore.immersiveMode = true;
    collectionStore.songs = [mockSong];
  });

  it("renders completion screen with milestone badge, title, and tracks count", () => {
    const { getByTestId, getByText } = render(ImmersiveSessionWrap, { session: mockSession });
    expect(getByTestId("immersive-session-wrap")).toBeInTheDocument();
    expect(getByText("Queue Complete")).toBeInTheDocument();
    expect(getByText("3 tracks played")).toBeInTheDocument();
    expect(getByText("Shuffle Library")).toBeInTheDocument();
    expect(getByText("Replay")).toBeInTheDocument();
    expect(getByText("Exit Immersive")).toBeInTheDocument();
  });

  it("displays context-aware title and singular track text when context is custom and 1 track", () => {
    const singleTrackSession: CompletedSession = {
      contextName: "Chill Mix",
      isQueue: false,
      songIds: [1],
      trackCount: 1,
      completedAt: Date.now(),
    };
    const { getByText } = render(ImmersiveSessionWrap, { session: singleTrackSession });
    expect(getByText("Chill Mix Complete")).toBeInTheDocument();
    expect(getByText("1 track played")).toBeInTheDocument();
  });

  it("calls playerStore.shuffleLibrary when Shuffle Library button is clicked", async () => {
    const shuffleSpy = vi.spyOn(playerStore, "shuffleLibrary").mockResolvedValue(undefined);
    const { getByText } = render(ImmersiveSessionWrap, { session: mockSession });
    const shuffleBtn = getByText("Shuffle Library");
    await fireEvent.click(shuffleBtn);
    expect(shuffleSpy).toHaveBeenCalledWith([mockSong]);
  });

  it("calls playerStore.replayCompletedSession when Replay button is clicked", async () => {
    const replaySpy = vi.spyOn(playerStore, "replayCompletedSession").mockResolvedValue(undefined);
    const { getByText } = render(ImmersiveSessionWrap, { session: mockSession });
    const replayBtn = getByText("Replay");
    await fireEvent.click(replayBtn);
    expect(replaySpy).toHaveBeenCalled();
  });

  it("exits immersive mode and clears completedSession when Exit Immersive button is clicked", async () => {
    const exitSpy = vi.spyOn(windowLayoutStore, "exitImmersiveMode");
    const { getByText } = render(ImmersiveSessionWrap, { session: mockSession });
    const exitBtn = getByText("Exit Immersive");
    await fireEvent.click(exitBtn);
    expect(playerStore.completedSession).toBeNull();
    expect(exitSpy).toHaveBeenCalled();
  });

  it("exits immersive mode when Escape key is pressed", async () => {
    const exitSpy = vi.spyOn(windowLayoutStore, "exitImmersiveMode");
    render(ImmersiveSessionWrap, { session: mockSession });
    await fireEvent.keyDown(window, { key: "Escape" });
    expect(playerStore.completedSession).toBeNull();
    expect(exitSpy).toHaveBeenCalled();
  });

  it("replays session when Space key is pressed", async () => {
    const replaySpy = vi.spyOn(playerStore, "replayCompletedSession").mockResolvedValue(undefined);
    render(ImmersiveSessionWrap, { session: mockSession });
    await fireEvent.keyDown(window, { code: "Space", key: " " });
    expect(replaySpy).toHaveBeenCalled();
  });
});
