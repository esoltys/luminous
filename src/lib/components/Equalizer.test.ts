import "@testing-library/jest-dom";
import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, fireEvent, waitFor } from "@testing-library/svelte";
import Equalizer from "./Equalizer.svelte";
import { invoke } from "@tauri-apps/api/core";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
}));

vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn().mockResolvedValue(() => {}),
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
    crossfade_manual_enabled: true,
    crossfade_manual_duration_ms: 1000,
    crossfade_auto_enabled: false,
    crossfade_auto_duration_secs: 3.0,
    crossfade_suppress_same_album: true,
  };

  beforeEach(() => {
    vi.clearAllMocks();
    vi.mocked(invoke).mockImplementation(async (cmd: string) => {
      if (cmd === "get_equalizer_state") return defaultEqConfig;
      if (cmd === "get_loudness_settings") return defaultLoudness;
      if (cmd === "get_fade_settings") return defaultFadeSettings;
      if (cmd === "get_loudness_analysis_remaining") return 0;
      if (cmd === "load_equalizer_preset") return { gains: [4, 3, 2, -1, -2, -1, 1, 2, 3, 4], parametric: [] };
      return null;
    });
  });

  it("renders equalizer title and preset selector", async () => {
    const { getByText, getByRole } = render(Equalizer);
    await waitFor(() => {
      expect(getByText(/equalizer/i)).toBeInTheDocument();
    });
    expect(getByRole("combobox")).toBeInTheDocument();
  });

  it("toggles equalizer enabled checkbox", async () => {
    const { getByLabelText } = render(Equalizer);
    let checkbox: HTMLInputElement;
    await waitFor(() => {
      checkbox = getByLabelText(/enable eq/i) as HTMLInputElement;
      expect(checkbox).toBeInTheDocument();
    });

    await fireEvent.click(checkbox!);
    expect(invoke).toHaveBeenCalledWith("set_equalizer_enabled", { enabled: false });
  });

  it("switches between Graphic and Parametric modes", async () => {
    const { getByText } = render(Equalizer);
    await waitFor(() => {
      expect(getByText(/parametric/i)).toBeInTheDocument();
    });

    const parametricBtn = getByText(/20-band parametric/i);
    await fireEvent.click(parametricBtn);

    expect(invoke).toHaveBeenCalledWith("set_equalizer_mode", { mode: "parametric20" });
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
    const { getAllByRole } = render(Equalizer);
    let loudnessCheckbox: HTMLInputElement;
    await waitFor(() => {
      const checkboxes = getAllByRole("checkbox") as HTMLInputElement[];
      expect(checkboxes.length).toBeGreaterThanOrEqual(2);
      loudnessCheckbox = checkboxes[1];
    });

    await fireEvent.click(loudnessCheckbox!);
    expect(invoke).toHaveBeenCalledWith("set_loudness_enabled", { enabled: true });
  });
});
