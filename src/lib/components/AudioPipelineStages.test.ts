import "@testing-library/jest-dom";
import { describe, it, expect } from "vitest";
import { render } from "@testing-library/svelte";
import AudioPipelineStages from "./AudioPipelineStages.svelte";
import type { AudioPipelineInfo } from "../types";

function pipeline(overrides: Partial<AudioPipelineInfo> = {}): AudioPipelineInfo {
  return {
    quality_tier: "hi-res",
    input_source: "collection",
    input_format: "FLAC",
    input_codec: "flac",
    input_sample_rate: 48_000,
    input_bit_depth: 24,
    input_channels: 2,
    decoder_name: "Symphonia FLAC decoder",
    headroom: "32-bit float PCM",
    loudness_source: "disabled",
    eq_enabled: false,
    eq_active_bands_count: 0,
    limiter: "-1.0 dBTP True Peak limiter",
    output_sample_rate: 44_100,
    output_channels: 2,
    output_format: "16-bit integer PCM",
    output_device_name: "USB DAC",
    output_backend: "WASAPI Exclusive",
    output_bypasses_mixer: true,
    output_fell_back: false,
    ...overrides,
  };
}

describe("AudioPipelineStages", () => {
  it("shows a resampled track as resampled to the output rate", () => {
    // The backend reports the rate it resamples to: the output rate.
    const { getByText, queryByText } = render(AudioPipelineStages, {
      pipeline: pipeline({ resample_rate: 44_100 }),
    });

    expect(getByText("Resampled")).toBeInTheDocument();
    expect(getByText("Resampled to 44.1 kHz")).toBeInTheDocument();
    expect(queryByText("Direct rate")).not.toBeInTheDocument();
  });

  it("shows a track at the device's rate as direct", () => {
    const { getByText } = render(AudioPipelineStages, {
      pipeline: pipeline({ input_sample_rate: 44_100 }),
    });

    expect(getByText("Direct rate")).toBeInTheDocument();
    expect(getByText("Direct (no resampling)")).toBeInTheDocument();
  });

  it("never calls shared output bit-perfect or direct", () => {
    // Untouched by Luminous, but the system mixer comes after it.
    const { getByText, queryByText } = render(AudioPipelineStages, {
      pipeline: pipeline({
        input_sample_rate: 44_100,
        output_backend: "CPAL",
        output_bypasses_mixer: false,
        output_fell_back: true,
      }),
    });

    expect(queryByText("Bit-perfect")).not.toBeInTheDocument();
    expect(queryByText("Direct rate")).not.toBeInTheDocument();
    expect(queryByText("Processed")).not.toBeInTheDocument();
    expect(getByText("System mixer")).toBeInTheDocument();
    expect(getByText("None in Luminous; the system mixer may convert")).toBeInTheDocument();
  });

  it("calls an untouched track on a bit-perfect output bit-perfect", () => {
    const { getByText } = render(AudioPipelineStages, {
      pipeline: pipeline({ input_sample_rate: 44_100 }),
    });

    expect(getByText("Bit-perfect")).toBeInTheDocument();
  });
});
