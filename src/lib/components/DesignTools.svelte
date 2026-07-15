<script lang="ts">
  import { themeStore, PREDEFINED_THEMES, type ThemeColors, type Theme } from "../stores/theme.svelte";
  import { Palette, Plus, Trash2, Copy, Download, Upload, Eye, RotateCcw, AlertCircle, CheckCircle, Check } from "lucide-svelte";
  import { open } from "@tauri-apps/plugin-dialog";
  import {
    hexToRgb,
    rgbToHex,
    calculateLuminance,
    isLightColor,
    calculateContrastRatio,
    checkWcagCompliance,
    formatLuminance,
    getWcagBadgeColor,
    getWcagBadgeText
  } from "../utils/colorUtils";

  let { themeId = null }: { themeId: string | null } = $props();

  let showAdvanced = $state(false);
  let selectedColorTool = $state<string>("primary");
  let themeName = $state("");
  let isEditing = $state(false);
  let colorPresets = $state<ThemeColors>({
    "bg-main": "#0d0b18",
    "bg-sidebar": "#07050e",
    "bg-playerbar": "#0a0813",
    "color-accent": "#8b5cf6",
    "color-accent-hover": "#a78bfa",
    "color-text-primary": "#f3f4f6",
    "color-text-secondary": "#9ca3af",
    "color-border": "#1f1b2e"
  });

  const colorLabels: Record<string, { label: string; description: string }> = {
    "bg-main": { label: "Main Background", description: "Primary view and content area" },
    "bg-sidebar": { label: "Sidebar Background", description: "Left navigation panel" },
    "bg-playerbar": { label: "Player Bar", description: "Bottom playback controls" },
    "color-accent": { label: "Accent Color", description: "Buttons, highlights, focus states" },
    "color-accent-hover": { label: "Accent Hover", description: "Hover state for accent elements" },
    "color-text-primary": { label: "Primary Text", description: "Main readable text" },
    "color-text-secondary": { label: "Secondary Text", description: "Hints, labels, muted text" },
    "color-border": { label: "Border Color", description: "Dividers and outlines" }
  };

  function initializeTheme() {
    if (themeId) {
      isEditing = true;
      const theme = themeStore.customThemes.find(t => t.id === themeId);
      if (theme) {
        themeName = theme.name;
        colorPresets = { ...theme.colors };
        applyLivePreview();
      }
    } else {
      isEditing = false;
      themeName = "";
      loadActiveThemeColors();
    }
  }

  $effect(() => {
    if (themeId) {
      initializeTheme();
    }
  });

  function loadActiveThemeColors() {
    if (typeof document === "undefined") return;
    const rootStyle = getComputedStyle(document.documentElement);

    const getHexColor = (prop: string, fallback: string): string => {
      const val = rootStyle.getPropertyValue(prop).trim();
      if (!val) return fallback;
      if (val.startsWith("rgb")) {
        const match = val.match(/\d+/g);
        if (match && match.length >= 3) {
          const r = parseInt(match[0]);
          const g = parseInt(match[1]);
          const b = parseInt(match[2]);
          const toHex = (c: number) => {
            const hex = c.toString(16);
            return hex.length === 1 ? "0" + hex : hex;
          };
          return `#${toHex(r)}${toHex(g)}${toHex(b)}`;
        }
      }
      return val.startsWith("#") ? val : fallback;
    };

    colorPresets = {
      "bg-main": getHexColor("--bg-main", "#0d0b18"),
      "bg-sidebar": getHexColor("--bg-sidebar", "#07050e"),
      "bg-playerbar": getHexColor("--bg-playerbar", "#0a0813"),
      "color-accent": getHexColor("--color-accent", "#8b5cf6"),
      "color-accent-hover": getHexColor("--color-accent-hover", "#a78bfa"),
      "color-text-primary": getHexColor("--color-text-primary", "#f3f4f6"),
      "color-text-secondary": getHexColor("--color-text-secondary", "#9ca3af"),
      "color-border": getHexColor("--color-border", "#1f1b2e"),
    };
  }

  function applyLivePreview() {
    if (typeof document === "undefined") return;
    let styleEl = document.getElementById("luminous-theme-style");
    if (!styleEl) {
      styleEl = document.createElement("style");
      styleEl.id = "luminous-theme-style";
      document.head.appendChild(styleEl);
    }
    styleEl.innerHTML = `
      :root {
        --bg-main: ${colorPresets["bg-main"]};
        --bg-sidebar: ${colorPresets["bg-sidebar"]};
        --bg-playerbar: ${colorPresets["bg-playerbar"]};
        --color-accent: ${colorPresets["color-accent"]};
        --color-accent-hover: ${colorPresets["color-accent-hover"]};
        --color-text-primary: ${colorPresets["color-text-primary"]};
        --color-text-secondary: ${colorPresets["color-text-secondary"]};
        --color-border: ${colorPresets["color-border"]};
      }
    `;
  }


  function updateColorFromRgb(key: keyof ThemeColors, r: number, g: number, b: number) {
    colorPresets[key] = rgbToHex(Math.max(0, Math.min(255, r)), Math.max(0, Math.min(255, g)), Math.max(0, Math.min(255, b)));
    applyLivePreview();
  }

  async function saveTheme() {
    if (themeName.trim() === "") {
      alert("Please enter a theme name");
      return;
    }

    if (isEditing && themeId) {
      // Update existing theme
      const theme = themeStore.customThemes.find(t => t.id === themeId);
      if (theme) {
        await themeStore.addCustomTheme({
          ...theme,
          name: themeName.trim(),
          colors: { ...colorPresets }
        });
      }
    } else {
      // Create new theme
      const id = "custom-" + themeName.toLowerCase().replace(/[^a-z0-9]/g, "-");
      await themeStore.addCustomTheme({
        id,
        name: themeName.trim(),
        colors: { ...colorPresets },
        isCustom: true
      });
      themeName = "";
    }
  }

  async function importTheme() {
    try {
      const file = await open({
        filters: [{ name: 'JSON', extensions: ['json'] }],
        multiple: false
      });

      if (!file || typeof file !== 'string') return;

      const content = await fetch(file).then(r => r.text());
      const themeData = JSON.parse(content);

      if (themeData.colors && typeof themeData.colors === 'object') {
        colorPresets = { ...themeData.colors as ThemeColors };
        themeName = themeData.name || 'Imported Theme';
        applyLivePreview();
      } else {
        alert('Invalid theme file format');
      }
    } catch (e) {
      console.error('Failed to import theme:', e);
      alert('Failed to import theme. Please check the file format.');
    }
  }

  function resetColors() {
    loadActiveThemeColors();
    applyLivePreview();
  }

  function exportTheme() {
    const themeData = {
      name: themeName || themeStore.currentTheme.name,
      colors: colorPresets,
      exported: new Date().toISOString()
    };
    const json = JSON.stringify(themeData, null, 2);
    const blob = new Blob([json], { type: "application/json" });
    const url = URL.createObjectURL(blob);
    const a = document.createElement("a");
    a.href = url;
    a.download = `${themeData.name.toLowerCase().replace(/\s+/g, "-")}-theme.json`;
    document.body.appendChild(a);
    a.click();
    document.body.removeChild(a);
    URL.revokeObjectURL(url);
  }

  function copyToClipboard() {
    const hex = colorPresets[selectedColorTool as keyof ThemeColors];
    navigator.clipboard.writeText(hex);
  }

  function getContrastMetrics(foregroundHex: string, backgroundHex: string) {
    const ratio = calculateContrastRatio(foregroundHex, backgroundHex);
    const compliance = checkWcagCompliance(foregroundHex, backgroundHex);
    return { ratio, compliance };
  }

  function getLuminancePercent(hex: string): string {
    const lum = calculateLuminance(hex);
    return formatLuminance(lum);
  }
</script>

<div class="flex flex-col gap-6 w-full h-full">
  <!-- Header -->
  <div class="border-b border-brand-border pb-4">
    <div class="flex items-center gap-3 mb-2">
      <Palette class="w-5 h-5 text-brand-accent" />
      <h2 class="text-lg font-bold text-brand-text-primary">
        {isEditing ? `Edit Theme: ${themeName}` : 'Design Tools - Create Theme'}
      </h2>
    </div>
    <p class="text-sm text-brand-text-secondary">
      {isEditing
        ? 'Modify the theme colors below and save your changes'
        : 'Customize your app\'s appearance with advanced design controls'}
    </p>
  </div>

  <!-- Color Customization Grid -->
  <div class="grid grid-cols-1 lg:grid-cols-2 gap-6">
    <!-- Main Color Picker -->
    <div class="bg-brand-sidebar/40 border border-brand-border rounded-xl p-6 space-y-5">
      <h3 class="font-bold text-sm text-brand-text-primary">Color Palette</h3>

      <div class="space-y-4">
        {#each Object.entries(colorLabels) as [key, { label, description }]}
          {@const hexValue = colorPresets[key as keyof ThemeColors]}
          {@const rgb = hexToRgb(hexValue)}
          <div class="space-y-2">
            <div class="flex items-center justify-between">
              <div class="flex items-center gap-3">
                <button
                  type="button"
                  class="w-10 h-10 rounded-lg border-2 border-brand-border shadow-sm cursor-pointer hover:border-brand-accent transition-colors"
                  style="background-color: {hexValue}"
                  onclick={() => { selectedColorTool = key; }}
                  title={`Select ${label} for editing`}
                ></button>
                <div>
                  <p class="text-xs font-semibold text-brand-text-primary">{label}</p>
                  <p class="text-[10px] text-brand-text-secondary/60">{description}</p>
                </div>
              </div>
              <input
                type="color"
                bind:value={colorPresets[key as keyof ThemeColors]}
                oninput={applyLivePreview}
                class="w-12 h-8 rounded cursor-pointer border border-brand-border bg-transparent"
              />
            </div>
            <div class="space-y-1">
              <input
                type="text"
                bind:value={colorPresets[key as keyof ThemeColors]}
                oninput={applyLivePreview}
                class="w-full bg-brand-main border border-brand-border rounded px-3 py-2 text-xs font-mono text-brand-text-primary outline-none focus:border-brand-accent"
                placeholder="#000000"
              />
              <div class="flex items-center justify-between px-1">
                <span class="text-[10px] text-brand-text-secondary/60">
                  Luminance: <span class="font-mono">{getLuminancePercent(colorPresets[key as keyof ThemeColors])}</span>
                </span>
                <span class="text-[10px] font-semibold" style="color: {isLightColor(colorPresets[key as keyof ThemeColors]) ? '#fbbf24' : '#6ee7b7'}">
                  {isLightColor(colorPresets[key as keyof ThemeColors]) ? '🔆 Light' : '🌙 Dark'}
                </span>
              </div>
            </div>
          </div>
        {/each}
      </div>

      <button
        onclick={resetColors}
        class="w-full bg-brand-main hover:bg-brand-sidebar border border-brand-border hover:border-brand-accent/40 text-brand-text-primary px-4 py-2 rounded-lg text-xs font-semibold flex items-center justify-center gap-2 transition-colors cursor-pointer"
      >
        <RotateCcw class="w-4 h-4" /> Reset to Current Theme
      </button>
    </div>

    <!-- Color Detail Editor -->
    <div class="bg-brand-sidebar/40 border border-brand-border rounded-xl p-6 space-y-5">
      <h3 class="font-bold text-sm text-brand-text-primary">RGB Values</h3>

      {#if selectedColorTool}
        {@const key = selectedColorTool as keyof ThemeColors}
        {@const rgb = hexToRgb(colorPresets[key])}
        <div class="space-y-4">
          <div class="w-full h-32 rounded-lg border-2 border-brand-border overflow-hidden shadow-sm" style="background-color: {colorPresets[key]}"></div>

          <div class="space-y-3">
            <div>
              <label for="red-slider" class="text-xs font-semibold text-brand-text-primary block mb-1">Red: {rgb.r}</label>
              <input
                id="red-slider"
                type="range"
                min="0"
                max="255"
                bind:value={rgb.r}
                oninput={() => updateColorFromRgb(key, rgb.r, rgb.g, rgb.b)}
                class="w-full"
              />
            </div>
            <div>
              <label for="green-slider" class="text-xs font-semibold text-brand-text-primary block mb-1">Green: {rgb.g}</label>
              <input
                id="green-slider"
                type="range"
                min="0"
                max="255"
                bind:value={rgb.g}
                oninput={() => updateColorFromRgb(key, rgb.r, rgb.g, rgb.b)}
                class="w-full"
              />
            </div>
            <div>
              <label for="blue-slider" class="text-xs font-semibold text-brand-text-primary block mb-1">Blue: {rgb.b}</label>
              <input
                id="blue-slider"
                type="range"
                min="0"
                max="255"
                bind:value={rgb.b}
                oninput={() => updateColorFromRgb(key, rgb.r, rgb.g, rgb.b)}
                class="w-full"
              />
            </div>
          </div>

          <button
            onclick={copyToClipboard}
            class="w-full bg-brand-accent hover:bg-brand-accent-hover text-white px-4 py-2 rounded-lg text-xs font-semibold flex items-center justify-center gap-2 transition-colors cursor-pointer"
          >
            <Copy class="w-4 h-4" /> Copy HEX Value
          </button>

          <!-- Color Metrics Section -->
          <div class="border-t border-brand-border pt-4 space-y-3">
            <h4 class="font-semibold text-xs text-brand-text-primary">CIE Color Metrics</h4>

            <!-- Luminance -->
            <div class="bg-brand-main rounded-lg p-3 space-y-2">
              <div class="flex items-center justify-between">
                <span class="text-xs text-brand-text-secondary">Relative Luminance (CIE)</span>
                <span class="font-mono text-xs font-bold text-brand-accent">{getLuminancePercent(colorPresets[selectedColorTool as keyof ThemeColors])}</span>
              </div>
              <p class="text-[10px] text-brand-text-secondary/60">
                {isLightColor(colorPresets[selectedColorTool as keyof ThemeColors])
                  ? "🔆 Perceived as LIGHT - Use dark text"
                  : "🌙 Perceived as DARK - Use light text"}
              </p>
            </div>

            <!-- Contrast Ratios -->
            <div class="space-y-2">
              <h5 class="text-xs font-semibold text-brand-text-primary">Contrast Ratios</h5>

              {#each [{ bg: 'bg-main', label: 'vs Main Background' }, { bg: 'bg-sidebar', label: 'vs Sidebar Background' }, { bg: 'bg-playerbar', label: 'vs Player Bar Background' }] as item}
                {@const contrast = getContrastMetrics(colorPresets[selectedColorTool as keyof ThemeColors], colorPresets[item.bg as keyof ThemeColors])}
                <div class="bg-brand-main rounded-lg p-3 space-y-1">
                  <div class="flex items-center justify-between">
                    <span class="text-xs text-brand-text-secondary">{item.label}</span>
                    <span class="font-mono text-xs font-bold">{contrast.ratio}:1</span>
                  </div>
                  <div class="flex items-center gap-2">
                    {#if contrast.compliance.level === 'AAA'}
                      <CheckCircle class="w-3.5 h-3.5" style="color: #10b981;" />
                    {:else if contrast.compliance.level === 'AA'}
                      <AlertCircle class="w-3.5 h-3.5" style="color: #f59e0b;" />
                    {:else}
                      <AlertCircle class="w-3.5 h-3.5" style="color: #ef4444;" />
                    {/if}
                    <span class="text-[10px]" style="color: {getWcagBadgeColor(contrast.compliance.level)}">
                      {getWcagBadgeText(contrast.compliance.level)}
                    </span>
                  </div>
                </div>
              {/each}
            </div>

            <p class="text-[10px] text-brand-text-secondary/60 pt-2 border-t border-brand-border/50">
              WCAG AA: 4.5:1 minimum for normal text<br />
              WCAG AAA: 7:1 minimum for maximum accessibility
            </p>
          </div>
        </div>
      {/if}
    </div>
  </div>

  <!-- Theme Management -->
  <div class="bg-brand-sidebar border border-brand-border rounded-xl p-6 space-y-5">
    <h3 class="font-bold text-sm text-brand-text-primary">Save & Export Theme</h3>

    <div class="flex flex-col gap-4">
      <div class="flex flex-col gap-1.5">
        <label for="theme-name" class="text-xs text-brand-text-secondary font-semibold">Theme Name</label>
        <input
          id="theme-name"
          type="text"
          bind:value={themeName}
          placeholder="e.g. Ocean Blue, Sunset..."
          class="bg-brand-main border border-brand-border rounded-lg px-3 py-2 text-xs text-brand-text-primary outline-none focus:border-brand-accent"
        />
      </div>

      <div class="flex gap-3">
        <button
          onclick={saveTheme}
          class="flex-1 bg-brand-accent hover:bg-brand-accent-hover text-white px-4 py-2 rounded-lg text-xs font-semibold flex items-center justify-center gap-2 transition-colors cursor-pointer"
        >
          {#if isEditing}
            <Check class="w-4 h-4" /> Save Changes
          {:else}
            <Plus class="w-4 h-4" /> Save as Custom Theme
          {/if}
        </button>
        <button
          onclick={exportTheme}
          class="flex-1 bg-brand-main hover:bg-brand-sidebar border border-brand-border hover:border-brand-accent/40 text-brand-text-primary px-4 py-2 rounded-lg text-xs font-semibold flex items-center justify-center gap-2 transition-colors cursor-pointer"
        >
          <Download class="w-4 h-4" /> Export Theme
        </button>
        <button
          onclick={importTheme}
          class="flex-1 bg-brand-main hover:bg-brand-sidebar border border-brand-border hover:border-brand-accent/40 text-brand-text-primary px-4 py-2 rounded-lg text-xs font-semibold flex items-center justify-center gap-2 transition-colors cursor-pointer"
        >
          <Upload class="w-4 h-4" /> Import Theme
        </button>
      </div>
    </div>
  </div>

  <!-- UI Preview Section -->
  <div class="bg-brand-sidebar border border-brand-border rounded-xl p-6 space-y-5">
    <h3 class="font-bold text-sm text-brand-text-primary flex items-center gap-2">
      <Eye class="w-4 h-4" /> Live Preview
    </h3>

    <div class="grid grid-cols-1 md:grid-cols-3 gap-4">
      <!-- Sidebar Preview -->
      <div class="space-y-2">
        <p class="text-xs font-semibold text-brand-text-secondary">Sidebar Preview</p>
        <div class="rounded-lg overflow-hidden border border-brand-border shadow-sm h-32" style="background-color: {colorPresets['bg-sidebar']}">
          <div class="p-2 space-y-1">
            <div class="h-2 rounded" style="background-color: {colorPresets['color-accent']}; width: 70%;"></div>
            <div class="h-2 rounded" style="background-color: {colorPresets['color-accent']}; width: 50%;"></div>
            <div class="h-2 rounded" style="background-color: {colorPresets['color-border']}; width: 60%;"></div>
          </div>
        </div>
      </div>

      <!-- Main View Preview -->
      <div class="space-y-2">
        <p class="text-xs font-semibold text-brand-text-secondary">Main View Preview</p>
        <div class="rounded-lg overflow-hidden border border-brand-border shadow-sm h-32" style="background-color: {colorPresets['bg-main']}">
          <div class="p-2 space-y-2">
            <div class="h-2 rounded" style="background-color: {colorPresets['color-text-primary']}; width: 100%;"></div>
            <div class="h-2 rounded" style="background-color: {colorPresets['color-text-secondary']}; width: 80%;"></div>
            <div class="h-2 rounded mt-2" style="background-color: {colorPresets['color-accent']}; width: 40%;"></div>
          </div>
        </div>
      </div>

      <!-- Player Bar Preview -->
      <div class="space-y-2">
        <p class="text-xs font-semibold text-brand-text-secondary">Player Bar Preview</p>
        <div class="rounded-lg overflow-hidden border border-brand-border shadow-sm h-32" style="background-color: {colorPresets['bg-playerbar']}">
          <div class="p-2 space-y-2">
            <div class="h-2 rounded" style="background-color: {colorPresets['color-accent']}; width: 100%;"></div>
            <div class="flex gap-1 justify-center mt-2">
              <div class="w-3 h-3 rounded" style="background-color: {colorPresets['color-accent-hover']};"></div>
              <div class="w-3 h-3 rounded" style="background-color: {colorPresets['color-accent']};"></div>
              <div class="w-3 h-3 rounded" style="background-color: {colorPresets['color-accent-hover']};"></div>
            </div>
          </div>
        </div>
      </div>
    </div>
  </div>
</div>
