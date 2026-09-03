import "@testing-library/jest-dom";
import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, waitFor, fireEvent } from "@testing-library/svelte";
import { invoke } from "@tauri-apps/api/core";
import LyricsView from "./LyricsView.svelte";
import { playerStore } from "../stores/player.svelte";
import type { Song } from "../types";

const SYNCED_LYRICS = [
  "[00:24.09] I hate living by the hospital",
  "[01:00.43] Baby, it's Halloween",
  "[01:08.72] And we can be anything",
  "[02:41.37] Baby, it's Halloween",
  "[02:49.13] There's a last time for everything",
].join("\n");

let getLyricsResult: string = SYNCED_LYRICS;

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn().mockImplementation((cmd: string) => {
    if (cmd === "get_lyrics") return Promise.resolve(getLyricsResult);
    return Promise.resolve(null);
  }),
}));

const mockSong: Song = {
  id: 12856,
  source: "local_file",
  filetype: "MP3",
  path: "/music/halloween.mp3",
  title: "Halloween",
  artist: "Phoebe Bridgers",
  album: "Punisher",
  album_artist: "Phoebe Bridgers",
  genre: "Indie",
  compilation: false,
  length_nanosec: 271_308_000_000,
  beginning_nanosec: 0,
  end_nanosec: 271_308_000_000,
  rating: -1,
  playcount: 0,
  skipcount: 0,
  art_embedded: false,
  art_unset: false,
  unavailable: false,
};

describe("LyricsView.svelte", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    getLyricsResult = SYNCED_LYRICS;
    playerStore.currentSong = undefined;
    playerStore.positionNanosec = 0;
    // jsdom's scrollIntoView is a no-op stub; spy on it so tests can assert
    // which behavior ("auto" vs "smooth") LyricsView actually requested.
    if (!Element.prototype.scrollIntoView) {
      Element.prototype.scrollIntoView = () => {};
    }
    vi.spyOn(Element.prototype, "scrollIntoView").mockImplementation(() => {});
  });

  it("shows the not-playing state when no song is loaded", () => {
    const { getAllByText } = render(LyricsView);
    // Rendered in both the header (song title slot) and the empty-state body.
    expect(getAllByText("Nothing playing").length).toBeGreaterThan(0);
  });

  it("highlights the lyric line matching the current playback position, not an earlier line with identical text", async () => {
    playerStore.currentSong = mockSong;
    // 2:43 — between the second "Baby, it's Halloween" (2:41.37) and the
    // following line (2:49.13); the first occurrence of that same text sits
    // at 1:00.43 and must NOT be the one marked active.
    playerStore.positionNanosec = 163_000_000_000;

    const { getAllByText } = render(LyricsView);

    await waitFor(() => {
      const matches = getAllByText("Baby, it's Halloween");
      expect(matches).toHaveLength(2);
    });

    const matches = getAllByText("Baby, it's Halloween");
    expect(matches[0].className).not.toContain("scale-105");
    expect(matches[1].className).toContain("scale-105");
  });

  it("jumps to the active line instantly on first render instead of animating (avoids losing the race with scroll-position restore)", async () => {
    playerStore.currentSong = mockSong;
    playerStore.positionNanosec = 163_000_000_000;

    render(LyricsView);

    await waitFor(() => {
      expect(Element.prototype.scrollIntoView).toHaveBeenCalled();
    });
    const firstCall = (Element.prototype.scrollIntoView as ReturnType<typeof vi.fn>).mock.calls[0];
    expect(firstCall[0]).toMatchObject({ behavior: "auto" });
  });

  it("falls back to plain text when lyrics aren't in synced LRC format", async () => {
    getLyricsResult = "Just some plain, unsynced lyrics.";
    playerStore.currentSong = mockSong;

    const { getByText } = render(LyricsView);

    await waitFor(() => {
      expect(getByText("Just some plain, unsynced lyrics.")).toBeInTheDocument();
    });
    expect(getByText("Synced lyrics not available. Showing plain text.")).toBeInTheDocument();
  });

  it("renders only a single unmark instrumental button in the header when an instrumental track is active", async () => {
    playerStore.currentSong = {
      ...mockSong,
      is_instrumental: true,
    };

    const { getAllByText, getByText } = render(LyricsView);

    expect(getByText("Instrumental Song")).toBeInTheDocument();
    expect(getByText("This song is marked as instrumental. Online lyrics search is bypassed.")).toBeInTheDocument();

    const buttons = getAllByText("Unmark Instrumental");
    expect(buttons).toHaveLength(1);

    await fireEvent.click(buttons[0]);
    expect(invoke).toHaveBeenCalledWith("set_instrumental", {
      songId: mockSong.id,
      isInstrumental: false,
    });
  });
});
