import "@testing-library/jest-dom";
import { describe, it, expect, vi, beforeEach } from "vitest";
import { render } from "@testing-library/svelte";
import RightPanel from "./RightPanel.svelte";
import { playerStore } from "../stores/player.svelte";
import type { Song } from "../types";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn().mockResolvedValue(null),
}));

vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn().mockResolvedValue(() => {}),
}));

describe("RightPanel.svelte", () => {
  const mockSong: Song = {
    id: 42,
    source: "local_file",
    filetype: "FLAC",
    path: "/music/test.flac",
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
    rating: 5,
    playcount: 10,
    skipcount: 0,
    art_embedded: false,
    art_unset: false,
    unavailable: false,
  };

  beforeEach(() => {
    vi.clearAllMocks();
    playerStore.state = "stopped";
    playerStore.currentSong = undefined;
  });

  it("renders 'Not Playing' when no current song", () => {
    const { getByText } = render(RightPanel);
    expect(getByText(/nothing playing/i)).toBeInTheDocument();
  });

  it("renders song title, artist, album, format and bitrate when song is set", () => {
    playerStore.currentSong = mockSong;
    const { getByText } = render(RightPanel);

    expect(getByText("Test Track Title")).toBeInTheDocument();
    expect(getByText("Test Artist")).toBeInTheDocument();
    expect(getByText("Test Album")).toBeInTheDocument();
    expect(getByText("FLAC")).toBeInTheDocument();
  });

  it("renders a plain bitrate for CBR files", () => {
    playerStore.currentSong = { ...mockSong, bitrate: 320, is_vbr: false };
    const { getByText } = render(RightPanel);

    expect(getByText("320 kbps")).toBeInTheDocument();
  });

  it("labels the bitrate as an average for VBR files", () => {
    playerStore.currentSong = { ...mockSong, bitrate: 245, is_vbr: true };
    const { getByText } = render(RightPanel);

    expect(getByText("245 kbps (avg)")).toBeInTheDocument();
  });
});
