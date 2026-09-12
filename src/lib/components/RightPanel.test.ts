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

  it("hides the MusicBrainz section on the Technical tab when no MusicBrainz IDs are present", () => {
    playerStore.currentSong = mockSong;
    const { queryByAltText } = render(RightPanel);

    expect(queryByAltText("MusicBrainz")).not.toBeInTheDocument();
  });

  it("shows only the MusicBrainz fields present on the song on the Technical tab", () => {
    playerStore.currentSong = {
      ...mockSong,
      album_artist: "Other Artist",
      musicbrainz_artist_id: "artist-uuid",
      musicbrainz_album_artist_id: "album-artist-uuid",
    };
    const { getByAltText, getByText, queryByText } = render(RightPanel);

    expect(getByAltText("MusicBrainz")).toBeInTheDocument();
    expect(getByText("Test Artist")).toBeInTheDocument();
    expect(getByText("Other Artist")).toBeInTheDocument();
    expect(queryByText("artist-uuid")).not.toBeInTheDocument();
    expect(queryByText("Release")).not.toBeInTheDocument();
  });

  it("hides Album Artist when it's the same MusicBrainz entity as Artist on the Technical tab", () => {
    playerStore.currentSong = {
      ...mockSong,
      musicbrainz_artist_id: "same-uuid",
      musicbrainz_album_artist_id: "same-uuid",
    };
    const { getByText, queryByText } = render(RightPanel);

    expect(getByText("Artist")).toBeInTheDocument();
    expect(queryByText("Album Artist")).not.toBeInTheDocument();
  });

  it("falls back to the raw ID when no matching name field is available on the Technical tab", () => {
    playerStore.currentSong = {
      ...mockSong,
      title: "",
      musicbrainz_recording_id: "recording-uuid",
    };
    const { getByText } = render(RightPanel);

    expect(getByText("recording-uuid")).toBeInTheDocument();
  });

  it("opens the MusicBrainz entity page when a MusicBrainz row is clicked on the Technical tab", async () => {
    playerStore.currentSong = {
      ...mockSong,
      musicbrainz_recording_id: "recording-uuid",
    };
    const { getByText } = render(RightPanel);

    await fireEvent.click(getByText("Test Track Title"));

    expect(openExternalUrlMock).toHaveBeenCalledWith(
      "https://musicbrainz.org/recording/recording-uuid"
    );
  });

  it("shows release type/barcode/catalog # as plain text on the Technical tab", () => {
    playerStore.currentSong = {
      ...mockSong,
      musicbrainz_release_type: "album",
      barcode: "4988011329586",
      catalog_number: "PHCR-1144",
    };
    const { getByAltText, getByText, queryByText } = render(RightPanel);

    expect(getByAltText("MusicBrainz")).toBeInTheDocument();
    expect(getByText("Album")).toBeInTheDocument();
    expect(getByText("4988011329586")).toBeInTheDocument();
    expect(getByText("PHCR-1144")).toBeInTheDocument();
    expect(queryByText("Country")).not.toBeInTheDocument();
  });

  it("shows the MusicBrainz section for release metadata alone, with no MusicBrainz IDs at all", () => {
    playerStore.currentSong = {
      ...mockSong,
      barcode: "4988011329586",
    };
    const { getByAltText, getByText, queryByText } = render(RightPanel);

    expect(getByAltText("MusicBrainz")).toBeInTheDocument();
    expect(getByText("Barcode")).toBeInTheDocument();
    expect(queryByText("Artist")).not.toBeInTheDocument();
  });

  it("hides the ListenBrainz section on the Information tab when no MusicBrainz IDs are present", async () => {
    playerStore.currentSong = mockSong;
    const { getByText, queryByAltText } = render(RightPanel);
    await fireEvent.click(getByText("Information"));

    expect(queryByAltText("ListenBrainz")).not.toBeInTheDocument();
  });

  it("shows the ListenBrainz section on the Information tab and opens entity pages on click", async () => {
    playerStore.currentSong = {
      ...mockSong,
      musicbrainz_artist_id: "artist-uuid",
      musicbrainz_release_group_id: "4e8a42f9-d1be-469e-9ac9-ad72d0aa8c39",
      musicbrainz_recording_id: "recording-uuid",
    };
    const { getByText, getByAltText } = render(RightPanel);
    await fireEvent.click(getByText("Information"));

    expect(getByAltText("ListenBrainz")).toBeInTheDocument();
    expect(getByText("Test Artist")).toBeInTheDocument();
    expect(getByText("Test Album")).toBeInTheDocument();
    expect(getByText("Test Track Title")).toBeInTheDocument();

    await fireEvent.click(getByText("Test Track Title"));
    expect(openExternalUrlMock).toHaveBeenCalledWith(
      "https://listenbrainz.org/recording/recording-uuid/"
    );

    await fireEvent.click(getByText("Test Album"));
    expect(openExternalUrlMock).toHaveBeenCalledWith(
      "https://listenbrainz.org/album/4e8a42f9-d1be-469e-9ac9-ad72d0aa8c39/"
    );

    await fireEvent.click(getByText("Test Artist"));
    expect(openExternalUrlMock).toHaveBeenCalledWith(
      "https://listenbrainz.org/artist/artist-uuid/"
    );
  });

  it("opens the album page on ListenBrainz when clicking the ListenBrainz logo button", async () => {
    playerStore.currentSong = {
      ...mockSong,
      musicbrainz_release_group_id: "4e8a42f9-d1be-469e-9ac9-ad72d0aa8c39",
    };
    const { getByText, getByAltText } = render(RightPanel);
    await fireEvent.click(getByText("Information"));

    await fireEvent.click(getByAltText("ListenBrainz"));
    expect(openExternalUrlMock).toHaveBeenCalledWith(
      "https://listenbrainz.org/album/4e8a42f9-d1be-469e-9ac9-ad72d0aa8c39/"
    );
  });

  it("shows the File Path row on the Technicals tab", () => {
    playerStore.currentSong = mockSong;
    const { getByText } = render(RightPanel);

    expect(getByText("File Path:")).toBeInTheDocument();
    expect(getByText("/music/test.flac")).toBeInTheDocument();
  });
});
