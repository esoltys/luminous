import "@testing-library/jest-dom";
import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, fireEvent, waitFor } from "@testing-library/svelte";
import Equalizer from "./Equalizer.svelte";
import { invoke } from "@tauri-apps/api/core";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
}));

describe("Equalizer.svelte", () => {
  const defaultEqConfig = {
    enabled: true,
    mode: "graphic10",
    preamp: 0.0,
    gains: [0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
    parametric: [
      { freq: 60, gain_db: 0, q: 1.0 },
      { freq: 1000, gain_db: 0, q: 1.0 },
    ],
  };

  const defaultLoudness = {
    enabled: false,
    target_lufs: -18.0,
    mode: "track",
    fallback_gain_db: -6.0,
  };

  const defaultFadeSettings = {
    fade_pause_enabled: true,
    fade_pause_duration_ms: 300,
    crossfade_auto_enabled: false,
    crossfade_auto_duration_secs: 3.0,
    crossfade_suppress_same_album: true,
  };

  const defaultRanges = {
    target_lufs: { min: -23, max: -9 },
    fallback_gain_db: { min: -12, max: 0 },
    fade_pause_duration_ms: { min: 0, max: 1000 },
    crossfade_auto_duration_secs: { min: 0, max: 8 },
  };

  beforeEach(() => {
    vi.clearAllMocks();
    vi.mocked(invoke).mockImplementation(async (cmd: string, args?: any) => {
      if (cmd === "get_equalizer_state") return defaultEqConfig;
      // The backend echoes the applied config back (post-clamping).
      if (cmd === "apply_equalizer_config") return args?.config;
      if (cmd === "get_loudness_settings") return defaultLoudness;
      if (cmd === "get_fade_settings") return defaultFadeSettings;
      if (cmd === "get_audio_setting_ranges") return defaultRanges;
      if (cmd === "get_loudness_analysis_remaining") return 0;
      if (cmd === "load_equalizer_preset") return { gains: [4, 3, 1, -1, -2, -1, 1, 3, 3.5, 3.5], parametric: [] };
      return null;
    });
  });

  it("draws the fade slider's range from the backend, not a retyped literal (#1249)", async () => {
    const backendRanges = {
      ...defaultRanges,
      fade_pause_duration_ms: { min: 0, max: 2000 },
    };
    vi.mocked(invoke).mockImplementation(async (cmd: string) => {
      if (cmd === "get_audio_setting_ranges") return backendRanges;
      if (cmd === "get_fade_settings") return defaultFadeSettings;
      if (cmd === "get_loudness_settings") return defaultLoudness;
      if (cmd === "get_equalizer_state") return defaultEqConfig;
      return null;
    });
    const { container, getByText } = render(Equalizer);
    await waitFor(() => {
      const slider = container.querySelector<HTMLInputElement>('input[type="range"][max="2000"]');
      expect(slider).not.toBeNull();
    });
    expect(getByText("2000")).toBeInTheDocument();
  });

  it("renders equalizer title and preset selector", async () => {
    const { getByText, getByRole } = render(Equalizer);
    await waitFor(() => {
      expect(getByText(/equalizer/i)).toBeInTheDocument();
    });
    expect(getByRole("combobox")).toBeInTheDocument();
  });

  it("toggles equalizer enabled switch", async () => {
    const { getByLabelText } = render(Equalizer);
    let toggle: HTMLElement;
    await waitFor(() => {
      toggle = getByLabelText(/enable eq/i);
      expect(toggle).toBeInTheDocument();
    });

    await fireEvent.click(toggle!);
    expect(invoke).toHaveBeenCalledWith(
      "apply_equalizer_config",
      expect.objectContaining({ config: expect.objectContaining({ enabled: false }) })
    );
  });

  it("switches between Graphic and Parametric modes", async () => {
    const { getByText } = render(Equalizer);
    await waitFor(() => {
      expect(getByText(/20-band/i)).toBeInTheDocument();
    });

    const parametricBtn = getByText(/20-band/i);
    await fireEvent.click(parametricBtn);

    expect(invoke).toHaveBeenCalledWith(
      "apply_equalizer_config",
      expect.objectContaining({ config: expect.objectContaining({ mode: "parametric20" }) })
    );
  });

  it("loads a preset when selected", async () => {
    const { getByRole } = render(Equalizer);
    let selectEl: HTMLSelectElement;
    await waitFor(() => {
      selectEl = getByRole("combobox") as HTMLSelectElement;
      expect(selectEl).toBeInTheDocument();
    });

    await fireEvent.change(selectEl!, { target: { value: "Rock" } });
    expect(invoke).toHaveBeenCalledWith("load_equalizer_preset", { presetName: "Rock" });
  });

  it("handles loudness normalization toggle", async () => {
    const { getByLabelText } = render(Equalizer);
    let loudnessToggle: HTMLElement;
    await waitFor(() => {
      loudnessToggle = getByLabelText(/loudness normalization/i);
      expect(loudnessToggle).toBeInTheDocument();
    });

    await fireEvent.click(loudnessToggle!);
    expect(invoke).toHaveBeenCalledWith("set_loudness_settings", {
      settings: { enabled: true, target_lufs: -18.0, mode: "track", fallback_gain_db: -6.0 },
    });
  });

  it("does not wrap loudness toggle to a new line and uses items-start layout", async () => {
    const { getByText } = render(Equalizer);
    await waitFor(() => {
      expect(getByText(/loudness normalization/i)).toBeInTheDocument();
    });

    const titleEl = getByText(/loudness normalization/i);
    const headerContainer = titleEl.closest("div.flex.items-start.justify-between");
    expect(headerContainer).not.toBeNull();
    expect(headerContainer).not.toHaveClass("flex-wrap");
  });

  it("renders ISO 266:1997 footnote in 10-band graphic mode", async () => {
    const { getByText } = render(Equalizer);
    await waitFor(() => {
      expect(getByText("ISO 266:1997")).toBeInTheDocument();
    });
  });
});
