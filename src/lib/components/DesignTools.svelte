<script lang="ts">
  import { themeStore, PREDEFINED_THEMES, type ThemeColors, type Theme } from "../stores/theme.svelte";
  import { Plus, Download, Upload, Eye, RotateCcw, Check, X } from "lucide-svelte";
  import { open } from "@tauri-apps/plugin-dialog";
  import { i18n } from "../stores/i18n.svelte";
  import {
    calculateLuminance,
    isLightColor,
    calculateContrastRatio,
    checkWcagCompliance,
    formatLuminance,
    getWcagBadgeText
  } from "../utils/colorUtils";

  let { themeId = null, customColors = undefined, newThemeName = undefined, onSaved = undefined }: { themeId?: string | null; customColors?: ThemeColors; newThemeName?: string; onSaved?: () => void } = $props();

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

  // Seed from props once on mount only — NOT a continuous reactive pull.
  // A two-way $effect.pre (pull from customColors) + $effect (push to
  // customColors) pairing is a trap: $effect.pre runs before $effect in
  // the same flush, so after a local reassignment of colorPresets (e.g.
  // resetColors()), the pull effect fires first, reads the *stale*
  // pre-change customColors, and overwrites the very value that was just
  // reset — right before the push effect would have propagated it out.
  let seededFromProps = false;
  $effect(() => {
    if (!seededFromProps) {
      if (newThemeName) {
        themeName = newThemeName;
      }
      if (customColors) {
        Object.assign(colorPresets, customColors);
      }
      seededFromProps = true;
    }
  });

  // One-directional: local edits (including resets) push out to the
  // parent's customColors, so Simple/Advanced mode stay in sync.
  $effect(() => {
    if (customColors) {
      Object.assign(customColors, colorPresets);
    }
  });

  const bgColorFields: { key: keyof ThemeColors; labelKey: string; descKey: string }[] = [
    { key: "bg-main", labelKey: "settings.mainViewLabel", descKey: "settings.mainViewDescription" },
    { key: "bg-sidebar", labelKey: "settings.sidebarLabel", descKey: "settings.sidebarDescription" },
    { key: "bg-playerbar", labelKey: "settings.playerBarLabel", descKey: "settings.playerBarDescription" },
    { key: "color-accent", labelKey: "settings.accentLabel", descKey: "settings.accentDescription" },
    { key: "color-accent-hover", labelKey: "settings.accentHoverLabel", descKey: "settings.accentHoverDescription" },
    { key: "color-border", labelKey: "settings.bordersLabel", descKey: "settings.bordersDescription" }
  ];

  const textColorFields: { key: keyof ThemeColors; labelKey: string; descKey: string }[] = [
    { key: "color-text-primary", labelKey: "settings.primaryTextLabel", descKey: "settings.primaryTextDescription" },
    { key: "color-text-secondary", labelKey: "settings.secondaryTextLabel", descKey: "settings.secondaryTextDescription" }
  ];

  const backgroundTargets: { bg: keyof ThemeColors; labelKey: string }[] = [
    { bg: "bg-main", labelKey: "settings.mainViewLabel" },
    { bg: "bg-sidebar", labelKey: "settings.sidebarLabel" },
    { bg: "bg-playerbar", labelKey: "settings.playerBarLabel" },
    { bg: "color-accent", labelKey: "settings.accentLabel" }
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
    } else if (isEditing) {
      // Leaving edit mode (themeId went back to null/undefined) — drop back
      // to the create-new-theme flow, reseeded from the current
      // customColors/newThemeName props rather than left on stale edit state.
      isEditing = false;
      themeName = newThemeName ?? "";
      if (customColors) {
        colorPresets = { ...customColors };
      }
    }
  }

  $effect(() => {
    initializeTheme();
  });

  function loadActiveThemeColors() {
    colorPresets = { ...themeStore.resolvedColors };
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
      alert(i18n.t('settings.enterThemeNameAlert'));
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
    onSaved?.();
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
        themeName = themeData.name || i18n.t('settings.importedThemeDefaultName');
        applyLivePreview();
      } else {
        alert(i18n.t('settings.invalidThemeFile'));
      }
    } catch (e) {
      console.error('Failed to import theme:', e);
      alert(i18n.t('settings.importThemeFailed'));
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
      <h3 class="font-bold text-sm text-brand-text-primary">{i18n.t('settings.colorPalette')}</h3>

      <div class="space-y-4">
        {#each bgColorFields as { key, labelKey, descKey }}
          {@const hexValue = colorPresets[key]}
          <div class="space-y-2">
            <div class="flex items-center justify-between">
              <div class="flex items-center gap-3">
                <div
                  class="w-10 h-10 rounded-lg border-2 border-brand-border shadow-sm shrink-0"
                  style="background-color: {hexValue}"
                ></div>
                <div>
                  <p class="text-xs font-semibold text-brand-text-primary">{i18n.t(labelKey)}</p>
                  <p class="text-[10px] text-brand-text-secondary/60">{i18n.t(descKey)}</p>
                </div>
              </div>
              <input
                type="color"
                bind:value={colorPresets[key]}
                oninput={applyLivePreview}
                class="w-12 h-8 rounded cursor-pointer border border-brand-border bg-transparent"
              />
            </div>
            <div class="space-y-1">
              <input
                type="text"
                bind:value={colorPresets[key]}
                oninput={applyLivePreview}
                class="w-full bg-brand-main border border-brand-border rounded px-3 py-2 text-xs font-mono text-brand-text-primary outline-none focus:border-brand-accent"
                placeholder={i18n.t('settings.hexPlaceholder')}
              />
              <div class="flex items-center justify-between px-1">
                <span class="text-[10px] text-brand-text-secondary/60">
                  {i18n.t('settings.luminanceLabel')} <span class="font-mono">{getLuminancePercent(hexValue)}</span>
                </span>
                <span class="text-[10px] font-semibold text-brand-text-secondary" title={isLightColor(hexValue) ? i18n.t('settings.lightBgTooltip') : i18n.t('settings.darkBgTooltip')}>
                  {isLightColor(hexValue) ? i18n.t('settings.lightBg') : i18n.t('settings.darkBg')}
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
        <RotateCcw class="w-4 h-4" /> {i18n.t('settings.resetToCurrentTheme')}
      </button>
    </div>

    <!-- Text Colors -->
    <div class="bg-brand-sidebar/40 border border-brand-border rounded-xl p-6 space-y-6">
      <h3 class="font-bold text-sm text-brand-text-primary">{i18n.t('settings.textColors')}</h3>

      {#each textColorFields as { key, labelKey, descKey }}
        {@const hexValue = colorPresets[key]}
        <div class="space-y-3">
          <div class="flex items-center justify-between">
            <div class="flex items-center gap-3">
              <div
                class="w-10 h-10 rounded-lg border-2 border-brand-border shadow-sm shrink-0"
                style="background-color: {hexValue}"
              ></div>
              <div>
                <p class="text-xs font-semibold text-brand-text-primary">{i18n.t(labelKey)}</p>
                <p class="text-[10px] text-brand-text-secondary/60">{i18n.t(descKey)}</p>
              </div>
            </div>
            <input
              type="color"
              bind:value={colorPresets[key]}
              oninput={applyLivePreview}
              class="w-12 h-8 rounded cursor-pointer border border-brand-border bg-transparent"
            />
          </div>
          <input
            type="text"
            bind:value={colorPresets[key]}
            oninput={applyLivePreview}
            class="w-full bg-brand-main border border-brand-border rounded px-3 py-2 text-xs font-mono text-brand-text-primary outline-none focus:border-brand-accent"
            placeholder={i18n.t('settings.hexPlaceholder')}
          />

          <!-- Contrast against every background -->
          <div class="grid grid-cols-2 gap-2">
            {#each backgroundTargets as target}
              {@const contrast = getContrastMetrics(hexValue, colorPresets[target.bg])}
              <div class="bg-brand-main rounded-lg p-2 flex flex-col items-center gap-1" title={getWcagBadgeText(contrast.compliance.level)}>
                <span class="text-[9px] text-brand-text-secondary/70">{i18n.t(target.labelKey)}</span>
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
        {i18n.t('settings.wcagHelpText')}
      </p>
    </div>
  </div>

  <!-- Theme Management -->
  <div class="bg-brand-sidebar border border-brand-border rounded-xl p-6 space-y-5">
    <h3 class="font-bold text-sm text-brand-text-primary">{i18n.t('settings.saveExportTheme')}</h3>

    <div class="flex flex-col gap-4">
      <div class="flex flex-col gap-1.5">
        <label for="theme-name" class="text-xs text-brand-text-secondary font-semibold">{i18n.t('settings.themeNameLabel')}</label>
        <input
          id="theme-name"
          type="text"
          bind:value={themeName}
          placeholder={i18n.t('settings.themeNamePlaceholder')}
          class="bg-brand-main border border-brand-border rounded-lg px-3 py-2 text-xs text-brand-text-primary outline-none focus:border-brand-accent"
        />
      </div>

      <div class="flex gap-3">
        <button
          onclick={saveTheme}
          class="flex-1 bg-brand-accent hover:bg-brand-accent-hover text-brand-accent-contrast px-4 py-2 rounded-lg text-xs font-semibold flex items-center justify-center gap-2 transition-colors cursor-pointer"
        >
          {#if isEditing}
            <Check class="w-4 h-4" /> {i18n.t('settings.saveChanges')}
          {:else}
            <Plus class="w-4 h-4" /> {i18n.t('settings.saveCustom')}
          {/if}
        </button>
        <button
          onclick={exportTheme}
          class="flex-1 bg-brand-main hover:bg-brand-sidebar border border-brand-border hover:border-brand-accent/40 text-brand-text-primary px-4 py-2 rounded-lg text-xs font-semibold flex items-center justify-center gap-2 transition-colors cursor-pointer"
        >
          <Download class="w-4 h-4" /> {i18n.t('settings.exportTheme')}
        </button>
        <button
          onclick={importTheme}
          class="flex-1 bg-brand-main hover:bg-brand-sidebar border border-brand-border hover:border-brand-accent/40 text-brand-text-primary px-4 py-2 rounded-lg text-xs font-semibold flex items-center justify-center gap-2 transition-colors cursor-pointer"
        >
          <Upload class="w-4 h-4" /> {i18n.t('settings.importTheme')}
        </button>
      </div>
    </div>
  </div>

  <!-- UI Preview Section -->
  <div class="bg-brand-sidebar border border-brand-border rounded-xl p-6 space-y-5">
    <h3 class="font-bold text-sm text-brand-text-primary flex items-center gap-2">
      <Eye class="w-4 h-4" /> {i18n.t('settings.livePreview')}
    </h3>

    <div class="grid grid-cols-1 md:grid-cols-3 gap-4">
      <!-- Sidebar Preview -->
      <div class="space-y-2">
        <p class="text-xs font-semibold text-brand-text-secondary">{i18n.t('settings.sidebarPreview')}</p>
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
        <p class="text-xs font-semibold text-brand-text-secondary">{i18n.t('settings.mainViewPreview')}</p>
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
        <p class="text-xs font-semibold text-brand-text-secondary">{i18n.t('settings.playerBarPreview')}</p>
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
