import "@testing-library/jest-dom";
import { describe, it, expect, beforeEach } from "vitest";
import { render, fireEvent } from "@testing-library/svelte";
import QualityBadge from "./QualityBadge.svelte";
import { playerStore } from "../stores/player.svelte";
import type { Song, AudioPipelineInfo } from "../types";

describe("QualityBadge.svelte", () => {
  const baseSong: Song = {
    id: 1,
    source: "local_file",
    filetype: "FLAC",
    path: "/music/test.flac",
    title: "Test Song",
    artist: "Test Artist",
    album: "Test Album",
    album_artist: "Test Artist",
    composer: undefined,
    genre: "Rock",
    track: 1,
    disc: 1,
    year: 2024,
    compilation: false,
    length_nanosec: 180_000_000_000,
    beginning_nanosec: 0,
    end_nanosec: 180_000_000_000,
    rating: 0,
    playcount: 0,
    skipcount: 0,
    art_embedded: false,
    art_unset: false,
    unavailable: false,
    bitrate: 920,
    samplerate: 96000,
    bitdepth: 24,
  };

  const mockPipeline: AudioPipelineInfo = {
    quality_tier: "hi-res",
    input_source: "local_file",
    input_format: "FLAC",
    input_codec: "flac",
    input_bitrate_kbps: 800,
    input_sample_rate: 44100,
    input_bit_depth: 16,
    input_channels: 2,
    decoder_name: "Symphonia FLAC decoder",
    headroom: "Direct passthrough (no DSP)",
    loudness_source: "disabled",
    eq_enabled: false,
    eq_active_bands_count: 0,
    limiter: "None",
    output_sample_rate: 44100,
    output_channels: 2,
    output_format: "32-bit float",
    output_device_name: "Speakers",
    output_backend: "WASAPI",
  };

  beforeEach(() => {
    playerStore.currentSong = undefined;
    playerStore.audioPipeline = null;
  });

  it("renders nothing when no song is loaded", () => {
    const { queryByRole } = render(QualityBadge);
    expect(queryByRole("button")).toBeNull();
  });

  it("renders Hi-Res badge for 24-bit 96kHz FLAC", () => {
    playerStore.currentSong = baseSong;
    const { getByText } = render(QualityBadge);
    expect(getByText("Hi-Res")).toBeInTheDocument();
  });

  it("renders HQ badge for standard 16-bit 44.1kHz FLAC", () => {
    playerStore.currentSong = {
      ...baseSong,
      samplerate: 44100,
      bitdepth: 16,
    };
    const { getByText } = render(QualityBadge);
    expect(getByText("HQ")).toBeInTheDocument();
  });

  it("renders SQ badge for 320 kbps MP3", () => {
    playerStore.currentSong = {
      ...baseSong,
      filetype: "MP3",
      bitrate: 320,
      samplerate: 44100,
      bitdepth: undefined,
    };
    const { getByText } = render(QualityBadge);
    expect(getByText("SQ")).toBeInTheDocument();
  });

  it("renders LQ badge for 128 kbps MP3", () => {
    playerStore.currentSong = {
      ...baseSong,
      filetype: "MP3",
      bitrate: 128,
      samplerate: 44100,
      bitdepth: undefined,
    };
    const { getByText } = render(QualityBadge);
    expect(getByText("LQ")).toBeInTheDocument();
  });

  it("uses quality_tier from audioPipeline when available", () => {
    playerStore.currentSong = baseSong;
    playerStore.audioPipeline = mockPipeline;
    const { getByText } = render(QualityBadge);
    expect(getByText("Hi-Res")).toBeInTheDocument();
  });

  it("toggles popover when clicked", async () => {
    playerStore.currentSong = baseSong;
    const { getByText, getByRole } = render(QualityBadge);

    const button = getByText("Hi-Res");
    await fireEvent.click(button);

    expect(getByRole("dialog")).toBeInTheDocument();
    expect(getByText("Audio Pipeline")).toBeInTheDocument();
  });
});
