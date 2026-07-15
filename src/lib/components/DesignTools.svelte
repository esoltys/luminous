<script lang="ts">
  import { themeStore, PREDEFINED_THEMES, type ThemeColors, type Theme } from "../stores/theme.svelte";
  import { Plus, Download, Upload, Eye, RotateCcw, Check, X } from "lucide-svelte";
  import { open } from "@tauri-apps/plugin-dialog";
  import {
    calculateLuminance,
    isLightColor,
    calculateContrastRatio,
    checkWcagCompliance,
    formatLuminance,
    getWcagBadgeText
  } from "../utils/colorUtils";

  let { themeId = null, customColors = undefined, newThemeName = undefined }: { themeId?: string | null; customColors?: ThemeColors; newThemeName?: string } = $props();

  let showAdvanced = $state(false);
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

  // Initialize from props
  $effect.pre(() => {
    if (newThemeName) {
      themeName = newThemeName;
    }
    if (customColors) {
      Object.assign(colorPresets, customColors);
    }
  });

  // Sync local state with parent customColors prop when provided
  $effect(() => {
    if (customColors) {
      Object.assign(customColors, colorPresets);
    }
  });

  const bgColorLabels: Record<string, { label: string; description: string }> = {
    "bg-main": { label: "Main Background", description: "Primary view and content area" },
    "bg-sidebar": { label: "Sidebar Background", description: "Left navigation panel" },
    "bg-playerbar": { label: "Player Bar", description: "Bottom playback controls" },
    "color-accent": { label: "Accent Color", description: "Buttons, highlights, focus states" },
    "color-accent-hover": { label: "Accent Hover", description: "Hover state for accent elements" },
    "color-border": { label: "Border Color", description: "Dividers and outlines" }
  };

  const textColorLabels: Record<string, { label: string; description: string }> = {
    "color-text-primary": { label: "Primary Text", description: "Main readable text" },
    "color-text-secondary": { label: "Secondary Text", description: "Hints, labels, muted text" }
  };

  const backgroundTargets: { bg: keyof ThemeColors; label: string }[] = [
    { bg: "bg-main", label: "Main" },
    { bg: "bg-sidebar", label: "Sidebar" },
    { bg: "bg-playerbar", label: "Player Bar" },
    { bg: "color-accent", label: "Accent" }
  ];

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

  <!-- Color Customization Grid -->
  <div class="grid grid-cols-1 lg:grid-cols-2 gap-6">
    <!-- Main Color Picker -->
    <div class="bg-brand-sidebar/40 border border-brand-border rounded-xl p-6 space-y-5">
      <h3 class="font-bold text-sm text-brand-text-primary">Color Palette</h3>

      <div class="space-y-4">
        {#each Object.entries(bgColorLabels) as [key, { label, description }]}
          {@const hexValue = colorPresets[key as keyof ThemeColors]}
          <div class="space-y-2">
            <div class="flex items-center justify-between">
              <div class="flex items-center gap-3">
                <div
                  class="w-10 h-10 rounded-lg border-2 border-brand-border shadow-sm shrink-0"
                  style="background-color: {hexValue}"
                ></div>
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
                  Luminance: <span class="font-mono">{getLuminancePercent(hexValue)}</span>
                </span>
                <span class="text-[10px] font-semibold" style="color: {isLightColor(hexValue) ? '#fbbf24' : '#6ee7b7'}" title={isLightColor(hexValue) ? 'Light background - use dark text for contrast' : 'Dark background - use light text for contrast'}>
                  {isLightColor(hexValue) ? 'Light bg' : 'Dark bg'}
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

    <!-- Text Colors -->
    <div class="bg-brand-sidebar/40 border border-brand-border rounded-xl p-6 space-y-6">
      <h3 class="font-bold text-sm text-brand-text-primary">Text Colors</h3>

      {#each Object.entries(textColorLabels) as [key, { label, description }]}
        {@const hexValue = colorPresets[key as keyof ThemeColors]}
        <div class="space-y-3">
          <div class="flex items-center justify-between">
            <div class="flex items-center gap-3">
              <div
                class="w-10 h-10 rounded-lg border-2 border-brand-border shadow-sm shrink-0"
                style="background-color: {hexValue}"
              ></div>
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
          <input
            type="text"
            bind:value={colorPresets[key as keyof ThemeColors]}
            oninput={applyLivePreview}
            class="w-full bg-brand-main border border-brand-border rounded px-3 py-2 text-xs font-mono text-brand-text-primary outline-none focus:border-brand-accent"
            placeholder="#000000"
          />

          <!-- Contrast against every background -->
          <div class="grid grid-cols-2 gap-2">
            {#each backgroundTargets as target}
              {@const contrast = getContrastMetrics(hexValue, colorPresets[target.bg])}
              <div class="bg-brand-main rounded-lg p-2 flex flex-col items-center gap-1" title={getWcagBadgeText(contrast.compliance.level)}>
                <span class="text-[9px] text-brand-text-secondary/70">{target.label}</span>
                <div class="flex items-center gap-1">
                  <span class="font-mono text-xs font-bold">{Math.round(contrast.ratio)}:1</span>
                  {#if contrast.compliance.wcagAA}
                    <Check class="w-3 h-3" style="color: #10b981;" />
                  {:else}
                    <X class="w-3 h-3" style="color: #ef4444;" />
                  {/if}
                </div>
              </div>
            {/each}
          </div>
        </div>
      {/each}

      <p class="text-[10px] text-brand-text-secondary/60 pt-2 border-t border-brand-border/50">
        Checkmark = meets WCAG AA (4.5:1 minimum for normal text)
      </p>
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
          class="flex-1 bg-brand-accent hover:bg-brand-accent-hover text-brand-accent-contrast px-4 py-2 rounded-lg text-xs font-semibold flex items-center justify-center gap-2 transition-colors cursor-pointer"
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
