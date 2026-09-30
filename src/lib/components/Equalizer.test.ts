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
      { kind: "peak", freq: 60, gain_db: 0, q: 1.0, enabled: true },
      { kind: "peak", freq: 1000, gain_db: 0, q: 1.0, enabled: true },
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
    eq: {
      freq: { min: 20, max: 20000 },
      gain_db: { min: -12, max: 12 },
      q: { min: 0.1, max: 10 },
      preamp: { min: -12, max: 12 },
      max_bands: 20,
      min_bands: 1,
    },
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
      if (cmd === "get_parametric_response") return args.frequencies.map(() => 0);
      return null;
    });
  });

  describe("parametric response curve (#1248)", () => {
    const parametricConfig = { ...defaultEqConfig, mode: "parametric" };

    function mockResponse(respond: (freqs: number[]) => number[]) {
      vi.mocked(invoke).mockImplementation(async (cmd: string, args?: any) => {
        if (cmd === "get_equalizer_state") return parametricConfig;
        if (cmd === "apply_equalizer_config") return args?.config;
        if (cmd === "get_loudness_settings") return defaultLoudness;
        if (cmd === "get_fade_settings") return defaultFadeSettings;
        if (cmd === "get_audio_setting_ranges") return defaultRanges;
        if (cmd === "get_parametric_response") return respond(args.frequencies);
        return null;
      });
    }

    function responseCalls() {
      return vi.mocked(invoke).mock.calls.filter(([cmd]) => cmd === "get_parametric_response") as [
        string,
        { frequencies: number[]; band?: number },
      ][];
    }

    function curveYs(container: HTMLElement): number[] {
      const d = container.querySelector('[data-testid="eq-response"]')?.getAttribute("d") ?? "";
      // Every segment ends at "x y" — the on-curve sample points.
      return [...d.matchAll(/(?:M|,)\s*([\d.]+) ([\d.-]+)(?=\s*(?:C|$))/g)].map((m) => Number(m[2]));
    }

    it("plots the backend's evaluated response instead of re-deriving it", async () => {
      mockResponse((freqs) => freqs.map(() => 12));
      const { container } = render(Equalizer);
      await waitFor(() => expect(curveYs(container).length).toBe(96));
      // +12 dB everywhere maps every sample to the top of the plot.
      for (const y of curveYs(container)) expect(y).toBeCloseTo(3);
      const call = vi.mocked(invoke).mock.calls.find(([cmd]) => cmd === "get_parametric_response");
      const freqs = (call![1] as { frequencies: number[] }).frequencies;
      expect(freqs[0]).toBeCloseTo(20);
      expect(freqs[freqs.length - 1]).toBeCloseTo(20000);
    });

    it("re-fetches the response after a band change", async () => {
      let level = 0;
      mockResponse((freqs) => freqs.map(() => level));
      const { container, getByLabelText } = render(Equalizer);
      await waitFor(() => expect(curveYs(container).length).toBe(96));
      for (const y of curveYs(container)) expect(y).toBeCloseTo(20);

      level = -12;
      const gain = getByLabelText("Gain 1") as HTMLInputElement;
      await fireEvent.change(gain, { target: { value: "-6" } });
      await waitFor(() => {
        for (const y of curveYs(container)) expect(y).toBeCloseTo(37);
      });
      expect(invoke).toHaveBeenCalledWith(
        "apply_equalizer_config",
        expect.objectContaining({
          config: expect.objectContaining({
            parametric: [expect.objectContaining({ freq: 60, gain_db: -6 }), expect.objectContaining({ freq: 1000 })],
          }),
        })
      );
    });

    it("asks the backend for the selected band's own response (#1333)", async () => {
      mockResponse((freqs) => freqs.map(() => 0));
      const { container, getByRole } = render(Equalizer);
      await waitFor(() => expect(responseCalls().some(([, a]) => a.band === 0)).toBe(true));

      await fireEvent.pointerDown(getByRole("button", { name: /^Band 2:/ }), { pointerId: 1 });
      await waitFor(() => expect(responseCalls().some(([, a]) => a.band === 1)).toBe(true));
      expect(container.querySelector('[data-testid="eq-band-response"]')).not.toBeNull();
    });

    it("keeps the newest edit when an older apply echoes back late", async () => {
      const echoes: Array<() => void> = [];
      mockResponse((freqs) => freqs.map(() => 0));
      const base = vi.mocked(invoke).getMockImplementation()!;
      vi.mocked(invoke).mockImplementation(async (cmd: string, args?: any) => {
        if (cmd !== "apply_equalizer_config") return base(cmd, args);
        const config = JSON.parse(JSON.stringify(args.config));
        return new Promise((resolve) => echoes.push(() => resolve(config)));
      });
      const { getByLabelText } = render(Equalizer);
      let gain!: HTMLInputElement;
      await waitFor(() => (gain = getByLabelText("Gain 1") as HTMLInputElement));

      await fireEvent.change(gain, { target: { value: "3" } });
      await fireEvent.change(gain, { target: { value: "5" } });
      await fireEvent.change(gain, { target: { value: "7" } });
      // One apply in flight; the two later edits coalesce into one follow-up.
      expect(echoes).toHaveLength(1);
      echoes[0]();
      await waitFor(() => expect(echoes).toHaveLength(2));
      echoes[1]();
      await waitFor(() => expect(gain.value).toBe("7"));
      const applies = vi.mocked(invoke).mock.calls.filter(([cmd]) => cmd === "apply_equalizer_config");
      expect(applies).toHaveLength(2);
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

  it("draws the EQ gain sliders' range from the backend, not a retyped ±12 (#1332)", async () => {
    const backendRanges = {
      ...defaultRanges,
      eq: { ...defaultRanges.eq, gain_db: { min: -15, max: 15 } },
    };
    vi.mocked(invoke).mockImplementation(async (cmd: string) => {
      if (cmd === "get_audio_setting_ranges") return backendRanges;
      if (cmd === "get_fade_settings") return defaultFadeSettings;
      if (cmd === "get_loudness_settings") return defaultLoudness;
      if (cmd === "get_equalizer_state") return defaultEqConfig;
      return null;
    });
    const { container } = render(Equalizer);
    await waitFor(() => {
      const sliders = container.querySelectorAll<HTMLInputElement>('input[type="range"][orient="vertical"]');
      expect(sliders).toHaveLength(10);
      for (const s of sliders) {
        expect(s.min).toBe("-15");
        expect(s.max).toBe("15");
      }
    });
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
    const { getByRole } = render(Equalizer);
    let parametricBtn: HTMLElement;
    await waitFor(() => {
      parametricBtn = getByRole("button", { name: /^parametric$/i });
    });

    await fireEvent.click(parametricBtn!);

    expect(invoke).toHaveBeenCalledWith(
      "apply_equalizer_config",
      expect.objectContaining({ config: expect.objectContaining({ mode: "parametric" }) })
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
