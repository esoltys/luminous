import "@testing-library/jest-dom";
import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, fireEvent, waitFor, screen } from "@testing-library/svelte";
import Equalizer from "./Equalizer.svelte";
import { invoke } from "@tauri-apps/api/core";
import { open, save } from "@tauri-apps/plugin-dialog";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
}));

vi.mock("@tauri-apps/plugin-dialog", () => ({
  open: vi.fn(),
  save: vi.fn(),
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

  const defaultPresets = { builtin: ["Flat", "Rock", "Pop"], user: [] };

  beforeEach(() => {
    vi.clearAllMocks();
    vi.mocked(invoke).mockImplementation(async (cmd: string, args?: any) => {
      if (cmd === "get_equalizer_state") return defaultEqConfig;
      if (cmd === "list_eq_presets") return defaultPresets;
      // The backend echoes the applied config back (post-clamping).
      if (cmd === "apply_equalizer_config") return args?.config;
      if (cmd === "get_loudness_settings") return defaultLoudness;
      if (cmd === "get_fade_settings") return defaultFadeSettings;
      if (cmd === "get_audio_setting_ranges") return defaultRanges;
      if (cmd === "get_loudness_analysis_remaining") return 0;
      if (cmd === "load_equalizer_preset")
        return { ...defaultEqConfig, gains: [4, 3, 1, -1, -2, -1, 1, 3, 3.5, 3.5], active_preset: args.presetName };
      if (cmd === "get_parametric_response") return args.frequencies.map(() => 0);
      if (cmd === "get_eq_preset_previews") {
        const freqs = args?.frequencies ?? [];
        return defaultPresets.builtin.map((key) => ({ key, response_db: freqs.map(() => 0) }));
      }
      return null;
    });
  });

  describe("parametric response curve (#1248)", () => {
    const parametricConfig = { ...defaultEqConfig, mode: "parametric" };

    function mockResponse(respond: (freqs: number[]) => number[]) {
      vi.mocked(invoke).mockImplementation(async (cmd: string, args?: any) => {
        if (cmd === "get_equalizer_state") return parametricConfig;
        if (cmd === "list_eq_presets") return defaultPresets;
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

  describe("user presets (#1335)", () => {
    const studio = { id: 7, name: "Studio" };
    let state: Record<string, unknown>;
    let presets: { builtin: string[]; user: { id: number; name: string }[] };

    function mockPresetBackend(overrides: Record<string, (args: any) => unknown> = {}) {
      state = { ...defaultEqConfig, mode: "parametric", active_preset: "user:7" };
      presets = { builtin: ["Flat", "Rock"], user: [studio] };
      vi.mocked(invoke).mockImplementation(async (cmd: string, args?: any) => {
        if (overrides[cmd]) return overrides[cmd](args);
        if (cmd === "get_equalizer_state") return state;
        if (cmd === "list_eq_presets") return presets;
        // Like the engine: any edit leaves the preset, so the echo is Custom.
        if (cmd === "apply_equalizer_config") return { ...args.config, active_preset: null };
        if (cmd === "get_loudness_settings") return defaultLoudness;
        if (cmd === "get_fade_settings") return defaultFadeSettings;
        if (cmd === "get_audio_setting_ranges") return defaultRanges;
        if (cmd === "get_parametric_response") return args.frequencies.map(() => 0);
        if (cmd === "get_eq_preset_previews") {
          const freqs = args?.frequencies ?? [];
          return [
            ...presets.builtin.map((key) => ({ key, response_db: freqs.map(() => 0) })),
            ...presets.user.map((u) => ({ key: `user:${u.id}`, response_db: freqs.map(() => 0) })),
          ];
        }
        return null;
      });
    }

    async function renderPicker() {
      const view = render(Equalizer);
      let picker!: HTMLElement;
      await waitFor(() => {
        picker = view.getByRole("combobox", { name: "Preset" });
        expect(picker).toHaveTextContent("Studio");
      });
      return { ...view, picker };
    }

    /** Open the preset actions menu next to the picker and choose `name`. */
    async function choosePresetAction(name: string) {
      await fireEvent.click(screen.getByRole("button", { name: "Preset actions" }));
      await fireEvent.click(screen.getByRole("menuitem", { name }));
    }

    async function presetActionNames() {
      await fireEvent.click(screen.getByRole("button", { name: "Preset actions" }));
      const names = screen.getAllByRole("menuitem").map((item) => item.textContent?.trim());
      await fireEvent.keyDown(window, { key: "Escape" });
      return names;
    }

    it("lists user presets beside the built-ins and selects the active one", async () => {
      mockPresetBackend();
      const { picker, getByRole } = await renderPicker();
      await fireEvent.click(picker);
      const groups = [...getByRole("listbox").querySelectorAll('[role="group"]')].map((g) =>
        g.getAttribute("aria-label")
      );
      expect(groups).toEqual(["Built-in", "My presets"]);
      expect(picker).toHaveTextContent("Studio");
      await fireEvent.keyDown(window, { key: "Escape" });
      expect(await presetActionNames()).toEqual(["Import…", "Export…", "Save as…", "Rename", "Delete"]);
    });

    it("shows Custom once an edit's echo leaves the preset", async () => {
      mockPresetBackend();
      const { picker, getByLabelText } = await renderPicker();
      await fireEvent.change(getByLabelText("Gain 1"), { target: { value: "-6" } });
      await waitFor(() => expect(picker).toHaveTextContent("Custom"));
      expect(await presetActionNames()).toEqual(["Import…", "Export…", "Save as…"]);
    });

    it("exports to the picked file, named after the active preset", async () => {
      mockPresetBackend();
      await renderPicker();
      vi.mocked(save).mockResolvedValueOnce("C:/eq/Studio ParametricEQ.txt");
      await choosePresetAction("Export…");
      await waitFor(() =>
        expect(invoke).toHaveBeenCalledWith("export_parametric_profile", {
          path: "C:/eq/Studio ParametricEQ.txt",
        })
      );
      expect(vi.mocked(save).mock.calls[0][0]).toMatchObject({
        defaultPath: "Studio ParametricEQ.txt",
      });
    });

    it("writes nothing when the export dialog is cancelled", async () => {
      mockPresetBackend();
      await renderPicker();
      vi.mocked(save).mockResolvedValueOnce(null);
      await choosePresetAction("Export…");
      await waitFor(() => expect(save).toHaveBeenCalled());
      expect(invoke).not.toHaveBeenCalledWith("export_parametric_profile", expect.anything());
    });

    it("saves the current bands under a new name and lists it", async () => {
      mockPresetBackend({
        save_eq_user_preset: ({ name }) => {
          presets = { ...presets, user: [...presets.user, { id: 8, name }] };
          return { ...state, active_preset: "user:8" };
        },
      });
      const { picker, getByRole, getByLabelText } = await renderPicker();
      await choosePresetAction("Save as…");
      await fireEvent.input(getByLabelText("Preset name"), { target: { value: "Late night" } });
      await fireEvent.click(getByRole("button", { name: "Save" }));
      expect(invoke).toHaveBeenCalledWith("save_eq_user_preset", { name: "Late night" });
      await waitFor(() => expect(picker).toHaveTextContent("Late night"));
    });

    it("explains a rejected name next to the field", async () => {
      mockPresetBackend({
        save_eq_user_preset: () => {
          throw "duplicate_name";
        },
      });
      const { getByRole, getByLabelText } = await renderPicker();
      await choosePresetAction("Save as…");
      await fireEvent.input(getByLabelText("Preset name"), { target: { value: "studio" } });
      await fireEvent.click(getByRole("button", { name: "Save" }));
      await waitFor(() =>
        expect(getByRole("alert")).toHaveTextContent("A preset with this name already exists")
      );
      expect(getByLabelText("Preset name")).toHaveAttribute("aria-invalid", "true");
    });

    it("renames the active preset", async () => {
      mockPresetBackend({
        rename_eq_user_preset: ({ id, name }) => {
          presets = { ...presets, user: [{ id, name }] };
          return null;
        },
      });
      const { picker, getByRole, getByLabelText } = await renderPicker();
      await choosePresetAction("Rename");
      const field = getByLabelText("New name") as HTMLInputElement;
      expect(field.value).toBe("Studio");
      await fireEvent.input(field, { target: { value: "Studio monitors" } });
      await fireEvent.click(getByRole("button", { name: "Save" }));
      expect(invoke).toHaveBeenCalledWith("rename_eq_user_preset", { id: 7, name: "Studio monitors" });
      await waitFor(() => expect(picker).toHaveTextContent("Studio monitors"));
    });

    it("deletes the active preset only after confirming", async () => {
      mockPresetBackend({
        delete_eq_user_preset: () => {
          presets = { ...presets, user: [] };
          return { ...state, active_preset: null };
        },
      });
      const { picker, getByRole } = await renderPicker();
      await choosePresetAction("Delete");
      expect(invoke).not.toHaveBeenCalledWith("delete_eq_user_preset", expect.anything());
      await fireEvent.click(getByRole("button", { name: "Delete" }));
      expect(invoke).toHaveBeenCalledWith("delete_eq_user_preset", { id: 7 });
      await waitFor(() => expect(picker).toHaveTextContent("Custom"));
      await fireEvent.click(picker);
      expect(getByRole("listbox").querySelectorAll('[role="group"]')).toHaveLength(1);
    });

    describe("importing a profile (#1336)", () => {
      const profile = "Preamp: -3.78 dB\nFilter 1: ON PK Fc 100 Hz Gain 2 dB Q 1\n";
      const imported = {
        ...defaultEqConfig,
        mode: "parametric",
        preamp: -3.78,
        parametric: [{ kind: "peak", freq: 100, gain_db: 2, q: 1, enabled: true }],
        active_preset: "user:8",
      };

      function mockImport(importProfile: (args: any) => unknown) {
        mockPresetBackend({
          read_eq_profile_file: () => profile,
          import_parametric_profile: (args) => {
            const config = importProfile(args);
            presets = { ...presets, user: [...presets.user, { id: 8, name: args.name }] };
            return config;
          },
        });
      }

      it("imports a chosen file under its headphone name and selects it", async () => {
        mockImport(() => imported);
        vi.mocked(open).mockResolvedValue("C:\\Downloads\\Anker Soundcore Life Q20 ParametricEq.txt");
        const { picker, getByRole, getByLabelText, getByText, queryByLabelText } = await renderPicker();
        await choosePresetAction("Import…");
        await fireEvent.click(getByRole("button", { name: "Choose file…" }));
        await waitFor(() => expect(getByLabelText("Profile text")).toHaveValue(profile));
        expect(getByLabelText("Preset name")).toHaveValue("Anker Soundcore Life Q20");
        expect(invoke).toHaveBeenCalledWith("read_eq_profile_file", {
          path: "C:\\Downloads\\Anker Soundcore Life Q20 ParametricEq.txt",
        });

        await fireEvent.click(getByRole("button", { name: "Import" }));
        expect(invoke).toHaveBeenCalledWith("import_parametric_profile", {
          text: profile,
          name: "Anker Soundcore Life Q20",
        });
        await waitFor(() => expect(picker).toHaveTextContent("Anker Soundcore Life Q20"));
        expect(getByText("-3.78 dB")).toBeInTheDocument();
        expect(queryByLabelText("Profile text")).toBeNull();
      });

      it("explains a rejected profile and leaves the panel open", async () => {
        mockImport(() => {
          throw JSON.stringify({ code: "unsupported_filters", filters: [{ line: 2, kind: "LP" }] });
        });
        const { picker, getByRole, getByLabelText } = await renderPicker();
        await choosePresetAction("Import…");
        await fireEvent.input(getByLabelText("Profile text"), { target: { value: "Filter: ON LP Fc 80 Hz" } });
        await fireEvent.input(getByLabelText("Preset name"), { target: { value: "Broken" } });
        await fireEvent.click(getByRole("button", { name: "Import" }));
        await waitFor(() => expect(getByRole("alert")).toHaveTextContent("Unsupported filters: LP (line 2)"));
        expect(getByLabelText("Preset name")).toHaveAttribute("aria-invalid", "true");
        expect(picker).toHaveTextContent("Studio");
      });

      it("disables Import until there's both a profile and a name", async () => {
        mockImport(() => imported);
        const { getByRole, getByLabelText } = await renderPicker();
        await choosePresetAction("Import…");
        expect(getByRole("button", { name: "Import" })).toBeDisabled();
        await fireEvent.input(getByLabelText("Profile text"), { target: { value: profile } });
        expect(getByRole("button", { name: "Import" })).toBeDisabled();
        await fireEvent.input(getByLabelText("Preset name"), { target: { value: "Pasted" } });
        expect(getByRole("button", { name: "Import" })).toBeEnabled();
      });
    });

    it("shows an imported off-grid preamp exactly on the snapping slider", async () => {
      mockPresetBackend();
      state = { ...state, preamp: -3.78 };
      const { getByLabelText, getByText } = await renderPicker();
      expect(getByLabelText("Preamp:")).toHaveAttribute("step", "0.5");
      expect(getByText("-3.78 dB")).toBeInTheDocument();
      expect(invoke).not.toHaveBeenCalledWith("apply_equalizer_config", expect.anything());
    });

    it("hides user presets in graphic mode, which they can't describe", async () => {
      mockPresetBackend();
      state = { ...state, mode: "graphic10", active_preset: "Rock" };
      const { getByRole, queryByRole } = render(Equalizer);
      await waitFor(() => expect(getByRole("combobox", { name: "Preset" })).toHaveTextContent("Rock"));
      await fireEvent.click(getByRole("combobox", { name: "Preset" }));
      expect(getByRole("listbox").querySelectorAll('[role="group"]')).toHaveLength(1);
      expect(queryByRole("button", { name: "Preset actions" })).toBeNull();
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
      if (cmd === "list_eq_presets") return defaultPresets;
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
      if (cmd === "list_eq_presets") return defaultPresets;
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
    let selectEl: HTMLElement;
    await waitFor(() => {
      selectEl = getByRole("combobox");
      expect(selectEl).toBeInTheDocument();
    });

    await fireEvent.click(selectEl!);
    await fireEvent.click(getByRole("option", { name: /rock/i }));
    expect(invoke).toHaveBeenCalledWith("load_equalizer_preset", { presetName: "Rock" });
  });

  it("turns the EQ on in the engine when a preset is picked while it's off", async () => {
    let engine: Record<string, unknown> = { ...defaultEqConfig, enabled: false };
    vi.mocked(invoke).mockImplementation(async (cmd: string, args?: any) => {
      if (cmd === "get_equalizer_state") return engine;
      if (cmd === "list_eq_presets") return defaultPresets;
      if (cmd === "get_eq_preset_previews") return defaultPresets.builtin.map((key) => ({ key, response_db: [] }));
      if (cmd === "apply_equalizer_config") return (engine = { ...args.config, active_preset: null });
      if (cmd === "load_equalizer_preset") return (engine = { ...engine, active_preset: args.presetName });
      if (cmd === "get_loudness_settings") return defaultLoudness;
      if (cmd === "get_fade_settings") return defaultFadeSettings;
      if (cmd === "get_audio_setting_ranges") return defaultRanges;
      return null;
    });
    const { getByRole } = render(Equalizer);
    let selectEl!: HTMLElement;
    await waitFor(() => {
      selectEl = getByRole("combobox");
      expect(selectEl).toBeInTheDocument();
    });

    await fireEvent.click(selectEl);
    await fireEvent.click(getByRole("option", { name: /rock/i }));
    await waitFor(() => expect(selectEl).toHaveTextContent("Rock"));
    expect(engine.enabled).toBe(true);
    expect(getByRole("switch", { name: "Enable EQ" })).toBeChecked();
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
