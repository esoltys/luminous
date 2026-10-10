<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { open, save } from "@tauri-apps/plugin-dialog";
  import { onMount } from "svelte";
  import { i18n, formatNumber } from "../stores/i18n.svelte";
  import { loudnessStore } from "../stores/loudness.svelte";
  import {
    SlidersIcon as Sliders,
    PulseIcon as Activity,
    ArrowsLeftRightIcon as ArrowLeftRight,
    ArrowSquareOutIcon as ExternalLink,
    DotsThreeIcon as MoreHorizontal,
    FloppyDiskIcon as Save,
    FolderOpenIcon as FileImport,
    ExportIcon as FileExport,
    PencilSimpleIcon as Pencil,
    TrashIcon as Trash2
  } from "phosphor-svelte";
  import Toggle from "./Toggle.svelte";
  import Select from "./Select.svelte";
  import Knob from "./Knob.svelte";
  import ParametricGraph from "./ParametricGraph.svelte";
  import ParametricBandStrip from "./ParametricBandStrip.svelte";
  import EqPresetPicker from "./EqPresetPicker.svelte";
  import ConfirmDialog from "./ConfirmDialog.svelte";
  import ContextMenu from "./ContextMenu.svelte";
  import ContextMenuItem from "./ContextMenuItem.svelte";
  import ContextMenuDivider from "./ContextMenuDivider.svelte";
  import { logSpacedFreqs } from "../utils/eqScale";
  import { importErrorMessage, profileNameFromPath } from "../utils/eqImport";
  import { openExternalUrl } from "../utils/openExternalUrl";
  import { toastStore } from "../stores/toast.svelte";

  import {
    userPresetKey,
    type EqConfig,
    type EqMode,
    type EqPresetList,
    type EqPresetPreview,
    type EqRanges,
    type ParametricBand,
    type SettingRange,
  } from "../types/equalizer";

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
  /** The backend's `active_preset`: a built-in name, `user:<id>`, or null (Custom). */
  let activePreset = $state<string | null>(null);
  let presetList = $state<EqPresetList>({ builtin: [], user: [] });

  let presetMenuPos = $state<{ x: number; y: number } | null>(null);
  let presetMenuButtonEl = $state<HTMLButtonElement | undefined>(undefined);

  function togglePresetMenu() {
    if (presetMenuPos) {
      presetMenuPos = null;
      return;
    }
    if (!presetMenuButtonEl) return;
    const rect = presetMenuButtonEl.getBoundingClientRect();
    presetMenuPos = { x: rect.left, y: rect.bottom + 8 };
  }

  /** Close the preset actions menu, then run the chosen action. */
  function fromPresetMenu(action: () => void) {
    presetMenuPos = null;
    action();
  }

  const bandLabels = $derived([
    ...[31.5, 63, 125, 250, 500].map((f) => `${formatNumber(f)} ${i18n.t("units.hz")}`),
    ...[1, 2, 4, 8, 16].map((f) => `${formatNumber(f)} ${i18n.t("units.khz")}`)
  ]);

  function presetLabel(presetName: string): string {
    const keyMap: Record<string, string> = {
      "Flat": "flatPreset",
      "Pop": "popPreset",
      "Rock": "rockPreset",
      "Bass Boost": "bassBoostPreset",
      "Vocal Boost": "vocalBoostPreset",
      "Treble Boost": "trebleBoostPreset"
    };
    const key = keyMap[presetName];
    return key ? i18n.t(`equalizer.${key}`) : presetName;
  }

  /** Take every field of a backend echo, including which preset it is. */
  function assignConfig(config: EqConfig) {
    enabled = config.enabled;
    mode = config.mode ?? "graphic10";
    preamp = config.preamp;
    gains = config.gains;
    parametric = config.parametric ?? [];
    activePreset = config.active_preset ?? null;
    selectedBand = Math.min(selectedBand, Math.max(0, parametric.length - 1));
  }

  async function loadConfig() {
    try {
      assignConfig(await invoke<EqConfig>("get_equalizer_state"));
    } catch (e) {
      console.error("Failed to load equalizer state:", e);
    }
  }

  let presetPreviews = $state<Map<string, number[]>>(new Map());

  async function loadPresetPreviews() {
    if (!ranges?.eq) return;
    try {
      const freqs = logSpacedFreqs(32, ranges.eq.freq);
      const previews = await invoke<EqPresetPreview[]>("get_eq_preset_previews", {
        frequencies: freqs,
        mode,
      });
      if (!Array.isArray(previews)) return;
      const map = new Map<string, number[]>();
      for (const p of previews) {
        map.set(p.key, p.response_db);
      }
      presetPreviews = map;
    } catch (e) {
      console.error("Failed to load equalizer preset previews:", e);
    }
  }

  async function loadPresetList() {
    try {
      presetList = await invoke<EqPresetList>("list_eq_presets");
      await loadPresetPreviews();
    } catch (e) {
      console.error("Failed to list equalizer presets:", e);
    }
  }

  /** The single EQ mutation path: send the whole edited config; the engine
   * clamps and echoes the canonical state back. Local edits already updated
   * the reactive fields, so the echo only matters when clamping changed a
   * value. Drags fire faster than the round-trip, so at most one apply is in
   * flight: edits made meanwhile coalesce into one follow-up with the latest
   * state, and an echo that newer edits have overtaken is not assigned. */
  let applyLoop: Promise<void> | null = null;
  let applyPending = false;

  function applyConfig(): Promise<void> {
    applyPending = true;
    applyLoop ??= (async () => {
      try {
        while (applyPending) {
          applyPending = false;
          const canonical = await invoke<EqConfig>("apply_equalizer_config", {
            config: { enabled, mode, preamp, gains, parametric },
          });
          if (applyPending) continue;
          assignConfig(canonical);
          await refreshCurves();
        }
      } catch (e) {
        console.error("Failed to apply equalizer config:", e);
      } finally {
        applyLoop = null;
      }
    })();
    return applyLoop;
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
    await loadPresetPreviews();
  }

  async function handlePreampChange() {
    await ensureEnabled();
    await applyConfig();
  }

  async function handleBandChange() {
    await ensureEnabled();
    await applyConfig();
  }

  function updateBand(idx: number, band: ParametricBand): Promise<void> {
    parametric[idx] = band;
    ensureEnabled();
    return applyConfig();
  }

  function addBand(freq: number) {
    parametric = [...parametric, { kind: "peak", freq, gain_db: 0, q: 1, enabled: true }];
    selectedBand = parametric.length - 1;
    applyConfig();
  }

  function removeBand(idx: number) {
    parametric = parametric.filter((_, i) => i !== idx);
    if (selectedBand > idx || selectedBand >= parametric.length) selectedBand = Math.max(0, selectedBand - 1);
    applyConfig();
  }

  function selectBand(idx: number) {
    selectedBand = idx;
    refreshBandCurve();
  }

  async function resetParametric() {
    try {
      assignConfig(await invoke<EqConfig>("reset_parametric_bands"));
      await refreshCurves();
    } catch (e) {
      console.error("Failed to reset parametric bands:", e);
    }
  }

  async function selectPreset(preset: string) {
    if (!preset) return;
    try {
      // Turn the EQ on in the engine, not just locally — the preset's echo
      // carries the engine's `enabled` and would switch it straight back off.
      if (!enabled) {
        enabled = true;
        await applyConfig();
      }
      assignConfig(await invoke<EqConfig>("load_equalizer_preset", { presetName: preset }));
      await refreshCurves();
    } catch (e) {
      console.error("Failed to load preset:", e);
    }
  }

  // --- User presets (#1335) ---
  let activeUserPreset = $derived(
    presetList.user.find((p) => activePreset === userPresetKey(p.id)) ?? null
  );
  /** The inline name field: saving a new preset or renaming the active one. */
  let nameEditor = $state<{ action: "save" | "rename"; name: string } | null>(null);
  let nameError = $state<string | null>(null);
  let confirmingDelete = $state(false);

  const PRESET_ERRORS: Record<string, string> = {
    empty_name: "equalizer.presetErrorEmptyName",
    duplicate_name: "equalizer.presetErrorDuplicateName",
    not_found: "equalizer.presetErrorNotFound",
  };

  function openNameEditor(action: "save" | "rename") {
    importPanel = null;
    nameEditor ={ action, name: action === "rename" ? (activeUserPreset?.name ?? "") : "" };
    nameError = null;
  }

  function closeNameEditor() {
    nameEditor = null;
    nameError = null;
  }

  async function submitName() {
    if (!nameEditor) return;
    try {
      if (nameEditor.action === "save") {
        assignConfig(await invoke<EqConfig>("save_eq_user_preset", { name: nameEditor.name }));
      } else if (activeUserPreset) {
        await invoke("rename_eq_user_preset", { id: activeUserPreset.id, name: nameEditor.name });
      }
      await loadPresetList();
      closeNameEditor();
    } catch (e) {
      nameError = i18n.t(PRESET_ERRORS[String(e)] ?? "equalizer.presetErrorGeneric");
    }
  }

  // --- Import an AutoEq / Equalizer APO profile (#1336) ---
  // The backend saves it as a user preset and makes it active, all or nothing.
  let importPanel = $state<{ text: string; name: string } | null>(null);
  let importError = $state<string | null>(null);
  let importing = $state(false);

  function openImportPanel() {
    importPanel = { text: "", name: "" };
    importError = null;
    nameEditor = null;
  }

  function closeImportPanel() {
    importPanel = null;
    importError = null;
  }

  async function chooseProfileFile() {
    try {
      const path = await open({
        multiple: false,
        title: i18n.t("equalizer.importChooseFile"),
        filters: [{ name: i18n.t("equalizer.importFileFilter"), extensions: ["txt"] }],
      });
      if (typeof path !== "string" || !importPanel) return;
      const text = await invoke<string>("read_eq_profile_file", { path });
      if (!importPanel) return;
      importPanel.text = text;
      importPanel.name = profileNameFromPath(path);
      importError = null;
    } catch (e) {
      importError = importErrorMessage(e);
    }
  }

  // --- Export the parametric bands as an Equalizer APO profile ---
  async function exportProfile() {
    const name = activeUserPreset?.name ?? (activePreset ? presetLabel(activePreset) : i18n.t("equalizer.customPreset"));
    try {
      const path = await save({
        title: i18n.t("equalizer.exportProfile"),
        defaultPath: `${name} ParametricEQ.txt`,
        filters: [{ name: i18n.t("equalizer.importFileFilter"), extensions: ["txt"] }],
      });
      if (!path) return;
      await invoke("export_parametric_profile", { path });
      toastStore.show(i18n.t("equalizer.exportSuccess", { name }));
    } catch (e) {
      console.error("Failed to export profile:", e);
      toastStore.show(i18n.t("equalizer.exportError", { error: String(e) }));
    }
  }

  async function submitImport() {
    if (!importPanel || importing) return;
    importing = true;
    try {
      assignConfig(
        await invoke<EqConfig>("import_parametric_profile", {
          text: importPanel.text,
          name: importPanel.name,
        })
      );
      importPanel = null;
      importError = null;
      await Promise.all([loadPresetList(), refreshCurves()]);
    } catch (e) {
      importError = importErrorMessage(e);
    } finally {
      importing = false;
    }
  }

  /** dB between labelled ticks under the preamp slider. */
  const PREAMP_TICK_DB = 6;

  function handlePreampInput(e: Event) {
    // One-way value: binding would let the range input's 0.5 dB step round an
    // imported off-grid preamp (e.g. -3.78), so only a drag writes it.
    preamp = Number((e.currentTarget as HTMLInputElement).value);
    handlePreampChange();
  }

  function focusOnMount(node: HTMLInputElement) {
    node.focus();
    node.select();
  }

  async function deleteActiveUserPreset() {
    confirmingDelete = false;
    if (!activeUserPreset) return;
    try {
      assignConfig(await invoke<EqConfig>("delete_eq_user_preset", { id: activeUserPreset.id }));
      await loadPresetList();
    } catch (e) {
      console.error("Failed to delete preset:", e);
    }
  }

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

  // The graph plots the response the backend evaluates for the filters it is
  // actually running (#1248) — the combined cascade, plus the selected band on
  // its own. Drags fire many applies, so a response older than the latest
  // request of its kind is discarded.
  const CURVE_SAMPLES = 96;
  let curveFreqs = $derived(ranges ? logSpacedFreqs(CURVE_SAMPLES, ranges.eq.freq) : []);
  let responseDb = $state<number[]>([]);
  let bandResponseDb = $state<number[]>([]);
  let curveRequest = 0;
  let bandCurveRequest = 0;

  async function refreshCurve() {
    if (mode !== "parametric" || curveFreqs.length === 0) return;
    const request = ++curveRequest;
    try {
      const db = await invoke<number[]>("get_parametric_response", { frequencies: curveFreqs });
      if (request === curveRequest) responseDb = db;
    } catch (e) {
      console.error("Failed to get parametric response:", e);
    }
  }

  async function refreshBandCurve() {
    if (mode !== "parametric" || curveFreqs.length === 0) return;
    const request = ++bandCurveRequest;
    const band = selectedBand;
    if (!parametric[band]) {
      bandResponseDb = [];
      return;
    }
    try {
      const db = await invoke<number[]>("get_parametric_response", { frequencies: curveFreqs, band });
      if (request === bandCurveRequest) bandResponseDb = db;
    } catch (e) {
      console.error("Failed to get band response:", e);
    }
  }

  function refreshCurves(): Promise<unknown> {
    return Promise.all([refreshCurve(), refreshBandCurve()]);
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

  // Dev-only entry points for scripted captures (src/lib/scripting/screens.ts); release builds never load them.
  $effect(() => {
    if (!import.meta.env.DEV) return;
    let off: (() => void) | undefined;
    let cancelled = false;
    import("../scripting/screens").then(({ registerScreenHook }) => {
      if (cancelled) return;
      const offs = [
        registerScreenHook("setEqualizerMode", handleModeChange),
        registerScreenHook("openEqualizerPresetMenu", () => {
          if (!presetMenuPos) togglePresetMenu();
        }),
      ];
      off = () => offs.forEach((o) => o());
    });
    return () => {
      cancelled = true;
      off?.();
    };
  });

  onMount(async () => {
    loadLoudnessSettings();
    loadFadeSettings();
    loudnessStore.init();
    await Promise.all([loadConfig(), loadSettingRanges(), loadPresetList()]);
    await Promise.all([refreshCurves(), loadPresetPreviews()]);
  });
</script>

{#if confirmingDelete && activeUserPreset}
  <ConfirmDialog
    title={i18n.t('equalizer.deletePresetTitle')}
    message={i18n.t('equalizer.deletePresetMessage', { name: activeUserPreset.name })}
    confirmLabel={i18n.t('equalizer.deletePreset')}
    cancelLabel={i18n.t('equalizer.cancelPreset')}
    onConfirm={deleteActiveUserPreset}
    onCancel={() => (confirmingDelete = false)}
  />
{/if}

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

      <!-- Equal grid columns, not flex-1: unequal label widths would drift the 50%-wide pill off its button. -->
      <div class="relative grid grid-cols-2 items-center bg-brand-main border border-brand-border rounded-[2rem] p-0.5" role="group" aria-label={i18n.t('equalizer.modeLabel')}>
        <!-- Sliding background pill -->
        <span
          class="absolute top-0.5 bottom-0.5 left-0.5 w-[calc(50%-2px)] rounded-full bg-brand-accent shadow-sm pointer-events-none transition-transform duration-200 ease-out {mode === 'parametric' ? 'translate-x-full' : 'translate-x-0'}"
          aria-hidden="true"
        ></span>
        <button
          class="relative z-10 whitespace-nowrap text-xs font-semibold px-4 py-1.5 rounded-full transition-colors duration-200 {mode === 'graphic10' ? 'text-brand-accent-contrast' : 'text-brand-text-secondary hover:text-brand-text-primary'}"
          onclick={() => handleModeChange("graphic10")}
          aria-pressed={mode === "graphic10"}
        >
          {i18n.t('equalizer.modeGraphic')}
        </button>
        <button
          class="relative z-10 whitespace-nowrap text-xs font-semibold px-4 py-1.5 rounded-full transition-colors duration-200 {mode === 'parametric' ? 'text-brand-accent-contrast' : 'text-brand-text-secondary hover:text-brand-text-primary'}"
          onclick={() => handleModeChange("parametric")}
          aria-pressed={mode === "parametric"}
        >
          {i18n.t('equalizer.modeParametric')}
        </button>
      </div>

      <div class="flex items-center gap-2 bg-brand-main border border-brand-border rounded-[2rem] px-4 py-1.5">
        <label for="eq-preset-picker" class="text-xs font-semibold text-brand-text-secondary">{i18n.t('equalizer.presetLabel')}:</label>
        <EqPresetPicker
          id="eq-preset-picker"
          value={activePreset}
          presets={presetList}
          {mode}
          previews={presetPreviews}
          gainRange={ranges?.eq?.gain_db}
          getLabel={presetLabel}
          onselect={selectPreset}
        />
        {#if mode === "parametric"}
          <button
            bind:this={presetMenuButtonEl}
            type="button"
            onclick={togglePresetMenu}
            aria-label={i18n.t('equalizer.presetActions')}
            title={i18n.t('equalizer.presetActions')}
            aria-haspopup="menu"
            aria-expanded={presetMenuPos !== null}
            class="flex items-center justify-center w-6 h-6 -mr-2 rounded-full text-brand-text-secondary hover:text-brand-accent-text hover:bg-brand-sidebar transition-colors shrink-0 cursor-pointer"
          >
            <MoreHorizontal class="w-4 h-4" />
          </button>
        {/if}
      </div>

      <div class="flex items-center gap-3 bg-brand-main border border-brand-border rounded-2xl px-4 py-1.5">
        <label for="eq-preamp" class="text-xs font-semibold text-brand-text-secondary">{i18n.t('equalizer.preamp')}:</label>
        {#if ranges}
          {@const preampRange = ranges.eq.preamp}
          <div class="w-48 flex flex-col pt-1">
            <input
              id="eq-preamp"
              type="range"
              min={preampRange.min}
              max={preampRange.max}
              step="0.5"
              value={preamp}
              oninput={handlePreampInput}
              class="themed-range w-full h-1.5 rounded-lg"
              style={rangeFillStyle(preamp, preampRange.min, preampRange.max)}
            />
            <div class="px-[7px]">
              <div class="relative w-full h-3.5 text-[9px] text-brand-text-secondary/60 font-medium mt-0.5" aria-hidden="true">
                {#each rangeTicks(preampRange, (preampRange.max - preampRange.min) / PREAMP_TICK_DB) as val (val)}
                  <div class="absolute top-0 flex flex-col items-center -translate-x-1/2" style="left: {((val - preampRange.min) / (preampRange.max - preampRange.min)) * 100}%">
                    <div class="h-1 w-[1px] bg-brand-border"></div>
                    <span class="leading-none">{val > 0 ? "+" : ""}{val}</span>
                  </div>
                {/each}
              </div>
            </div>
          </div>
        {/if}
        <span class="text-xs font-mono font-medium w-16 text-right {preamp > 0 ? 'text-green-400' : preamp < 0 ? 'text-red-400' : 'text-brand-text-primary'}">
          {preamp > 0 ? "+" : ""}{formatNumber(preamp, { minimumFractionDigits: 1, maximumFractionDigits: 2 })} {i18n.t("units.db")}
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

    {#if presetMenuPos && mode === "parametric"}
      <ContextMenu
        x={presetMenuPos.x}
        y={presetMenuPos.y}
        estimatedHeight={activeUserPreset ? 220 : 140}
        onClose={() => (presetMenuPos = null)}
      >
        <ContextMenuItem
          icon={FileImport}
          label={i18n.t('equalizer.importProfile')}
          onclick={() => fromPresetMenu(openImportPanel)}
        />
        <ContextMenuItem
          icon={FileExport}
          label={i18n.t('equalizer.exportProfile')}
          onclick={() => fromPresetMenu(exportProfile)}
        />
        <ContextMenuDivider />
        <ContextMenuItem
          icon={Save}
          label={i18n.t('equalizer.savePresetAs')}
          onclick={() => fromPresetMenu(() => openNameEditor("save"))}
        />
        {#if activeUserPreset}
          <ContextMenuItem
            icon={Pencil}
            label={i18n.t('equalizer.renamePreset')}
            onclick={() => fromPresetMenu(() => openNameEditor("rename"))}
          />
          <ContextMenuDivider />
          <ContextMenuItem
            icon={Trash2}
            label={i18n.t('equalizer.deletePreset')}
            destructive
            onclick={() => fromPresetMenu(() => (confirmingDelete = true))}
          />
        {/if}
      </ContextMenu>
    {/if}

    {#if nameEditor && mode === "parametric"}
      <form
        class="flex flex-col gap-1"
        onsubmit={(e) => { e.preventDefault(); submitName(); }}
      >
        <div class="flex items-center gap-2 flex-wrap">
          <label for="eq-preset-name" class="text-xs font-semibold text-brand-text-secondary">
            {nameEditor.action === "save" ? i18n.t('equalizer.savePresetLabel') : i18n.t('equalizer.renamePresetLabel')}
          </label>
          <input
            id="eq-preset-name"
            type="text"
            bind:value={nameEditor.name}
            use:focusOnMount
            onkeydown={(e) => { if (e.key === "Escape") closeNameEditor(); }}
            aria-invalid={nameError !== null}
            aria-describedby={nameError ? "eq-preset-name-error" : undefined}
            class="bg-brand-main text-xs text-brand-text-primary border border-brand-border rounded px-3 py-1 outline-none focus:border-brand-accent min-w-48"
          />
          <button
            type="submit"
            class="text-xs font-semibold px-4 py-1.5 bg-brand-accent text-brand-accent-contrast rounded-full hover:bg-brand-accent-hover transition-colors"
          >
            {i18n.t('equalizer.savePreset')}
          </button>
          <button
            type="button"
            class="text-xs font-semibold px-4 py-1.5 bg-brand-main border border-brand-border rounded-full text-brand-text-secondary hover:text-brand-text-primary transition-colors"
            onclick={closeNameEditor}
          >
            {i18n.t('equalizer.cancelPreset')}
          </button>
        </div>
        {#if nameError}
          <p id="eq-preset-name-error" class="text-xs text-red-400" role="alert">{nameError}</p>
        {/if}
      </form>
    {/if}

    {#if importPanel && mode === "parametric"}
      <form
        class="flex flex-col gap-3 bg-brand-main/50 border border-brand-border/50 rounded-xl p-4"
        aria-label={i18n.t('equalizer.importPanelLabel')}
        onsubmit={(e) => { e.preventDefault(); submitImport(); }}
      >
        <p class="text-xs text-brand-text-secondary leading-relaxed text-pretty">
          {i18n.t('equalizer.importHint')}
          <button
            type="button"
            class="inline-flex items-center gap-0.5 text-brand-accent-text hover:text-brand-accent-text-hover hover:underline"
            onclick={() => openExternalUrl("https://autoeq.app")}
          >
            {i18n.t('equalizer.importAutoEqLink')}
            <ExternalLink class="w-3 h-3 shrink-0" aria-hidden="true" />
          </button>
        </p>
        <div class="flex items-center gap-2 flex-wrap">
          <button
            type="button"
            class="text-xs font-semibold px-4 py-1.5 bg-brand-main border border-brand-border rounded-full text-brand-text-secondary hover:text-brand-text-primary transition-colors"
            onclick={chooseProfileFile}
          >
            {i18n.t('equalizer.importChooseFile')}
          </button>
          <span class="text-xs text-brand-text-secondary">{i18n.t('equalizer.importOrPaste')}</span>
        </div>
        <textarea
          bind:value={importPanel.text}
          rows="6"
          spellcheck="false"
          aria-label={i18n.t('equalizer.importTextLabel')}
          placeholder={i18n.t('equalizer.importPlaceholder')}
          class="bg-brand-main text-xs font-mono text-brand-text-primary border border-brand-border rounded px-3 py-2 outline-none focus:border-brand-accent resize-y"
        ></textarea>
        <div class="flex items-center gap-2 flex-wrap">
          <label for="eq-import-name" class="text-xs font-semibold text-brand-text-secondary">
            {i18n.t('equalizer.savePresetLabel')}
          </label>
          <input
            id="eq-import-name"
            type="text"
            bind:value={importPanel.name}
            onkeydown={(e) => { if (e.key === "Escape") closeImportPanel(); }}
            aria-invalid={importError !== null}
            aria-describedby={importError ? "eq-import-error" : undefined}
            class="bg-brand-main text-xs text-brand-text-primary border border-brand-border rounded px-3 py-1 outline-none focus:border-brand-accent min-w-48"
          />
          <button
            type="submit"
            disabled={importing || !importPanel.text.trim() || !importPanel.name.trim()}
            class="text-xs font-semibold px-4 py-1.5 bg-brand-accent text-brand-accent-contrast rounded-full hover:bg-brand-accent-hover transition-colors disabled:opacity-50 disabled:cursor-not-allowed"
          >
            {i18n.t('equalizer.importSubmit')}
          </button>
          <button
            type="button"
            class="text-xs font-semibold px-4 py-1.5 bg-brand-main border border-brand-border rounded-full text-brand-text-secondary hover:text-brand-text-primary transition-colors"
            onclick={closeImportPanel}
          >
            {i18n.t('equalizer.cancelPreset')}
          </button>
        </div>
        {#if importError}
          <p id="eq-import-error" class="text-xs text-red-400" role="alert">{importError}</p>
        {/if}
      </form>
    {/if}
    </div>

    <!-- Slider bounds are the backend's clamp range (#1249), so wait for them. -->
    {#if ranges && mode === "graphic10"}
      <div class="grid grid-cols-5 @3xl:grid-cols-10 gap-3 @3xl:gap-5 min-h-64 h-auto @3xl:h-72 items-center bg-brand-main/50 border border-brand-border/50 rounded-xl p-4 @3xl:p-6">
        {#each gains as gain, idx}
          <div class="flex flex-col items-center justify-between h-full group">
            <span class="text-[10px] font-bold w-full text-center transition-colors {gain > 0 ? 'text-green-400/80' : gain < 0 ? 'text-red-400/80' : 'text-brand-text-secondary/70'}">
              {gain > 0 ? "+" : ""}{formatNumber(gain, { minimumFractionDigits: 1, maximumFractionDigits: 1 })}
            </span>

            <div class="h-40 @3xl:h-48 flex items-center justify-center relative">
              <input
                type="range"
                min={ranges.eq.gain_db.min}
                max={ranges.eq.gain_db.max}
                step="0.25"
                use:verticalOrient
                bind:value={gains[idx]}
                oninput={handleBandChange}
                class="accent-brand-accent cursor-ns-resize"
                style="appearance: slider-vertical; -webkit-appearance: slider-vertical; width: 12px; height: 100%;"
              />
            </div>

            <span class="text-[10px] @3xl:text-[11px] font-medium text-brand-text-secondary text-center truncate w-full">
              {bandLabels[idx]}
            </span>
          </div>
        {/each}
      </div>
      <p class="text-xs text-brand-text-secondary px-1 -mt-2">
        {i18n.t('equalizer.isoStandard')}
      </p>
    {:else if ranges}
      <ParametricGraph
        bands={parametric}
        selected={selectedBand}
        ranges={ranges.eq}
        active={enabled}
        response={responseDb}
        bandResponse={bandResponseDb}
        onselect={selectBand}
        onchange={updateBand}
        onadd={addBand}
        onremove={removeBand}
      />
      <ParametricBandStrip
        bands={parametric}
        selected={selectedBand}
        ranges={ranges.eq}
        onselect={selectBand}
        onchange={updateBand}
        onadd={addBand}
        onremove={removeBand}
      />
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

      <div class="grid grid-cols-1 @3xl:grid-cols-3 gap-12">
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
              suffix={i18n.t("units.lufs")}
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
              suffix={i18n.t("units.db")}
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
          {i18n.plural('loudness.analyzing', loudnessStore.analysisRemaining)}
        {:else}
          {i18n.plural('loudness.analysisPaused', loudnessStore.analysisRemaining)}
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
            <span class="font-mono font-bold text-brand-text-primary">{formatNumber(crossfadeAutoDurationSecs, { minimumFractionDigits: 1, maximumFractionDigits: 1 })}s</span>
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
                  <span>{formatNumber(val, { minimumFractionDigits: 1, maximumFractionDigits: 1 })}s</span>
                </div>
              {/each}
            </div>
          </div>
          <div class="flex items-center justify-between gap-2 pt-4 @3xl:w-1/2 text-xs text-brand-text-secondary">
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
