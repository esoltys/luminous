<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { onMount } from "svelte";
  import { SlidersHorizontal } from "lucide-svelte";

  let enabled = $state(false);
  let preamp = $state(0.0);
  let gains = $state<number[]>(Array(10).fill(0.0));
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
      const config = await invoke<{ enabled: boolean; preamp: number; gains: number[] }>("get_equalizer_state");
      enabled = config.enabled;
      preamp = config.preamp;
      gains = config.gains;
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

  async function handlePreampChange() {
    await ensureEnabled();
    await invoke("set_equalizer_preamp", { preampDb: preamp });
  }

  async function handleBandChange(index: number) {
    activePreset = "Custom";
    await ensureEnabled();
    await invoke("set_equalizer_band", { bandIdx: index, gainDb: gains[index] });
  }

  async function selectPreset(preset: string) {
    if (preset === "Custom") return;
    try {
      await ensureEnabled();
      const config = await invoke<{ enabled: boolean; preamp: number; gains: number[] }>("load_equalizer_preset", { presetName: preset });
      gains = config.gains;
      activePreset = preset;
    } catch (e) {
      console.error("Failed to load preset:", e);
    }
  }

  // Smooth Catmull-Rom spline path generator for the SVG EQ envelope graphic
  let curvePath = $derived.by(() => {
    if (gains.length === 0) return "";
    const pts = gains.map((g, i) => ({
      x: (i / 9) * 100,
      y: 20 - (g / 12.0) * 17
    }));

    let d = `M ${pts[0].x} ${pts[0].y}`;

    for (let i = 0; i < pts.length - 1; i++) {
      const p0 = i > 0 ? pts[i - 1] : pts[i];
      const p1 = pts[i];
      const p2 = pts[i + 1];
      const p3 = i < pts.length - 2 ? pts[i + 2] : p2;

      // Calculate control points for smooth transition
      const cp1x = p1.x + (p2.x - p0.x) / 6;
      const cp1y = p1.y + (p2.y - p0.y) / 6;

      const cp2x = p2.x - (p3.x - p1.x) / 6;
      const cp2y = p2.y - (p3.y - p1.y) / 6;

      d += ` C ${cp1x} ${cp1y}, ${cp2x} ${cp2y}, ${p2.x} ${p2.y}`;
    }
    return d;
  });

  function verticalOrient(node: HTMLInputElement) {
    node.setAttribute("orient", "vertical");
  }

  onMount(loadConfig);
</script>

<div class="flex flex-col text-brand-text-primary">
  <div class="flex items-center justify-between mb-6 pb-4 border-b border-brand-border">
    <div>
      <h3 class="text-sm font-bold text-brand-text-primary">Graphic Equalizer</h3>
      <p class="text-xs text-brand-text-secondary/70 mt-0.5">Shape your frequency response with a 10-band cascaded peaking filter</p>
    </div>

    <!-- Toggle & Presets controls -->
    <div class="flex items-center gap-4">
      <div class="flex items-center gap-2 bg-brand-sidebar/40 border border-brand-border rounded-lg px-3 py-1.5">
        <label for="eq-toggle" class="text-xs font-semibold text-brand-text-secondary">Enable EQ</label>
        <input
          id="eq-toggle"
          type="checkbox"
          bind:checked={enabled}
          onchange={handleToggle}
          class="w-4 h-4 text-brand-accent-text bg-brand-main border-brand-border rounded focus:ring-brand-accent accent-brand-accent cursor-pointer"
        />
      </div>

      <div class="flex items-center gap-2 bg-brand-sidebar/40 border border-brand-border rounded-lg px-3 py-1.5">
        <span class="text-xs font-semibold text-brand-text-secondary">Preset:</span>
        <select
          bind:value={activePreset}
          onchange={() => selectPreset(activePreset)}
          class="bg-brand-main text-xs text-brand-text-primary border border-brand-border rounded px-2.5 py-1 pr-6 outline-none cursor-pointer focus:border-brand-accent font-medium appearance-none -webkit-appearance-none"
          style="background-image: url(&quot;data:image/svg+xml;charset=utf-8,%3Csvg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 20 20' fill='none'%3E%3Cpath stroke='%239ca3af' stroke-linecap='round' stroke-linejoin='round' stroke-width='1.5' d='m6 8 4 4 4-4'/%3E%3C/svg%3E&quot;); background-position: right 0.375rem center; background-repeat: no-repeat; background-size: 1.25em;"
        >
          {#each presets as preset}
            <option value={preset} class="bg-brand-main text-brand-text-primary">{preset}</option>
          {/each}
          {#if activePreset === "Custom"}
            <option value="Custom" class="bg-brand-main text-brand-text-primary" disabled>Custom</option>
          {/if}
        </select>
      </div>
    </div>
  </div>

  <!-- Rack Container -->
  <div class="bg-brand-sidebar/20 border border-brand-border rounded-2xl p-6 md:p-8 flex flex-col gap-8 shadow-xl shadow-black/30">
    <!-- Preamp and Spline Preview -->
    <div class="grid grid-cols-1 lg:grid-cols-4 gap-6 items-center">
      <!-- Preamp Slider -->
      <div class="flex flex-col gap-2 bg-brand-sidebar/40 border border-brand-border rounded-xl p-4 lg:col-span-1">
        <div class="flex justify-between items-center text-xs font-bold text-brand-text-secondary">
          <span>PRE-AMP</span>
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
          class="w-full accent-brand-accent bg-brand-main h-1.5 rounded-lg appearance-none cursor-pointer"
        />
      </div>

      <!-- EQ Curve Preview -->
      <div class="lg:col-span-3 h-20 bg-brand-main border border-brand-border rounded-xl p-3 flex flex-col justify-between relative overflow-hidden">
        <!-- Center line -->
        <div class="absolute left-0 right-0 top-1/2 border-t border-dashed border-brand-border pointer-events-none"></div>
        <svg class="w-full h-full" viewBox="0 0 100 40" preserveAspectRatio="none">
          {#if gains.length === 10}
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
          <span>Bass</span>
          <span>Mid</span>
          <span>Treble</span>
        </div>
      </div>
    </div>

    <!-- Sliders Rack -->
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
  </div>
</div>
