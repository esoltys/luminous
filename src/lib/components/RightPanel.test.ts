import "@testing-library/jest-dom";
import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, fireEvent } from "@testing-library/svelte";
import RightPanel from "./RightPanel.svelte";
import { playerStore } from "../stores/player.svelte";
import type { Song } from "../types";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn().mockResolvedValue(null),
}));

const openExternalUrlMock = vi.fn();
vi.mock("../utils/openExternalUrl", () => ({
  openExternalUrl: (url: string) => openExternalUrlMock(url),
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

  it("renders format when song is set", () => {
    playerStore.currentSong = mockSong;
    const { getByText } = render(RightPanel);

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

  it("renders Mono channel info when channels is 1", () => {
    playerStore.currentSong = { ...mockSong, channels: 1 };
    const { getByText } = render(RightPanel);

    expect(getByText("Channels")).toBeInTheDocument();
    expect(getByText("Mono")).toBeInTheDocument();
  });

  it("renders Stereo channel info when channels is 2", () => {
    playerStore.currentSong = { ...mockSong, channels: 2 };
    const { getByText } = render(RightPanel);

    expect(getByText("Channels")).toBeInTheDocument();
    expect(getByText("Stereo")).toBeInTheDocument();
  });

  it("renders 5.1 Surround channel info when channels is 6", () => {
    playerStore.currentSong = { ...mockSong, channels: 6 };
    const { getByText } = render(RightPanel);

    expect(getByText("Channels")).toBeInTheDocument();
    expect(getByText("5.1 Surround")).toBeInTheDocument();
  });

  it("hides the MusicBrainz section when no MusicBrainz IDs are present", () => {
    playerStore.currentSong = mockSong;
    const { queryByText } = render(RightPanel);

    expect(queryByText("MusicBrainz")).not.toBeInTheDocument();
  });

  it("shows only the MusicBrainz fields present on the song", () => {
    playerStore.currentSong = {
      ...mockSong,
      musicbrainz_artist_id: "artist-uuid",
      musicbrainz_album_artist_id: "album-artist-uuid",
    };
    const { getByText, queryByText } = render(RightPanel);

    expect(getByText("MusicBrainz")).toBeInTheDocument();
    expect(getByText("artist-uuid")).toBeInTheDocument();
    expect(getByText("album-artist-uuid")).toBeInTheDocument();
    expect(queryByText("Release")).not.toBeInTheDocument();
  });

  it("opens the MusicBrainz entity page when a MusicBrainz ID is clicked", async () => {
    playerStore.currentSong = {
      ...mockSong,
      musicbrainz_recording_id: "recording-uuid",
    };
    const { getByText } = render(RightPanel);

    await fireEvent.click(getByText("recording-uuid"));

    expect(openExternalUrlMock).toHaveBeenCalledWith(
      "https://musicbrainz.org/recording/recording-uuid"
    );
  });
});
