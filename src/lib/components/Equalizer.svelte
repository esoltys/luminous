<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { onMount } from "svelte";
  import { i18n } from "../stores/i18n.svelte";
  import { loudnessStore } from "../stores/loudness.svelte";
  import {
    SlidersIcon as Sliders,
    PulseIcon as Activity,
    ArrowsLeftRightIcon as ArrowLeftRight
  } from "phosphor-svelte";
  import Toggle from "./Toggle.svelte";
  import Select from "./Select.svelte";
  import Knob from "./Knob.svelte";

  import type { EqConfig, EqMode, EqRanges, ParametricBand, SettingRange } from "../types/equalizer";

  // Matches PlayerBar's volume-slider gradient recipe so every horizontal
  // range input in the app shows the same accent-filled "active range" look.
  function rangeFillStyle(value: number, min: number, max: number): string {
    const pct = ((value - min) / (max - min)) * 100;
    return `background: linear-gradient(to right, var(--color-accent) 0%, var(--color-accent) ${pct}%, var(--color-border) ${pct}%, var(--color-border) 100%)`;
  }

  let enabled = $state(false);
  let mode = $state<EqMode>("graphic10");
  let preamp = $state(0.0);
  let gains = $state<number[]>(Array(10).fill(0.0));
  let parametric = $state<ParametricBand[]>([]);
  let selectedBand = $state(0);
  let activePreset = $state("Flat");

  const bandLabels = [
    "31.5 Hz", "63 Hz", "125 Hz", "250 Hz", "500 Hz",
    "1 kHz", "2 kHz", "4 kHz", "8 kHz", "16 kHz"
  ];

  const presets = [
    "Flat", "Rock", "Pop",
    "Bass Boost", "Vocal Boost", "Headphones"
  ];

  function getPresetTranslationKey(presetName: string): string {
    const keyMap: Record<string, string> = {
      "Flat": "flatPreset",
      "Pop": "popPreset",
      "Rock": "rockPreset",
      "Bass Boost": "bassBoostPreset",
      "Vocal Boost": "vocalBoostPreset",
      "Treble Boost": "trebleBoostPreset",
      "Headphones": "headphonesPreset"
    };
    return "equalizer." + (keyMap[presetName] || "customPreset");
  }

  async function loadConfig() {
    try {
      const config = await invoke<EqConfig>("get_equalizer_state");
      enabled = config.enabled;
      mode = config.mode ?? "graphic10";
      preamp = config.preamp;
      gains = config.gains;
      parametric = config.parametric ?? [];
      determinePresetName();
      await refreshCurve();
    } catch (e) {
      console.error("Failed to load equalizer state:", e);
    }
  }

  function determinePresetName() {
    const rockGains = [4.0, 3.0, 1.0, -1.0, -2.0, -1.0, 1.0, 3.0, 3.5, 3.5];
    const popGains = [1.5, 2.5, 1.0, -1.0, -0.5, 1.0, 2.5, 3.0, 2.5, 2.0];
    const bassBoostGains = [9.0, 7.0, 4.0, 1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0];
    const vocalBoostGains = [-3.0, -2.0, -1.0, 0.0, 2.0, 4.0, 4.5, 3.5, 1.0, -1.0];
    const headphonesGains = [2.0, 1.5, 0.5, 0.0, 0.0, 0.0, -0.5, -1.0, -0.5, 1.0];
    const flatGains = Array(10).fill(0.0);

    const matches = (a: number[], b: number[]) => a.every((v, i) => Math.abs(v - b[i]) < 0.1);

    if (matches(gains, flatGains)) activePreset = "Flat";
    else if (matches(gains, rockGains)) activePreset = "Rock";
    else if (matches(gains, popGains)) activePreset = "Pop";
    else if (matches(gains, bassBoostGains)) activePreset = "Bass Boost";
    else if (matches(gains, vocalBoostGains)) activePreset = "Vocal Boost";
    else if (matches(gains, headphonesGains)) activePreset = "Headphones";
    else activePreset = "Custom";
  }

  /** The single EQ mutation path: send the whole edited config; the engine
   * clamps and echoes the canonical state back. Local edits already updated
   * the reactive fields, so the echo only matters when clamping changed a
   * value. */
  async function applyConfig() {
    const canonical = await invoke<EqConfig>("apply_equalizer_config", {
      config: { enabled, mode, preamp, gains, parametric },
    });
    enabled = canonical.enabled;
    mode = canonical.mode;
    preamp = canonical.preamp;
    gains = canonical.gains;
    parametric = canonical.parametric;
    await refreshCurve();
  }

  async function ensureEnabled() {
    if (!enabled) enabled = true;
  }

  async function handleToggle() {
    await applyConfig();
  }

  async function handleModeChange(newMode: EqMode) {
    if (mode === newMode) return;
    mode = newMode;
    await applyConfig();
  }

  async function handlePreampChange() {
    await ensureEnabled();
    await applyConfig();
  }

  async function handleBandChange(index: number) {
    activePreset = "Custom";
    await ensureEnabled();
    await applyConfig();
  }

  async function pushParametricBand(index: number) {
    if (!parametric[index]) return;
    activePreset = "Custom";
    await ensureEnabled();
    await applyConfig();
  }

  async function resetParametric() {
    try {
      const config = await invoke<EqConfig>("reset_parametric_bands");
      parametric = config.parametric;
      await refreshCurve();
    } catch (e) {
      console.error("Failed to reset parametric bands:", e);
    }
  }

  async function selectPreset(preset: string) {
    if (preset === "Custom") return;
    try {
      await ensureEnabled();
      const config = await invoke<EqConfig>("load_equalizer_preset", { presetName: preset });
      gains = config.gains;
      parametric = config.parametric;
      activePreset = preset;
      await refreshCurve();
    } catch (e) {
      console.error("Failed to load preset:", e);
    }
  }

  // --- Log-frequency helpers (20 Hz – 20 kHz mapped to 0..1) ---
  const FREQ_MIN = 20;
  const FREQ_MAX = 20000;
  const FREQ_SPAN = Math.log(FREQ_MAX / FREQ_MIN);

  function unitToFreq(unit: number): number {
    return Math.round(FREQ_MIN * Math.exp(unit * FREQ_SPAN));
  }

  function formatFreq(freq: number): string {
    if (freq >= 10000) return `${(freq / 1000).toFixed(0)}k`;
    if (freq >= 1000) return `${(freq / 1000).toFixed(1).replace(/\.0$/, "")}k`;
    return `${Math.round(freq)}`;
  }

  // Smooth Catmull-Rom spline path for the SVG EQ envelope graphic.
  function splinePath(pts: { x: number; y: number }[]): string {
    if (pts.length === 0) return "";
    let d = `M ${pts[0].x} ${pts[0].y}`;
    for (let i = 0; i < pts.length - 1; i++) {
      const p0 = i > 0 ? pts[i - 1] : pts[i];
      const p1 = pts[i];
      const p2 = pts[i + 1];
      const p3 = i < pts.length - 2 ? pts[i + 2] : p2;
      const cp1x = p1.x + (p2.x - p0.x) / 6;
      const cp1y = p1.y + (p2.y - p0.y) / 6;
      const cp2x = p2.x - (p3.x - p1.x) / 6;
      const cp2y = p2.y - (p3.y - p1.y) / 6;
      d += ` C ${cp1x} ${cp1y}, ${cp2x} ${cp2y}, ${p2.x} ${p2.y}`;
    }
    return d;
  }

  // Parametric-only curve preview. Unlike the graphic bands (fixed Q), each
  // parametric band's Q changes its bandwidth — the gain sliders alone can't
  // show that, but the combined response curve can. The backend evaluates
  // the magnitude response of the filters it is actually running (shelves at
  // the edges, peaking bands between); this only plots it (#1248).
  const CURVE_SAMPLES = 96;
  const curveFreqs = Array.from(
    { length: CURVE_SAMPLES },
    (_, i) => FREQ_MIN * Math.exp((i / (CURVE_SAMPLES - 1)) * FREQ_SPAN)
  );
  let responseDb = $state<number[]>([]);
  let curveRequest = 0;

  /** Re-fetch the evaluated response after canonical state lands. Slider
   * drags fire many applies, so a response older than the latest request is
   * discarded. */
  async function refreshCurve() {
    const request = ++curveRequest;
    try {
      const db = await invoke<number[]>("get_parametric_response", { frequencies: curveFreqs });
      if (request === curveRequest) responseDb = db;
    } catch (e) {
      console.error("Failed to get parametric response:", e);
    }
  }

  let curvePath = $derived.by(() => {
    const gainRange = ranges?.eq.gain_db;
    if (!gainRange || parametric.length === 0 || responseDb.length !== CURVE_SAMPLES) return "";
    const pts = responseDb.map((db, i) => {
      const clamped = Math.max(gainRange.min, Math.min(gainRange.max, db));
      return { x: (i / (CURVE_SAMPLES - 1)) * 100, y: 20 - (clamped / gainRange.max) * 17 };
    });
    return splinePath(pts);
  });

  function verticalOrient(node: HTMLInputElement) {
    node.setAttribute("orient", "vertical");
  }

  // --- Loudness normalization (#77) ---
  type LoudnessMode = "track" | "album";
  interface LoudnessSettings {
    enabled: boolean;
    target_lufs: number;
    mode: LoudnessMode;
    fallback_gain_db: number;
  }

  // Bounds of the loudness/fade controls. The backend clamps to these and
  // owns them (#1249), so the controls wait for them rather than retyping them.
  interface AudioSettingRanges {
    target_lufs: SettingRange;
    fallback_gain_db: SettingRange;
    fade_pause_duration_ms: SettingRange;
    crossfade_auto_duration_secs: SettingRange;
    eq: EqRanges;
  }
  let ranges = $state<AudioSettingRanges | null>(null);

  async function loadSettingRanges() {
    try {
      ranges = await invoke<AudioSettingRanges>("get_audio_setting_ranges");
    } catch (e) {
      console.error("Failed to load audio setting ranges:", e);
    }
  }

  /** Tick values from `min` to `max` inclusive, `count` intervals apart. */
  function rangeTicks({ min, max }: SettingRange, count: number): number[] {
    return Array.from({ length: count + 1 }, (_, i) => min + ((max - min) * i) / count);
  }

  let targetLufs = $state(-16.0);
  let loudnessMode = $state<LoudnessMode>("track");
  let fallbackGainDb = $state(-6.0);

  async function loadLoudnessSettings() {
    try {
      const settings = await invoke<LoudnessSettings>("get_loudness_settings");
      loudnessStore.setEnabled(settings.enabled);
      targetLufs = settings.target_lufs;
      loudnessMode = settings.mode;
      fallbackGainDb = settings.fallback_gain_db;
    } catch (e) {
      console.error("Failed to load loudness settings:", e);
    }
  }

  async function saveLoudnessSettings() {
    try {
      await invoke("set_loudness_settings", {
        settings: {
          enabled: loudnessStore.enabled,
          target_lufs: targetLufs,
          mode: loudnessMode,
          fallback_gain_db: fallbackGainDb,
        },
      });
    } catch (e) {
      console.error("Failed to save loudness settings:", e);
    }
  }

  async function handleLoudnessToggle(enabled: boolean) {
    loudnessStore.setEnabled(enabled);
    await saveLoudnessSettings();
  }

  async function handleTargetLufsChange() {
    await saveLoudnessSettings();
  }

  async function handleLoudnessModeChange(newMode: LoudnessMode) {
    if (loudnessMode === newMode) return;
    loudnessMode = newMode;
    await saveLoudnessSettings();
  }

  async function handleFallbackGainChange() {
    await saveLoudnessSettings();
  }

  // --- Playback Fades & Crossfade (#79) ---
  interface FadeSettings {
    fade_pause_enabled: boolean;
    fade_pause_duration_ms: number;
    crossfade_auto_enabled: boolean;
    crossfade_auto_duration_secs: number;
    crossfade_suppress_same_album: boolean;
  }

  let fadePauseEnabled = $state(true);
  let fadePauseDurationMs = $state(300);
  let crossfadeAutoEnabled = $state(false);
  let crossfadeAutoDurationSecs = $state(3.0);
  let crossfadeSuppressSameAlbum = $state(true);

  async function loadFadeSettings() {
    try {
      const settings = await invoke<FadeSettings>("get_fade_settings");
      fadePauseEnabled = settings.fade_pause_enabled;
      fadePauseDurationMs = settings.fade_pause_duration_ms;
      crossfadeAutoEnabled = settings.crossfade_auto_enabled;
      crossfadeAutoDurationSecs = settings.crossfade_auto_duration_secs;
      crossfadeSuppressSameAlbum = settings.crossfade_suppress_same_album;
    } catch (e) {
      console.error("Failed to load fade settings:", e);
    }
  }

  async function saveFadeSettings() {
    try {
      await invoke("set_fade_settings", {
        settings: {
          fade_pause_enabled: fadePauseEnabled,
          fade_pause_duration_ms: fadePauseDurationMs,
          crossfade_auto_enabled: crossfadeAutoEnabled,
          crossfade_auto_duration_secs: crossfadeAutoDurationSecs,
          crossfade_suppress_same_album: crossfadeSuppressSameAlbum,
        },
      });
    } catch (e) {
      console.error("Failed to save fade settings:", e);
    }
  }

  onMount(async () => {
    loadConfig();
    loadSettingRanges();
    loadLoudnessSettings();
    loadFadeSettings();
    loudnessStore.init();
  });
</script>

<div class="flex flex-col gap-6 text-brand-text-primary">
  <div class="bg-brand-sidebar border border-brand-border rounded-xl p-6 flex flex-col gap-6">
    <div class="flex flex-col gap-4">
    <div class="flex items-start justify-between gap-4">
      <div class="flex items-center gap-3 min-w-0">
        <div class="p-2 rounded-xl bg-brand-accent/15 text-brand-accent-text shrink-0">
          <Sliders class="w-5 h-5" />
        </div>
        <div class="space-y-1 min-w-0">
          <h3 class="font-bold text-sm text-brand-text-primary">
            {mode === "parametric" ? i18n.t('equalizer.titleParametric') : i18n.t('equalizer.title')}
          </h3>
          <p class="text-xs text-brand-text-secondary leading-relaxed text-pretty">
            {mode === "parametric" ? i18n.t('equalizer.subtitleParametric') : i18n.t('equalizer.subtitle')}
          </p>
        </div>
      </div>
      
      <div class="flex items-center gap-2 shrink-0">
        <Toggle
          id="eq-toggle"
          checked={enabled}
          onchange={(v) => { enabled = v; handleToggle(); }}
          label={i18n.t('equalizer.enableEq')}
        />
      </div>
    </div>
    <div class="flex items-center gap-4 flex-wrap">

      <div class="relative flex items-center bg-brand-main border border-brand-border rounded-[2rem] p-0.5" role="group" aria-label={i18n.t('equalizer.modeLabel')}>
        <!-- Sliding background pill -->
        <span
          class="absolute top-0.5 bottom-0.5 left-0.5 w-[calc(50%-2px)] rounded-full bg-brand-accent shadow-sm pointer-events-none transition-transform duration-200 ease-out {mode === 'parametric' ? 'translate-x-full' : 'translate-x-0'}"
          aria-hidden="true"
        ></span>
        <button
          class="relative z-10 flex-1 whitespace-nowrap text-xs font-semibold px-4 py-1.5 rounded-full transition-colors duration-200 {mode === 'graphic10' ? 'text-brand-accent-contrast' : 'text-brand-text-secondary hover:text-brand-text-primary'}"
          onclick={() => handleModeChange("graphic10")}
          aria-pressed={mode === "graphic10"}
        >
          {i18n.t('equalizer.modeGraphic')}
        </button>
        <button
          class="relative z-10 flex-1 whitespace-nowrap text-xs font-semibold px-4 py-1.5 rounded-full transition-colors duration-200 {mode === 'parametric' ? 'text-brand-accent-contrast' : 'text-brand-text-secondary hover:text-brand-text-primary'}"
          onclick={() => handleModeChange("parametric")}
          aria-pressed={mode === "parametric"}
        >
          {i18n.t('equalizer.modeParametric')}
        </button>
      </div>

      <div class="flex items-center gap-2 bg-brand-main border border-brand-border rounded-[2rem] px-4 py-1.5">
        <span class="text-xs font-semibold text-brand-text-secondary">{i18n.t('equalizer.presetLabel')}:</span>
        <Select
          value={activePreset}
          onchange={(e) => { activePreset = e.currentTarget.value; selectPreset(activePreset); }}
          class="bg-brand-main text-xs text-brand-text-primary border border-brand-border rounded pl-3.5 pr-6 py-1 outline-none focus:border-brand-accent font-medium"
          chevronPosition="0.375rem"
        >
          {#each presets as preset}
            <option value={preset} class="bg-brand-main text-brand-text-primary">
              {i18n.t(getPresetTranslationKey(preset), {}, preset)}
            </option>
          {/each}
          {#if activePreset === "Custom"}
            <option value="Custom" class="bg-brand-main text-brand-text-primary" disabled>{i18n.t('equalizer.customPreset')}</option>
          {/if}
        </Select>
      </div>

      <div class="flex items-center gap-3 bg-brand-main border border-brand-border rounded-[2rem] px-4 py-1.5">
        <span class="text-xs font-semibold text-brand-text-secondary">{i18n.t('equalizer.preamp')}:</span>
        {#if ranges}
          <Knob
            min={ranges.eq.preamp.min}
            max={ranges.eq.preamp.max}
            step={0.25}
            bind:value={preamp}
            oninput={handlePreampChange}
            showValue={false}
            size={24}
          />
        {/if}
        <span class="text-xs font-mono font-medium {preamp > 0 ? 'text-green-400' : preamp < 0 ? 'text-red-400' : 'text-brand-text-primary'}">
          {preamp > 0 ? "+" : ""}{preamp.toFixed(1)} dB
        </span>
      </div>

      {#if mode === "parametric"}
        <button
          class="text-xs font-semibold px-4 py-1.5 bg-brand-main border border-brand-border rounded-full text-brand-text-secondary hover:text-brand-text-primary transition-colors"
          onclick={resetParametric}
        >
          {i18n.t('equalizer.resetBands')}
        </button>
      {/if}
    </div>
    </div>

    {#if mode === "parametric"}
      <!-- Response curve preview — parametric only, because Q (bandwidth)
           can't be read off the gain sliders but shapes the curve here.
           The curve is the backend's evaluated filter response. -->
      <div class="h-24 bg-brand-main border border-brand-border rounded-xl p-3 flex flex-col justify-between relative overflow-hidden">
        <div class="absolute left-0 right-0 top-1/2 border-t border-dashed border-brand-border pointer-events-none"></div>
        <svg class="w-full h-full" viewBox="0 0 100 40" preserveAspectRatio="none">
          {#if curvePath}
            <path
              d={curvePath}
              fill="none"
              stroke={enabled ? "url(#eqGrad)" : "var(--color-border)"}
              stroke-width="1.5"
              class="transition-all duration-200"
            />
          {/if}
          <defs>
            <linearGradient id="eqGrad" x1="0" y1="0" x2="1" y2="0">
              <stop offset="0%" stop-color="var(--color-accent)" />
              <stop offset="100%" stop-color="var(--color-accent-hover)" />
            </linearGradient>
          </defs>
        </svg>
        <div class="flex justify-between text-[8px] text-brand-text-secondary/40 px-1 font-mono uppercase">
          <span>{i18n.t('equalizer.bass')}</span>
          <span>{i18n.t('equalizer.mid')}</span>
          <span>{i18n.t('equalizer.treble')}</span>
        </div>
      </div>
    {/if}

    <!-- Slider bounds are the backend's clamp range (#1249), so wait for them. -->
    {#if ranges && mode === "graphic10"}
      <div class="grid grid-cols-5 md:grid-cols-10 gap-3 md:gap-5 min-h-64 h-auto md:h-72 items-center bg-brand-main/50 border border-brand-border/50 rounded-xl p-4 md:p-6">
        {#each gains as gain, idx}
          <div class="flex flex-col items-center justify-between h-full group">
            <span class="text-[10px] font-bold w-full text-center transition-colors {gain > 0 ? 'text-green-400/80' : gain < 0 ? 'text-red-400/80' : 'text-brand-text-secondary/70'}">
              {gain > 0 ? "+" : ""}{gain.toFixed(1)}
            </span>

            <div class="h-40 md:h-48 flex items-center justify-center relative">
              <input
                type="range"
                min={ranges.eq.gain_db.min}
                max={ranges.eq.gain_db.max}
                step="0.25"
                use:verticalOrient
                bind:value={gains[idx]}
                oninput={() => handleBandChange(idx)}
                class="accent-brand-accent cursor-ns-resize"
                style="appearance: slider-vertical; -webkit-appearance: slider-vertical; width: 12px; height: 100%;"
              />
            </div>

            <span class="text-[10px] md:text-[11px] font-medium text-brand-text-secondary text-center truncate w-full">
              {bandLabels[idx]}
            </span>
          </div>
        {/each}
      </div>
      <p class="text-xs text-brand-text-secondary px-1 -mt-2">
        {i18n.t('equalizer.isoStandard')}
      </p>
    {:else if ranges}
      <div class="grid grid-cols-10 md:grid-cols-[repeat(20,minmax(0,1fr))] gap-1 md:gap-1.5 min-h-64 h-auto md:h-72 items-center bg-brand-main/50 border border-brand-border/50 rounded-xl p-3 md:p-4">
        {#each parametric as band, idx}
          <div
            class="flex flex-col items-center justify-between h-full group rounded-md transition-colors {selectedBand === idx ? 'bg-brand-accent/10 ring-1 ring-brand-accent/50' : 'hover:bg-brand-sidebar/30'}"
            onclick={() => (selectedBand = idx)}
            role="button"
            tabindex="0"
            onkeydown={(e) => { if (e.key === "Enter" || e.key === " ") selectedBand = idx; }}
            aria-label={`${i18n.t('equalizer.bandLabel')} ${idx + 1}`}
          >
            <span class="text-[9px] font-bold w-full text-center transition-colors {band.gain_db > 0 ? 'text-green-400/80' : band.gain_db < 0 ? 'text-red-400/80' : 'text-brand-text-secondary/70'}">
              {band.gain_db > 0 ? "+" : ""}{band.gain_db.toFixed(1)}
            </span>

            <div class="h-40 md:h-48 flex items-center justify-center relative">
              <input
                type="range"
                min={ranges.eq.gain_db.min}
                max={ranges.eq.gain_db.max}
                step="0.25"
                use:verticalOrient
                bind:value={parametric[idx].gain_db}
                oninput={() => { selectedBand = idx; pushParametricBand(idx); }}
                class="accent-brand-accent cursor-ns-resize"
                style="appearance: slider-vertical; -webkit-appearance: slider-vertical; width: 10px; height: 100%;"
              />
            </div>

            <span class="text-[9px] font-medium text-center truncate w-full {selectedBand === idx ? 'text-brand-accent-text' : 'text-brand-text-secondary'}">
              {formatFreq(band.freq)}
            </span>
          </div>
        {/each}
      </div>

      <!-- Selected band detail: Q -->
      {#if parametric[selectedBand] && ranges}
        <div class="flex flex-col gap-2 bg-brand-sidebar border border-brand-border rounded-xl p-4">
          <div class="flex justify-between items-center text-xs font-bold text-brand-text-secondary">
            <span>
              {i18n.t('equalizer.bandLabel')} {selectedBand + 1}
              <span class="text-brand-text-secondary font-mono">· {formatFreq(parametric[selectedBand].freq)}Hz</span>
              — {i18n.t('equalizer.qFactor').toUpperCase()}
            </span>
            <span class="text-brand-accent-text font-mono">{parametric[selectedBand].q.toFixed(1)}</span>
          </div>
          <input
            type="range"
            min={ranges.eq.q.min}
            max={ranges.eq.q.max}
            step="0.1"
            bind:value={parametric[selectedBand].q}
            oninput={() => pushParametricBand(selectedBand)}
            class="w-full accent-brand-accent bg-brand-main h-1.5 rounded-lg appearance-none"
          />
        </div>
      {/if}
    {/if}
  </div>

    <!-- Loudness Normalization (#77) -->
    <div class="flex flex-col gap-6 bg-brand-sidebar border border-brand-border rounded-xl p-6">
      <div class="flex items-start justify-between gap-4 mb-2">
        <div class="flex items-center gap-3 min-w-0">
          <div class="p-2 rounded-xl bg-brand-accent/15 text-brand-accent-text shrink-0">
            <Activity class="w-5 h-5" />
          </div>
          <div class="space-y-1 min-w-0">
            <h3 class="font-bold text-sm text-brand-text-primary">{i18n.t('loudness.title')}</h3>
            <p class="text-xs text-brand-text-secondary leading-relaxed text-pretty">{i18n.t('loudness.subtitle')}</p>
          </div>
        </div>
        <div class="flex items-center gap-2 shrink-0">
          <Toggle
            checked={loudnessStore.enabled}
            onchange={(v) => handleLoudnessToggle(v)}
            label={i18n.t('loudness.title')}
          />
        </div>
      </div>

      <div class="grid grid-cols-1 md:grid-cols-3 gap-12">
        <div class="flex flex-col items-center justify-center gap-1.5 h-full">
          {#if ranges}
            <Knob
              min={ranges.target_lufs.min}
              max={ranges.target_lufs.max}
              step={0.25}
              bind:value={targetLufs}
              oninput={handleTargetLufsChange}
              disabled={!loudnessStore.enabled}
              label={i18n.t('loudness.targetLevel')}
              suffix="LUFS"
              size={80}
            />
          {/if}
        </div>

        <div class="flex flex-col items-center justify-center gap-1.5 h-full">
          <span class="text-[10px] font-bold text-brand-text-secondary uppercase tracking-wider text-center">{i18n.t('loudness.mode')}</span>
          <div class="relative flex items-center bg-brand-main border border-brand-border rounded-[2rem] p-0.5 mt-1 mx-auto w-full max-w-[200px]" role="group" aria-label={i18n.t('loudness.mode')}>
            <!-- Sliding background pill -->
            <span
              class="absolute top-0.5 bottom-0.5 left-0.5 w-[calc(50%-2px)] rounded-full bg-brand-accent shadow-sm pointer-events-none transition-transform duration-200 ease-out {loudnessMode === 'album' ? 'translate-x-full' : 'translate-x-0'} {!loudnessStore.enabled ? 'opacity-50' : ''}"
              aria-hidden="true"
            ></span>
            <button
              class="relative z-10 flex-1 text-xs font-semibold px-4 py-1.5 rounded-full transition-colors duration-200 {loudnessMode === 'track' ? 'text-brand-accent-contrast' : 'text-brand-text-secondary hover:text-brand-text-primary'}"
              onclick={() => handleLoudnessModeChange("track")}
              aria-pressed={loudnessMode === "track"}
              disabled={!loudnessStore.enabled}
            >
              {i18n.t('loudness.modeTrack')}
            </button>
            <button
              class="relative z-10 flex-1 text-xs font-semibold px-4 py-1.5 rounded-full transition-colors duration-200 {loudnessMode === 'album' ? 'text-brand-accent-contrast' : 'text-brand-text-secondary hover:text-brand-text-primary'}"
              onclick={() => handleLoudnessModeChange("album")}
              aria-pressed={loudnessMode === "album"}
              disabled={!loudnessStore.enabled}
            >
              {i18n.t('loudness.modeAlbum')}
            </button>
          </div>
        </div>

        <div class="flex flex-col items-center justify-center gap-1.5 h-full">
          {#if ranges}
            <Knob
              min={ranges.fallback_gain_db.min}
              max={ranges.fallback_gain_db.max}
              step={0.25}
              bind:value={fallbackGainDb}
              oninput={handleFallbackGainChange}
              disabled={!loudnessStore.enabled}
              label={i18n.t('loudness.fallbackGain')}
              suffix="dB"
              size={80}
            />
          {/if}
          <span class="text-[11px] text-brand-text-secondary text-center mt-2 px-4 text-pretty">{i18n.t('loudness.fallbackGainHint')}</span>
        </div>
      </div>

      <p class="text-xs text-brand-text-secondary border-t border-brand-border/60 pt-2">
        {#if loudnessStore.analysisRemaining === 0}
          {i18n.t('loudness.analyzed')}
        {:else if loudnessStore.enabled}
          {i18n.t('loudness.analyzing', { remaining: loudnessStore.analysisRemaining })}
        {:else}
          {i18n.t('loudness.analysisPaused', { remaining: loudnessStore.analysisRemaining })}
        {/if}
      </p>
    </div>

    <!-- Playback Fades & Crossfade (#79) -->
    <div class="flex flex-col gap-6 bg-brand-sidebar border border-brand-border rounded-xl p-6">
      <div class="flex items-center gap-3 mb-2">
        <div class="p-2 rounded-xl bg-brand-accent/15 text-brand-accent-text shrink-0">
          <ArrowLeftRight class="w-5 h-5" />
        </div>
        <div class="space-y-1 min-w-0">
          <h3 class="font-bold text-sm text-brand-text-primary">{i18n.t('fades.title')}</h3>
          <p class="text-xs text-brand-text-secondary leading-relaxed text-pretty">{i18n.t('fades.subtitle')}</p>
        </div>
      </div>

      <div class="flex flex-col gap-1.5 pt-1">
        <div class="flex items-center justify-between mb-4">
          <span class="text-sm font-bold text-brand-text-primary">{i18n.t('fades.fadePause')}</span>
          <Toggle
            checked={fadePauseEnabled}
            onchange={(v) => { fadePauseEnabled = v; saveFadeSettings(); }}
            label={i18n.t('fades.fadePause')}
          />
        </div>
        {#if fadePauseEnabled && ranges}
          {@const fadeRange = ranges.fade_pause_duration_ms}
          <div class="flex items-center justify-between text-xs text-brand-text-secondary">
            <span>{i18n.t('fades.fadeDuration')}</span>
            <span class="font-mono font-bold text-brand-text-primary">{fadePauseDurationMs}ms</span>
          </div>
          <input
            type="range"
            min={fadeRange.min}
            max={fadeRange.max}
            step="100"
            bind:value={fadePauseDurationMs}
            onchange={saveFadeSettings}
            class="themed-range w-full h-1.5 rounded-lg"
            style={rangeFillStyle(fadePauseDurationMs, fadeRange.min, fadeRange.max)}
          />
          <div class="px-[7px]">
            <div class="relative w-full h-4 text-[9px] text-brand-text-secondary/60 font-medium mt-0.5">
              {#each rangeTicks(fadeRange, 10) as val}
                <div class="absolute top-0 flex flex-col items-center -translate-x-1/2" style="left: {((val - fadeRange.min) / (fadeRange.max - fadeRange.min)) * 100}%">
                  <div class="h-1 w-[1px] bg-brand-border mb-0.5"></div>
                  <span>{val}</span>
                </div>
              {/each}
            </div>
          </div>
        {/if}
      </div>

      <div class="flex flex-col gap-1.5 border-t border-brand-border pt-6">
        <div class="flex items-center justify-between mb-4">
          <span class="text-sm font-bold text-brand-text-primary">{i18n.t('fades.crossfadeAuto')}</span>
          <Toggle
            checked={crossfadeAutoEnabled}
            onchange={(v) => { crossfadeAutoEnabled = v; saveFadeSettings(); }}
            label={i18n.t('fades.crossfadeAuto')}
          />
        </div>
        {#if crossfadeAutoEnabled && ranges}
          {@const crossfadeRange = ranges.crossfade_auto_duration_secs}
          <div class="flex items-center justify-between text-xs text-brand-text-secondary">
            <span>{i18n.t('fades.crossfadeDuration')}</span>
            <span class="font-mono font-bold text-brand-text-primary">{crossfadeAutoDurationSecs.toFixed(1)}s</span>
          </div>
          <input
            type="range"
            min={crossfadeRange.min}
            max={crossfadeRange.max}
            step="0.25"
            bind:value={crossfadeAutoDurationSecs}
            onchange={saveFadeSettings}
            class="themed-range w-full h-1.5 rounded-lg"
            style={rangeFillStyle(crossfadeAutoDurationSecs, crossfadeRange.min, crossfadeRange.max)}
          />
          <div class="px-[7px]">
            <div class="relative w-full h-4 text-[9px] text-brand-text-secondary/60 font-medium mt-0.5">
              {#each rangeTicks(crossfadeRange, crossfadeRange.max - crossfadeRange.min) as val}
                <div class="absolute top-0 flex flex-col items-center -translate-x-1/2" style="left: {((val - crossfadeRange.min) / (crossfadeRange.max - crossfadeRange.min)) * 100}%">
                  <div class="h-1 w-[1px] bg-brand-border mb-0.5"></div>
                  <span>{val.toFixed(1)}s</span>
                </div>
              {/each}
            </div>
          </div>
          <div class="flex items-center justify-between gap-2 pt-4 md:w-1/2 text-xs text-brand-text-secondary">
            <span>{i18n.t('fades.suppressSameAlbum')}</span>
            <Toggle
              checked={crossfadeSuppressSameAlbum}
              onchange={(v) => { crossfadeSuppressSameAlbum = v; saveFadeSettings(); }}
              label={i18n.t('fades.suppressSameAlbum')}
            />
          </div>
        {/if}
      </div>
    </div>
</div>
