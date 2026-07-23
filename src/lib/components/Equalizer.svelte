<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { listen } from "@tauri-apps/api/event";
  import { onMount, onDestroy } from "svelte";
  import { i18n } from "../stores/i18n.svelte";

  type EqMode = "graphic10" | "parametric20";
  interface ParametricBand {
    freq: number;
    gain_db: number;
    q: number;
  }
  interface EqConfig {
    enabled: boolean;
    mode: EqMode;
    preamp: number;
    gains: number[];
    parametric: ParametricBand[];
  }

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
    "Flat", "Rock", "Pop", "Classical", "Jazz",
    "Bass Boost", "Vocal Boost"
  ];

  async function loadConfig() {
    try {
      const config = await invoke<EqConfig>("get_equalizer_state");
      enabled = config.enabled;
      mode = config.mode ?? "graphic10";
      preamp = config.preamp;
      gains = config.gains;
      parametric = config.parametric ?? [];
      determinePresetName();
    } catch (e) {
      console.error("Failed to load equalizer state:", e);
    }
  }

  function determinePresetName() {
    const rockGains = [4.0, 3.0, 2.0, -1.0, -2.0, -1.0, 1.0, 2.0, 3.0, 4.0];
    const popGains = [-2.0, -1.0, 0.0, 2.0, 4.0, 4.0, 2.0, 0.0, -1.0, -2.0];
    const classicalGains = [5.0, 3.0, 2.0, 2.0, -1.0, -1.0, 0.0, 2.0, 3.0, 4.0];
    const jazzGains = [3.0, 2.0, 1.0, 2.0, -1.0, -1.0, 0.0, 1.0, 2.0, 3.0];
    const bassBoostGains = [6.0, 5.0, 4.0, 2.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0];
    const vocalBoostGains = [-2.0, -2.0, -1.0, 1.0, 3.0, 4.0, 3.0, 1.0, -1.0, -2.0];
    const flatGains = Array(10).fill(0.0);

    const matches = (a: number[], b: number[]) => a.every((v, i) => Math.abs(v - b[i]) < 0.1);

    if (matches(gains, flatGains)) activePreset = "Flat";
    else if (matches(gains, rockGains)) activePreset = "Rock";
    else if (matches(gains, popGains)) activePreset = "Pop";
    else if (matches(gains, classicalGains)) activePreset = "Classical";
    else if (matches(gains, jazzGains)) activePreset = "Jazz";
    else if (matches(gains, bassBoostGains)) activePreset = "Bass Boost";
    else if (matches(gains, vocalBoostGains)) activePreset = "Vocal Boost";
    else activePreset = "Custom";
  }

  async function ensureEnabled() {
    if (!enabled) {
      enabled = true;
      await invoke("set_equalizer_enabled", { enabled: true });
    }
  }

  async function handleToggle() {
    await invoke("set_equalizer_enabled", { enabled });
  }

  async function handleModeChange(newMode: EqMode) {
    if (mode === newMode) return;
    mode = newMode;
    try {
      await invoke("set_equalizer_mode", { mode: newMode });
    } catch (e) {
      console.error("Failed to set equalizer mode:", e);
    }
  }

  async function handlePreampChange() {
    await ensureEnabled();
    await invoke("set_equalizer_preamp", { preampDb: preamp });
  }

  async function handleBandChange(index: number) {
    activePreset = "Custom";
    await ensureEnabled();
    await invoke("set_equalizer_band", { bandIdx: index, gainDb: gains[index] });
  }

  async function pushParametricBand(index: number) {
    const band = parametric[index];
    if (!band) return;
    activePreset = "Custom";
    await ensureEnabled();
    // Band center frequencies are fixed — only gain and Q are adjustable.
    await invoke("set_parametric_band", {
      bandIdx: index,
      gainDb: band.gain_db,
      q: band.q
    });
  }

  async function resetParametric() {
    try {
      const config = await invoke<EqConfig>("reset_parametric_bands");
      parametric = config.parametric;
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
  // show that, but the combined response curve can. We approximate the
  // response by summing each band's peaking-filter gain (in dB) across a log
  // frequency sweep, so widening Q visibly broadens the bump.
  function bandGainDb(band: ParametricBand, freq: number): number {
    if (Math.abs(band.gain_db) < 0.01) return 0;
    // Bell shape in log-frequency: full gain at center, falling off over a
    // width set by Q (higher Q = narrower).
    const octaves = Math.log2(freq / band.freq);
    const bandwidth = 1 / Math.max(band.q, 0.1); // ~octaves to half-gain
    const falloff = Math.exp(-((octaves / bandwidth) ** 2));
    return band.gain_db * falloff;
  }

  let curvePath = $derived.by(() => {
    if (parametric.length === 0) return "";
    const SAMPLES = 96;
    const pts = Array.from({ length: SAMPLES }, (_, i) => {
      const unit = i / (SAMPLES - 1);
      const freq = unitToFreq(unit);
      const total = parametric.reduce((sum, b) => sum + bandGainDb(b, freq), 0);
      const clamped = Math.max(-12, Math.min(12, total));
      return { x: unit * 100, y: 20 - (clamped / 12.0) * 17 };
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

  let loudnessEnabled = $state(false);
  let targetLufs = $state(-18.0);
  let loudnessMode = $state<LoudnessMode>("track");
  let fallbackGainDb = $state(-6.0);
  let analysisRemaining = $state(0);
  let unlistenAnalysis: (() => void) | undefined;

  async function loadLoudnessSettings() {
    try {
      const settings = await invoke<LoudnessSettings>("get_loudness_settings");
      loudnessEnabled = settings.enabled;
      targetLufs = settings.target_lufs;
      loudnessMode = settings.mode;
      fallbackGainDb = settings.fallback_gain_db;
    } catch (e) {
      console.error("Failed to load loudness settings:", e);
    }
  }

  async function loadAnalysisRemaining() {
    try {
      analysisRemaining = await invoke<number>("get_loudness_analysis_remaining");
    } catch (e) {
      console.error("Failed to load loudness analysis progress:", e);
    }
  }

  async function handleLoudnessToggle() {
    await invoke("set_loudness_enabled", { enabled: loudnessEnabled });
  }

  async function handleTargetLufsChange() {
    await invoke("set_loudness_target_lufs", { targetLufs });
  }

  async function handleLoudnessModeChange(newMode: LoudnessMode) {
    if (loudnessMode === newMode) return;
    loudnessMode = newMode;
    await invoke("set_loudness_mode", { mode: newMode });
  }

  async function handleFallbackGainChange() {
    await invoke("set_loudness_fallback_gain", { fallbackGainDb });
  }

  // --- Playback Fades & Crossfade (#79) ---
  interface FadeSettings {
    fade_pause_enabled: boolean;
    fade_pause_duration_ms: number;
    crossfade_manual_enabled: boolean;
    crossfade_manual_duration_ms: number;
    crossfade_auto_enabled: boolean;
    crossfade_auto_duration_secs: number;
    crossfade_suppress_same_album: boolean;
  }

  let fadePauseEnabled = $state(true);
  let fadePauseDurationMs = $state(300);
  let crossfadeManualEnabled = $state(true);
  let crossfadeManualDurationMs = $state(1000);
  let crossfadeAutoEnabled = $state(false);
  let crossfadeAutoDurationSecs = $state(3.0);
  let crossfadeSuppressSameAlbum = $state(true);

  async function loadFadeSettings() {
    try {
      const settings = await invoke<FadeSettings>("get_fade_settings");
      fadePauseEnabled = settings.fade_pause_enabled;
      fadePauseDurationMs = settings.fade_pause_duration_ms;
      crossfadeManualEnabled = settings.crossfade_manual_enabled;
      crossfadeManualDurationMs = settings.crossfade_manual_duration_ms;
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
          crossfade_manual_enabled: crossfadeManualEnabled,
          crossfade_manual_duration_ms: crossfadeManualDurationMs,
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
    loadLoudnessSettings();
    loadFadeSettings();
    loadAnalysisRemaining();
    unlistenAnalysis = await listen<{ remaining: number }>(
      "loudness-analysis-progress",
      (event) => {
        analysisRemaining = event.payload.remaining;
      }
    );
  });

  onDestroy(() => {
    unlistenAnalysis?.();
  });
</script>

<div class="flex flex-col text-brand-text-primary">
  <div class="flex items-center justify-between mb-6 pb-4 border-b border-brand-border gap-4 flex-wrap">
    <div>
      <h3 class="text-sm font-bold text-brand-text-primary">
        {mode === "parametric20" ? i18n.t('equalizer.titleParametric') : i18n.t('equalizer.title')}
      </h3>
      <p class="text-xs text-brand-text-secondary/70 mt-0.5">
        {mode === "parametric20" ? i18n.t('equalizer.subtitleParametric') : i18n.t('equalizer.subtitle')}
      </p>
    </div>

    <!-- Toggle, Mode & Presets controls -->
    <div class="flex items-center gap-3 flex-wrap">
      <div class="flex items-center gap-3 bg-brand-sidebar/40 border border-brand-border rounded-lg px-4 py-2">
        <label for="eq-toggle" class="text-xs font-semibold text-brand-text-secondary">{i18n.t('equalizer.enableEq')}</label>
        <input
          id="eq-toggle"
          type="checkbox"
          bind:checked={enabled}
          onchange={handleToggle}
          class="w-4 h-4 shrink-0 text-brand-accent-text bg-brand-main border-brand-border rounded focus:ring-brand-accent accent-brand-accent cursor-pointer"
        />
      </div>

      <!-- Mode segmented control -->
      <div class="flex items-center bg-brand-sidebar/40 border border-brand-border rounded-lg p-1" role="group" aria-label={i18n.t('equalizer.modeLabel')}>
        <button
          class="text-xs font-semibold px-3 py-1.5 rounded-md transition-colors cursor-pointer {mode === 'graphic10' ? 'bg-brand-accent text-brand-accent-contrast' : 'text-brand-text-secondary hover:text-brand-text-primary'}"
          onclick={() => handleModeChange("graphic10")}
          aria-pressed={mode === "graphic10"}
        >
          {i18n.t('equalizer.modeGraphic')}
        </button>
        <button
          class="text-xs font-semibold px-3 py-1.5 rounded-md transition-colors cursor-pointer {mode === 'parametric20' ? 'bg-brand-accent text-brand-accent-contrast' : 'text-brand-text-secondary hover:text-brand-text-primary'}"
          onclick={() => handleModeChange("parametric20")}
          aria-pressed={mode === "parametric20"}
        >
          {i18n.t('equalizer.modeParametric')}
        </button>
      </div>

      <div class="flex items-center gap-2 bg-brand-sidebar/40 border border-brand-border rounded-lg px-3 py-1.5">
        <span class="text-xs font-semibold text-brand-text-secondary">{i18n.t('equalizer.presetLabel')}:</span>
        <select
          bind:value={activePreset}
          onchange={() => selectPreset(activePreset)}
          class="bg-brand-main text-xs text-brand-text-primary border border-brand-border rounded px-2.5 py-1 pr-6 outline-none cursor-pointer focus:border-brand-accent font-medium appearance-none -webkit-appearance-none"
          style="background-image: url(&quot;data:image/svg+xml;charset=utf-8,%3Csvg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 20 20' fill='none'%3E%3Cpath stroke='%239ca3af' stroke-linecap='round' stroke-linejoin='round' stroke-width='1.5' d='m6 8 4 4 4-4'/%3E%3C/svg%3E&quot;); background-position: right 0.375rem center; background-repeat: no-repeat; background-size: 1.25em;"
        >
          {#each presets as preset}
            <option value={preset} class="bg-brand-main text-brand-text-primary">
              {i18n.t('equalizer.' + preset.toLowerCase().replace(' ', '') + 'Preset', {}, preset)}
            </option>
          {/each}
          {#if activePreset === "Custom"}
            <option value="Custom" class="bg-brand-main text-brand-text-primary" disabled>{i18n.t('equalizer.customPreset')}</option>
          {/if}
        </select>
      </div>

      {#if mode === "parametric20"}
        <button
          class="text-xs font-semibold px-3 py-2 bg-brand-sidebar/40 border border-brand-border rounded-lg text-brand-text-secondary hover:text-brand-text-primary transition-colors cursor-pointer"
          onclick={resetParametric}
        >
          {i18n.t('equalizer.resetBands')}
        </button>
      {/if}
    </div>
  </div>

  <!-- Rack Container -->
  <div class="bg-brand-sidebar/20 border border-brand-border rounded-2xl p-6 md:p-8 flex flex-col gap-8 shadow-xl shadow-black/30">
    <!-- Preamp -->
    <div class="flex flex-col gap-2 bg-brand-sidebar/40 border border-brand-border rounded-xl p-4">
      <div class="flex justify-between items-center text-xs font-bold text-brand-text-secondary">
        <span>{i18n.t('equalizer.preamp').toUpperCase()}</span>
        <span class={preamp > 0 ? "text-green-400" : preamp < 0 ? "text-red-400" : "text-brand-text-secondary/50"}>
          {preamp > 0 ? "+" : ""}{preamp.toFixed(1)} dB
        </span>
      </div>
      <input
        type="range"
        min="-12.0"
        max="12.0"
        step="0.5"
        bind:value={preamp}
        oninput={handlePreampChange}
        class="themed-range w-full h-1.5 rounded-lg cursor-pointer"
        style={rangeFillStyle(preamp, -12.0, 12.0)}
      />
    </div>

    <!-- Loudness Normalization (#77) -->
    <div class="flex flex-col gap-3 bg-brand-sidebar/40 border border-brand-border rounded-xl p-4">
      <div class="flex items-center justify-between gap-4 flex-wrap">
        <div>
          <h4 class="text-xs font-bold text-brand-text-primary">{i18n.t('loudness.title')}</h4>
          <p class="text-[11px] text-brand-text-secondary/70 mt-0.5">{i18n.t('loudness.subtitle')}</p>
        </div>
        <label class="flex items-center gap-2 shrink-0">
          <span class="text-xs font-semibold text-brand-text-secondary">{i18n.t('loudness.enable')}</span>
          <input
            type="checkbox"
            bind:checked={loudnessEnabled}
            onchange={handleLoudnessToggle}
            class="w-4 h-4 shrink-0 text-brand-accent-text bg-brand-main border-brand-border rounded focus:ring-brand-accent accent-brand-accent cursor-pointer"
          />
        </label>
      </div>

      <div class="grid grid-cols-1 md:grid-cols-3 gap-4">
        <div class="flex flex-col gap-1.5">
          <div class="flex justify-between items-center text-xs font-bold text-brand-text-secondary">
            <span>{i18n.t('loudness.targetLevel')}</span>
            <span class="text-brand-accent-text font-mono">{targetLufs.toFixed(1)} LUFS</span>
          </div>
          <input
            type="range"
            min="-24.0"
            max="-14.0"
            step="0.5"
            bind:value={targetLufs}
            oninput={handleTargetLufsChange}
            disabled={!loudnessEnabled}
            class="themed-range w-full h-1.5 rounded-lg cursor-pointer disabled:opacity-40 disabled:cursor-not-allowed"
            style={rangeFillStyle(targetLufs, -24.0, -14.0)}
          />
        </div>

        <div class="flex flex-col gap-1.5">
          <span class="text-xs font-bold text-brand-text-secondary">{i18n.t('loudness.mode')}</span>
          <div class="flex items-center bg-brand-main border border-brand-border rounded-lg p-1" role="group" aria-label={i18n.t('loudness.mode')}>
            <button
              class="flex-1 text-xs font-semibold px-3 py-1 rounded-md transition-colors cursor-pointer {loudnessMode === 'track' ? 'bg-brand-accent text-brand-accent-contrast' : 'text-brand-text-secondary hover:text-brand-text-primary'}"
              onclick={() => handleLoudnessModeChange("track")}
              aria-pressed={loudnessMode === "track"}
              disabled={!loudnessEnabled}
            >
              {i18n.t('loudness.modeTrack')}
            </button>
            <button
              class="flex-1 text-xs font-semibold px-3 py-1 rounded-md transition-colors cursor-pointer {loudnessMode === 'album' ? 'bg-brand-accent text-brand-accent-contrast' : 'text-brand-text-secondary hover:text-brand-text-primary'}"
              onclick={() => handleLoudnessModeChange("album")}
              aria-pressed={loudnessMode === "album"}
              disabled={!loudnessEnabled}
            >
              {i18n.t('loudness.modeAlbum')}
            </button>
          </div>
        </div>

        <div class="flex flex-col gap-1.5">
          <div class="flex justify-between items-center text-xs font-bold text-brand-text-secondary">
            <span>{i18n.t('loudness.fallbackGain')}</span>
            <span class="text-brand-accent-text font-mono">{fallbackGainDb.toFixed(1)} dB</span>
          </div>
          <input
            type="range"
            min="-24.0"
            max="0.0"
            step="0.5"
            bind:value={fallbackGainDb}
            oninput={handleFallbackGainChange}
            disabled={!loudnessEnabled}
            class="themed-range w-full h-1.5 rounded-lg cursor-pointer disabled:opacity-40 disabled:cursor-not-allowed"
            style={rangeFillStyle(fallbackGainDb, -24.0, 0.0)}
          />
          <span class="text-[10px] text-brand-text-secondary/60">{i18n.t('loudness.fallbackGainHint')}</span>
        </div>
      </div>

      <p class="text-[11px] text-brand-text-secondary/70 border-t border-brand-border/60 pt-2">
        {analysisRemaining > 0
          ? i18n.t('loudness.analyzing', { remaining: analysisRemaining })
          : i18n.t('loudness.analyzed')}
      </p>
    </div>

    <!-- Playback Fades & Crossfade (#79) -->
    <div class="bg-brand-sidebar/40 border border-brand-border rounded-xl p-4 flex flex-col gap-3">
      <div>
        <h4 class="text-xs font-bold text-brand-text-primary">{i18n.t('fades.title')}</h4>
        <p class="text-[11px] text-brand-text-secondary/70 mt-0.5">{i18n.t('fades.subtitle')}</p>
      </div>

      <!-- Fade on Pause/Stop -->
      <div class="flex flex-col gap-1.5 pt-1">
        <div class="flex items-center justify-between">
          <span class="text-xs font-semibold text-brand-text-secondary">{i18n.t('fades.fadePause')}</span>
          <input
            type="checkbox"
            bind:checked={fadePauseEnabled}
            onchange={saveFadeSettings}
            class="toggle-checkbox accent-brand-accent cursor-pointer"
          />
        </div>
        {#if fadePauseEnabled}
          <div class="flex items-center justify-between text-xs text-brand-text-secondary">
            <span>{i18n.t('fades.fadeDuration')}</span>
            <span class="font-mono text-brand-accent-text">{fadePauseDurationMs}ms</span>
          </div>
          <input
            type="range"
            min="100"
            max="1000"
            step="50"
            bind:value={fadePauseDurationMs}
            onchange={saveFadeSettings}
            class="themed-range w-full h-1.5 rounded-lg cursor-pointer"
            style={rangeFillStyle(fadePauseDurationMs, 100, 1000)}
          />
        {/if}
      </div>

      <!-- Automatic Crossfade -->
      <div class="flex flex-col gap-1.5 border-t border-brand-border/40 pt-2">
        <div class="flex items-center justify-between">
          <span class="text-xs font-semibold text-brand-text-secondary">{i18n.t('fades.crossfadeAuto')}</span>
          <input
            type="checkbox"
            bind:checked={crossfadeAutoEnabled}
            onchange={saveFadeSettings}
            class="toggle-checkbox accent-brand-accent cursor-pointer"
          />
        </div>
        {#if crossfadeAutoEnabled}
          <div class="flex items-center justify-between text-xs text-brand-text-secondary">
            <span>{i18n.t('fades.crossfadeDuration')}</span>
            <span class="font-mono text-brand-accent-text">{crossfadeAutoDurationSecs.toFixed(1)}s</span>
          </div>
          <input
            type="range"
            min="0.5"
            max="10.0"
            step="0.5"
            bind:value={crossfadeAutoDurationSecs}
            onchange={saveFadeSettings}
            class="themed-range w-full h-1.5 rounded-lg cursor-pointer"
            style={rangeFillStyle(crossfadeAutoDurationSecs, 0.5, 10.0)}
          />
          <div class="flex items-center justify-between pt-1">
            <span class="text-[11px] text-brand-text-secondary/80">{i18n.t('fades.suppressSameAlbum')}</span>
            <input
              type="checkbox"
              bind:checked={crossfadeSuppressSameAlbum}
              onchange={saveFadeSettings}
              class="accent-brand-accent cursor-pointer"
            />
          </div>
        {/if}
      </div>
    </div>

    {#if mode === "parametric20"}
      <!-- Response curve preview — parametric only, because Q (bandwidth)
           can't be read off the gain sliders but shapes the curve here. -->
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

    {#if mode === "graphic10"}
      <!-- Graphic Sliders Rack -->
      <div class="grid grid-cols-5 md:grid-cols-10 gap-3 md:gap-5 h-64 md:h-72 items-center bg-brand-main/40 border border-brand-border/60 rounded-xl p-4 md:p-6">
        {#each gains as gain, idx}
          <div class="flex flex-col items-center justify-between h-full group">
            <!-- Gain display -->
            <span class="text-[9px] font-mono font-bold w-full text-center transition-colors group-hover:text-brand-accent-text {gain > 0 ? 'text-green-400' : gain < 0 ? 'text-red-400' : 'text-brand-text-secondary/50'}">
              {gain > 0 ? "+" : ""}{gain.toFixed(1)}
            </span>

            <!-- Slider track -->
            <div class="h-40 md:h-48 flex items-center justify-center relative">
              <input
                type="range"
                min="-12.0"
                max="12.0"
                step="0.5"
                use:verticalOrient
                bind:value={gains[idx]}
                oninput={() => handleBandChange(idx)}
                class="accent-brand-accent cursor-ns-resize"
                style="appearance: slider-vertical; -webkit-appearance: slider-vertical; width: 12px; height: 100%;"
              />
            </div>

            <!-- Label -->
            <span class="text-[9px] md:text-[10px] font-medium text-brand-text-secondary/70 font-mono text-center tracking-tighter truncate w-full">
              {bandLabels[idx]}
            </span>
          </div>
        {/each}
      </div>
    {:else}
      <!-- Parametric Sliders Rack -->
      <div class="grid grid-cols-10 md:grid-cols-[repeat(20,minmax(0,1fr))] gap-1 md:gap-1.5 h-64 md:h-72 items-center bg-brand-main/40 border border-brand-border/60 rounded-xl p-3 md:p-4">
        {#each parametric as band, idx}
          <div
            class="flex flex-col items-center justify-between h-full group rounded-md transition-colors cursor-pointer {selectedBand === idx ? 'bg-brand-accent/10 ring-1 ring-brand-accent/50' : 'hover:bg-brand-sidebar/30'}"
            onclick={() => (selectedBand = idx)}
            role="button"
            tabindex="0"
            onkeydown={(e) => { if (e.key === "Enter" || e.key === " ") selectedBand = idx; }}
            aria-label={`${i18n.t('equalizer.bandLabel')} ${idx + 1}`}
          >
            <!-- Gain display -->
            <span class="text-[8px] font-mono font-bold w-full text-center transition-colors {band.gain_db > 0 ? 'text-green-400' : band.gain_db < 0 ? 'text-red-400' : 'text-brand-text-secondary/50'}">
              {band.gain_db > 0 ? "+" : ""}{band.gain_db.toFixed(1)}
            </span>

            <!-- Slider track -->
            <div class="h-40 md:h-48 flex items-center justify-center relative">
              <input
                type="range"
                min="-12.0"
                max="12.0"
                step="0.5"
                use:verticalOrient
                bind:value={parametric[idx].gain_db}
                oninput={() => { selectedBand = idx; pushParametricBand(idx); }}
                class="accent-brand-accent cursor-ns-resize"
                style="appearance: slider-vertical; -webkit-appearance: slider-vertical; width: 10px; height: 100%;"
              />
            </div>

            <!-- Frequency label -->
            <span class="text-[8px] font-medium font-mono text-center tracking-tighter truncate w-full {selectedBand === idx ? 'text-brand-accent-text' : 'text-brand-text-secondary/70'}">
              {formatFreq(band.freq)}
            </span>
          </div>
        {/each}
      </div>

      <!-- Selected band detail: Q only (band frequencies are fixed) -->
      {#if parametric[selectedBand]}
        <div class="flex flex-col gap-2 bg-brand-sidebar/40 border border-brand-border rounded-xl p-4">
          <div class="flex justify-between items-center text-xs font-bold text-brand-text-secondary">
            <span>
              {i18n.t('equalizer.bandLabel')} {selectedBand + 1}
              <span class="text-brand-text-secondary/50 font-mono">· {formatFreq(parametric[selectedBand].freq)}Hz</span>
              — {i18n.t('equalizer.qFactor').toUpperCase()}
            </span>
            <span class="text-brand-accent-text font-mono">{parametric[selectedBand].q.toFixed(1)}</span>
          </div>
          <input
            type="range"
            min="0.1"
            max="10"
            step="0.1"
            bind:value={parametric[selectedBand].q}
            oninput={() => pushParametricBand(selectedBand)}
            class="w-full accent-brand-accent bg-brand-main h-1.5 rounded-lg appearance-none cursor-pointer"
          />
        </div>
      {/if}
    {/if}
  </div>
</div>
