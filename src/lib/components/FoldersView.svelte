<script lang="ts">
  import { collectionStore } from "../stores/collection.svelte";
  import { themeStore, PREDEFINED_THEMES, LUMINOUS_DARK_COLORS, LUMINOUS_LIGHT_COLORS, type ThemeColors, type Theme } from "../stores/theme.svelte";
  import { playerStore } from "../stores/player.svelte";
  import { Folder, Plus, Trash2, HelpCircle, Palette, Settings, Check, Wand2, RefreshCw, RotateCcw, Sparkles, Clock, Activity, HardDrive } from "lucide-svelte";
  import { open } from "@tauri-apps/plugin-dialog";
  import { i18n, type Locale } from "../stores/i18n.svelte";
  import { prefs, type RatingStyle } from "../stores/prefs.svelte";
  import { onMount } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import Equalizer from "./Equalizer.svelte";
  import DesignTools from "./DesignTools.svelte";

  let settingsTab = $state<"general" | "folders" | "themes" | "equalizer" | "formats">("general");
  let isTabInitialized = $state(false);
  let editingThemeId = $state<string | null>(null);
  let useAdvancedBuilder = $state(false);

  let pruneMsg = $state<string | null>(null);

  async function handlePruneMissing() {
    const count = await collectionStore.pruneMissing();
    pruneMsg = i18n.t('settings.pruneCompleteMsg', { count });
    setTimeout(() => { pruneMsg = null; }, 4000);
  }

  onMount(async () => {
    try {
      const settings = await invoke<Record<string, string>>("get_all_app_settings");
      if (settings && settings.active_settings_tab) {
        const savedTab = settings.active_settings_tab;
        if (savedTab === "general" || savedTab === "folders" || savedTab === "themes" || savedTab === "equalizer" || savedTab === "formats") {
          settingsTab = savedTab;
        }
      }
    } catch (e) {
      console.error("Failed to restore active_settings_tab:", e);
    } finally {
      isTabInitialized = true;
    }
  });

  $effect(() => {
    if (isTabInitialized) {
      invoke("set_app_setting", { key: "active_settings_tab", value: settingsTab }).catch((err) => {
        console.error("Failed to save active_settings_tab:", err);
      });
    }
  });

  // Folders operations
  async function handleAddDirectory() {
    try {
      const selected = await open({
        directory: true,
        multiple: false,
        title: i18n.t('settings.selectMusicDirectory'),
      });
      if (selected && typeof selected === "string") {
        await collectionStore.addDirectory(selected);
      }
    } catch (err) {
      console.error("Failed to open directory dialog:", err);
    }
  }

  async function handleRemoveDirectory(path: string) {
    if (confirm(i18n.t('settings.confirmRemoveFolder', { path }))) {
      await collectionStore.removeDirectory(path);
    }
  }

  // Custom Theme state
  let newThemeName = $state("");
  let customColors = $state<ThemeColors>({
    "bg-main": "#0d0b18",
    "bg-sidebar": "#07050e",
    "bg-playerbar": "#0a0813",
    "color-accent": "#8b5cf6",
    "color-accent-hover": "#a78bfa",
    "color-text-primary": "#f3f4f6",
    "color-text-secondary": "#9ca3af",
    "color-border": "#1f1b2e"
  });

  // The System theme's live colors depend on the OS light/dark preference,
  // not the static (dark) preview baked into its PREDEFINED_THEMES entry —
  // use the current system scheme so its swatch matches what's on screen.
  function getPreviewColors(theme: Theme): ThemeColors {
    if (theme.id === "system") {
      return themeStore.systemColorScheme === "dark" ? LUMINOUS_DARK_COLORS : LUMINOUS_LIGHT_COLORS;
    }
    return theme.colors;
  }

  function loadActiveThemeColors() {
    customColors = { ...themeStore.resolvedColors };
  }

  // Pre-fill theme builder with current active theme colors on mount and
  // updates — skipped while editing an existing custom theme, since that
  // case is seeded from the theme being edited instead (below).
  $effect(() => {
    const _themeId = themeStore.activeThemeId;
    const _songId = playerStore.currentSong?.id;
    if (!editingThemeId) {
      loadActiveThemeColors();
    }
  });

  let editingTheme = $derived(themeStore.customThemes.find(t => t.id === editingThemeId));

  // Seed the builder (Simple and Advanced share this same customColors/
  // newThemeName state) from the theme being edited whenever Edit is clicked.
  $effect(() => {
    if (editingTheme) {
      newThemeName = editingTheme.name;
      customColors = { ...editingTheme.colors };
    }
  });

  function handleLivePreview() {
    // Inject the colors live to the document head for instant feedback
    if (typeof document === "undefined") return;
    let styleEl = document.getElementById("luminous-theme-style");
    if (!styleEl) {
      styleEl = document.createElement("style");
      styleEl.id = "luminous-theme-style";
      document.head.appendChild(styleEl);
    }
    styleEl.innerHTML = `
      :root {
        --bg-main: ${customColors["bg-main"]};
        --bg-sidebar: ${customColors["bg-sidebar"]};
        --bg-playerbar: ${customColors["bg-playerbar"]};
        --color-accent: ${customColors["color-accent"]};
        --color-accent-hover: ${customColors["color-accent-hover"]};
        --color-text-primary: ${customColors["color-text-primary"]};
        --color-text-secondary: ${customColors["color-text-secondary"]};
        --color-border: ${customColors["color-border"]};
      }
    `;
  }

  async function saveCustomTheme() {
    if (newThemeName.trim() === "") {
      alert(i18n.t('settings.enterThemeNameAlert'));
      return;
    }

    if (editingThemeId) {
      await themeStore.addCustomTheme({
        id: editingThemeId,
        name: newThemeName.trim(),
        colors: { ...customColors },
        isCustom: true
      });
      editingThemeId = null;
    } else {
      const id = "custom-" + newThemeName.toLowerCase().replace(/[^a-z0-9]/g, "-");
      await themeStore.addCustomTheme({
        id,
        name: newThemeName.trim(),
        colors: { ...customColors },
        isCustom: true
      });
      newThemeName = "";
    }
  }
</script>

<div class="flex-1 flex flex-col overflow-hidden bg-brand-main text-brand-text-secondary h-full">
  <!-- Top Header bar -->
  <div class="h-16 px-6 border-b border-brand-border flex items-center justify-between">
    <div class="flex items-center gap-3">
      <Settings class="w-5 h-5 text-brand-accent-text" />
      <h2 class="text-base font-bold text-brand-text-primary">{i18n.t('settings.title')}</h2>
    </div>

    <!-- Sleek Tab Control -->
    <div class="flex bg-brand-sidebar border border-brand-border rounded-xl p-0.5 text-xs shadow-sm">
      <button
        onclick={() => { settingsTab = "general"; }}
        class="px-4 py-1.5 rounded-lg font-semibold transition-all cursor-pointer {settingsTab === 'general' ? 'bg-brand-accent text-brand-accent-contrast shadow-md' : 'text-brand-text-secondary hover:text-brand-text-primary'}"
      >
        {i18n.t('settings.tabGeneral')}
      </button>
      <button
        onclick={() => { settingsTab = "folders"; }}
        class="px-4 py-1.5 rounded-lg font-semibold transition-all cursor-pointer {settingsTab === 'folders' ? 'bg-brand-accent text-brand-accent-contrast shadow-md' : 'text-brand-text-secondary hover:text-brand-text-primary'}"
      >
        {i18n.t('settings.tabFolders')}
      </button>
      <button
        onclick={() => { settingsTab = "themes"; editingThemeId = null; }}
        class="px-4 py-1.5 rounded-lg font-semibold transition-all cursor-pointer {settingsTab === 'themes' ? 'bg-brand-accent text-brand-accent-contrast shadow-md' : 'text-brand-text-secondary hover:text-brand-text-primary'}"
      >
        {i18n.t('settings.tabThemes')}
      </button>
      <button
        onclick={() => { settingsTab = "equalizer"; }}
        class="px-4 py-1.5 rounded-lg font-semibold transition-all cursor-pointer {settingsTab === 'equalizer' ? 'bg-brand-accent text-brand-accent-contrast shadow-md' : 'text-brand-text-secondary hover:text-brand-text-primary'}"
      >
        {i18n.t('settings.tabEqualizer')}
      </button>
      <button
        onclick={() => { settingsTab = "formats"; }}
        class="px-4 py-1.5 rounded-lg font-semibold transition-all cursor-pointer {settingsTab === 'formats' ? 'bg-brand-accent text-brand-accent-contrast shadow-md' : 'text-brand-text-secondary hover:text-brand-text-primary'}"
      >
        {i18n.t('settings.tabFormats')}
      </button>
    </div>
  </div>

  <!-- Content Area -->
  <div class="flex-1 overflow-y-auto p-6 space-y-6" class:pb-24={playerStore.hasEverPlayed}>
    {#if settingsTab === "general"}
      <!-- General Settings Section -->
      <div class="bg-brand-sidebar border border-brand-border rounded-xl p-6">
        <h3 class="text-xs text-brand-text-secondary font-bold tracking-wider uppercase mb-1">{i18n.t('settings.generalTitle')}</h3>

        <!-- Language row -->
        <div class="flex items-center justify-between gap-4 py-4 border-b border-brand-border/50">
          <label for="language-select" class="text-sm font-medium text-brand-text-primary">{i18n.t('settings.selectLanguage')}</label>
          <select
            id="language-select"
            value={i18n.currentLocale}
            onchange={(e) => i18n.setLocale(e.currentTarget.value as Locale)}
            class="shrink-0 bg-brand-main hover:bg-brand-sidebar border border-brand-border text-brand-text-primary text-xs rounded-lg pl-2.5 pr-8 py-1.5 focus:outline-none focus:border-brand-accent transition-all cursor-pointer font-medium appearance-none -webkit-appearance-none"
            style="background-image: url(&quot;data:image/svg+xml;charset=utf-8,%3Csvg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 20 20' fill='none'%3E%3Cpath stroke='%239ca3af' stroke-linecap='round' stroke-linejoin='round' stroke-width='1.5' d='m6 8 4 4 4-4'/%3E%3C/svg%3E&quot;); background-position: right 0.625rem center; background-repeat: no-repeat; background-size: 1.25em;"
          >
            <option value="en">{i18n.t('settings.languageEnglish')}</option>
            <option value="fr">{i18n.t('settings.languageFrench')}</option>
          </select>
        </div>

        <!-- Rating style row -->
        <div class="flex items-center justify-between gap-4 py-4">
          <div class="flex flex-col gap-0.5 min-w-0">
            <label for="rating-style-select" class="text-sm font-medium text-brand-text-primary">{i18n.t('settings.ratingStyle')}</label>
            <p class="text-xs text-brand-text-secondary">{i18n.t('settings.ratingStyleHint')}</p>
          </div>
          <select
            id="rating-style-select"
            value={prefs.ratingStyle}
            onchange={(e) => prefs.setRatingStyle(e.currentTarget.value as RatingStyle)}
            class="shrink-0 bg-brand-main hover:bg-brand-sidebar border border-brand-border text-brand-text-primary text-xs rounded-lg pl-2.5 pr-8 py-1.5 focus:outline-none focus:border-brand-accent transition-all cursor-pointer font-medium appearance-none -webkit-appearance-none"
            style="background-image: url(&quot;data:image/svg+xml;charset=utf-8,%3Csvg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 20 20' fill='none'%3E%3Cpath stroke='%239ca3af' stroke-linecap='round' stroke-linejoin='round' stroke-width='1.5' d='m6 8 4 4 4-4'/%3E%3C/svg%3E&quot;); background-position: right 0.625rem center; background-repeat: no-repeat; background-size: 1.25em;"
          >
            <option value="heart">{i18n.t('settings.ratingStyleHeart')}</option>
            <option value="stars">{i18n.t('settings.ratingStyleStars')}</option>
          </select>
        </div>
      </div>
    {:else if settingsTab === "folders"}
      <!-- Watched Folders Section -->
      <div class="flex justify-between items-center">
        <h3 class="text-xs text-brand-text-secondary font-bold tracking-wider uppercase">{i18n.t('settings.tabFolders')}</h3>
        <button
          onclick={handleAddDirectory}
          class="bg-brand-accent hover:bg-brand-accent-hover text-brand-accent-contrast px-3.5 py-1.5 rounded-lg text-xs font-semibold flex items-center gap-1.5 transition-colors shadow-lg shadow-brand-accent/20 cursor-pointer"
        >
          <Plus class="w-4 h-4" /> {i18n.t('settings.addFolder')}
        </button>
      </div>

      <!-- Info Banner -->
      <div class="bg-brand-accent/5 border border-brand-accent/20 rounded-xl p-4 flex gap-3.5 text-sm text-brand-text-secondary">
        <HelpCircle class="w-5 h-5 text-brand-accent-text shrink-0 mt-0.5" />
        <div class="space-y-1">
          <h4 class="font-semibold text-brand-text-primary">{i18n.t('settings.folderHelpTitle')}</h4>
          <p class="text-xs text-brand-text-secondary/70 leading-relaxed">
            {i18n.t('settings.folderHelpText')}
          </p>
        </div>
      </div>

      <!-- Folders List -->
      <div class="space-y-2">
        {#each collectionStore.directories as dir}
          <div class="flex items-center justify-between bg-brand-sidebar/40 border border-brand-border rounded-xl p-4 hover:border-brand-border/80 transition-colors">
            <div class="flex items-center gap-3.5 min-w-0">
              <div class="w-10 h-10 rounded-lg bg-brand-main border border-brand-border flex items-center justify-center text-brand-accent-text">
                <Folder class="w-5 h-5" />
              </div>
              <div class="min-w-0">
                <p class="text-sm font-medium text-brand-text-primary truncate" title={dir.path}>{dir.path}</p>
                <p class="text-[10px] text-brand-text-secondary/50 mt-0.5">{i18n.t('settings.folderItemRecursive')}</p>
              </div>
            </div>
            <button
              onclick={() => handleRemoveDirectory(dir.path)}
              class="p-2 rounded-lg bg-brand-main hover:bg-red-950/20 text-brand-text-secondary hover:text-red-400 border border-brand-border hover:border-red-900/30 transition-colors cursor-pointer"
              title={i18n.t('settings.folderItemStopWatch')}
            >
              <Trash2 class="w-4 h-4" />
            </button>
          </div>
        {/each}

        {#if collectionStore.directories.length === 0}
          <div class="border border-dashed border-brand-border rounded-xl py-12 text-center text-brand-text-secondary/60">
            <Folder class="w-12 h-12 mx-auto mb-2 text-brand-text-secondary/30" />
            <h4 class="font-semibold text-brand-text-primary mb-1">{i18n.t('settings.noFoldersTitle')}</h4>
            <p class="text-xs text-brand-text-secondary/60 mb-4">{i18n.t('settings.noFoldersText')}</p>
          </div>
        {/if}
      </div>

      <!-- Library Scanning & Maintenance Section -->
      <div class="bg-brand-sidebar border border-brand-border rounded-xl p-6 space-y-5">
        <div class="border-b border-brand-border pb-3">
          <div class="flex items-start gap-3">
            <RefreshCw class="w-5 h-5 text-brand-accent-text mt-0.5 shrink-0 {collectionStore.isScanning ? 'animate-spin' : ''}" />
            <div class="space-y-1 min-w-0">
              <h4 class="font-bold text-sm text-brand-text-primary">{i18n.t('settings.rescanTitle')}</h4>
              <p class="text-xs text-brand-text-secondary/70 leading-relaxed">{i18n.t('settings.rescanSubtitle')}</p>
            </div>
          </div>
        </div>

        <!-- Scan Status & Progress Bar (when scanning) -->
        {#if collectionStore.isScanning}
          <div class="bg-brand-main/60 border border-brand-accent/30 rounded-xl p-4 space-y-2">
            <div class="flex justify-between items-center text-xs font-semibold text-brand-text-primary">
              <span class="capitalize flex items-center gap-2">
                <RefreshCw class="w-4 h-4 animate-spin text-brand-accent-text" />
                {i18n.t('settings.scanningPhase', { phase: collectionStore.scanProgress?.phase || i18n.t('sidebar.scanning') })}
              </span>
              <span>{collectionStore.scanProgress?.scanned || 0} / {collectionStore.scanProgress?.total || 0}</span>
            </div>
            <div class="w-full bg-brand-sidebar rounded-full h-2 overflow-hidden border border-brand-border/40">
              <div
                class="bg-brand-accent h-2 rounded-full transition-all duration-300"
                style="width: {collectionStore.scanProgress?.total ? (collectionStore.scanProgress.scanned / collectionStore.scanProgress.total) * 100 : 0}%"
              ></div>
            </div>
            <p class="text-[10px] text-brand-text-secondary/60 truncate">{collectionStore.scanProgress?.current_path || ""}</p>
          </div>
        {/if}

        <!-- Manual Action Buttons -->
        <div class="flex flex-wrap items-center gap-3">
          <button
            onclick={() => collectionStore.startScan(false)}
            disabled={collectionStore.isScanning}
            class="bg-brand-accent hover:bg-brand-accent-hover text-brand-accent-contrast px-4 py-2 rounded-lg text-xs font-semibold flex items-center gap-2 transition-all shadow-md shadow-brand-accent/10 cursor-pointer disabled:opacity-50"
          >
            <RefreshCw class="w-4 h-4 {collectionStore.isScanning ? 'animate-spin' : ''}" />
            {i18n.t('settings.incrementalRescanBtn')}
          </button>

          <button
            onclick={() => collectionStore.startScan(true)}
            disabled={collectionStore.isScanning}
            class="bg-brand-main hover:bg-brand-sidebar border border-brand-border hover:border-brand-accent/40 text-brand-text-primary px-4 py-2 rounded-lg text-xs font-semibold flex items-center gap-2 transition-colors cursor-pointer disabled:opacity-50"
            title={i18n.t('settings.forceFullScanHint')}
          >
            <RotateCcw class="w-4 h-4 text-brand-accent-text" />
            {i18n.t('settings.forceFullScanBtn')}
          </button>

          <button
            onclick={handlePruneMissing}
            disabled={collectionStore.isScanning}
            class="bg-brand-main hover:bg-red-950/20 text-brand-text-secondary hover:text-red-400 border border-brand-border hover:border-red-900/30 px-4 py-2 rounded-lg text-xs font-semibold flex items-center gap-2 transition-colors cursor-pointer disabled:opacity-50"
            title={i18n.t('settings.pruneMissingHint')}
          >
            <Sparkles class="w-4 h-4" />
            {i18n.t('settings.pruneMissingBtn')}
          </button>

          {#if pruneMsg}
            <span class="text-xs text-brand-accent-text font-medium transition-all">{pruneMsg}</span>
          {/if}
        </div>

        <!-- Configuration Toggles -->
        <div class="pt-3 border-t border-brand-border/50 space-y-4">
          <!-- Real-time Folder Watching -->
          <div class="flex items-center justify-between gap-4">
            <div class="flex flex-col gap-0.5 min-w-0">
              <span class="text-sm font-medium text-brand-text-primary">{i18n.t('settings.watchRealtimeLabel')}</span>
              <p class="text-xs text-brand-text-secondary/70">{i18n.t('settings.watchRealtimeHint')}</p>
            </div>
            <div class="flex items-center gap-2 shrink-0">
              <span class="text-xs font-medium text-brand-text-secondary w-6">
                {collectionStore.watchFoldersRealtime ? i18n.t('common.on') : i18n.t('common.off')}
              </span>
              <button
                type="button"
                role="switch"
                aria-checked={collectionStore.watchFoldersRealtime}
                aria-label={i18n.t('settings.watchRealtimeLabel')}
                onclick={() => collectionStore.setWatchFoldersRealtime(!collectionStore.watchFoldersRealtime)}
                class="relative inline-flex h-6 w-11 items-center rounded-full transition-colors cursor-pointer {collectionStore.watchFoldersRealtime ? 'bg-brand-accent' : 'bg-brand-border'}"
              >
                <span class="inline-block h-4 w-4 transform rounded-full bg-white shadow transition-transform {collectionStore.watchFoldersRealtime ? 'translate-x-6' : 'translate-x-1'}"></span>
              </button>
            </div>
          </div>

          <!-- Rescan on Startup -->
          <div class="flex items-center justify-between gap-4">
            <div class="flex flex-col gap-0.5 min-w-0">
              <span class="text-sm font-medium text-brand-text-primary">{i18n.t('settings.scanOnStartupLabel')}</span>
              <p class="text-xs text-brand-text-secondary/70">{i18n.t('settings.scanOnStartupHint')}</p>
            </div>
            <div class="flex items-center gap-2 shrink-0">
              <span class="text-xs font-medium text-brand-text-secondary w-6">
                {collectionStore.scanOnStartup ? i18n.t('common.on') : i18n.t('common.off')}
              </span>
              <button
                type="button"
                role="switch"
                aria-checked={collectionStore.scanOnStartup}
                aria-label={i18n.t('settings.scanOnStartupLabel')}
                onclick={() => collectionStore.setScanOnStartup(!collectionStore.scanOnStartup)}
                class="relative inline-flex h-6 w-11 items-center rounded-full transition-colors cursor-pointer {collectionStore.scanOnStartup ? 'bg-brand-accent' : 'bg-brand-border'}"
              >
                <span class="inline-block h-4 w-4 transform rounded-full bg-white shadow transition-transform {collectionStore.scanOnStartup ? 'translate-x-6' : 'translate-x-1'}"></span>
              </button>
            </div>
          </div>
        </div>

        <!-- Library Stats & Last Scan Summary -->
        <div class="pt-3 border-t border-brand-border/50 space-y-3">
          {#if collectionStore.lastScanTime}
            <div class="text-xs text-brand-text-secondary/70 flex items-center justify-between font-medium">
              <span class="flex items-center gap-1.5">
                <Clock class="w-3.5 h-3.5 text-brand-accent-text shrink-0" />
                {i18n.t('settings.lastScanned', { time: collectionStore.lastScanTime })}
              </span>
            </div>
          {/if}

          <div class="grid grid-cols-2 md:grid-cols-4 gap-4 text-xs">
          <div class="bg-brand-main/40 border border-brand-border rounded-lg p-3">
            <span class="text-[10px] text-brand-text-secondary/60 uppercase font-semibold">{i18n.t('settings.statsSongs')}</span>
            <p class="text-base font-bold text-brand-text-primary mt-0.5">{collectionStore.stats.total_songs.toLocaleString()}</p>
          </div>
          <div class="bg-brand-main/40 border border-brand-border rounded-lg p-3">
            <span class="text-[10px] text-brand-text-secondary/60 uppercase font-semibold">{i18n.t('settings.statsAlbums')}</span>
            <p class="text-base font-bold text-brand-text-primary mt-0.5">{collectionStore.stats.total_albums.toLocaleString()}</p>
          </div>
          <div class="bg-brand-main/40 border border-brand-border rounded-lg p-3">
            <span class="text-[10px] text-brand-text-secondary/60 uppercase font-semibold">{i18n.t('settings.statsArtists')}</span>
            <p class="text-base font-bold text-brand-text-primary mt-0.5">{collectionStore.stats.total_artists.toLocaleString()}</p>
          </div>
          <div class="bg-brand-main/40 border border-brand-border rounded-lg p-3">
            <span class="text-[10px] text-brand-text-secondary/60 uppercase font-semibold">{i18n.t('settings.statsSize')}</span>
            <p class="text-base font-bold text-brand-text-primary mt-0.5">{(collectionStore.stats.total_filesize_bytes / (1024 * 1024 * 1024)).toFixed(2)} GB</p>
          </div>
        </div>
      </div>
      </div>
    {:else if settingsTab === "themes"}
      <!-- UI Themes Section -->
      <div class="space-y-6">
        <div>
          <h3 class="text-xs text-brand-text-secondary font-bold tracking-wider uppercase mb-3">{i18n.t('settings.predefinedThemes')}</h3>
          <div class="grid grid-cols-1 md:grid-cols-3 lg:grid-cols-5 gap-4">
            {#each PREDEFINED_THEMES as theme}
              {@const previewColors = getPreviewColors(theme)}
              <button
                onclick={() => themeStore.setTheme(theme.id)}
                class="bg-brand-sidebar/40 border rounded-xl p-4 flex flex-col items-start gap-3 text-left transition-all duration-200 group hover:border-brand-accent/40 cursor-pointer w-full relative {themeStore.activeThemeId === theme.id ? 'border-2 border-brand-accent shadow-md shadow-brand-accent/5' : 'border-brand-border'}"
              >
                <div class="flex items-center justify-between w-full">
                  <span class="font-semibold text-sm text-brand-text-primary">{theme.isCustom ? theme.name : i18n.t('themes.' + theme.id, {}, theme.name)}</span>
                </div>
                <!-- Miniature colors preview -->
                <div class="flex gap-1 w-full h-8 rounded-lg overflow-hidden border border-brand-border/40 bg-black/10">
                  <div class="w-[30%]" style="background-color: {previewColors['bg-sidebar']}" title={i18n.t('settings.sidebarLabel')}></div>
                  <div class="w-[50%]" style="background-color: {previewColors['bg-main']}" title={i18n.t('settings.mainViewLabel')}></div>
                  <div class="w-[10%]" style="background-color: {previewColors['bg-playerbar']}" title={i18n.t('settings.playerBarLabel')}></div>
                  <div class="w-[10%]" style="background-color: {previewColors['color-accent']}" title={i18n.t('settings.accentLabel')}></div>
                </div>
              </button>
            {/each}
          </div>
        </div>

        {#if themeStore.customThemes.length > 0}
          <div>
            <h3 class="text-xs text-brand-text-secondary font-bold tracking-wider uppercase mb-3">{i18n.t('settings.customThemes')}</h3>
            <div class="grid grid-cols-1 md:grid-cols-3 lg:grid-cols-5 gap-4">
              {#each themeStore.customThemes as theme}
                <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
                <!-- svelte-ignore a11y_click_events_have_key_events -->
                <div
                  onclick={() => themeStore.setTheme(theme.id)}
                  role="button"
                  tabindex="0"
                  class="bg-brand-sidebar/40 border rounded-xl p-4 flex flex-col gap-3 text-left transition-colors cursor-pointer w-full {themeStore.activeThemeId === theme.id ? 'border-2 border-brand-accent shadow-md shadow-brand-accent/5' : 'border-brand-border hover:border-brand-border/80'}"
                >
                  <div class="flex items-center justify-between w-full">
                    <span class="font-semibold text-sm text-brand-text-primary truncate">{theme.name}</span>
                  </div>
                  <div class="flex gap-1 w-full h-8 rounded-lg overflow-hidden border border-brand-border/40 bg-black/10">
                    <div class="w-[30%]" style="background-color: {theme.colors['bg-sidebar']}" title={i18n.t('settings.sidebarLabel')}></div>
                    <div class="w-[50%]" style="background-color: {theme.colors['bg-main']}" title={i18n.t('settings.mainViewLabel')}></div>
                    <div class="w-[10%]" style="background-color: {theme.colors['bg-playerbar']}" title={i18n.t('settings.playerBarLabel')}></div>
                    <div class="w-[10%]" style="background-color: {theme.colors['color-accent']}" title={i18n.t('settings.accentLabel')}></div>
                  </div>
                  <div class="flex gap-2">
                    <button
                      onclick={(e) => { e.stopPropagation(); editingThemeId = theme.id; }}
                      class="flex-1 px-2 py-1 rounded text-xs font-semibold bg-brand-accent hover:bg-brand-accent-hover text-brand-accent-contrast transition-colors cursor-pointer"
                      title={i18n.t('settings.editTheme')}
                    >
                      {i18n.t('settings.editTheme').split(' ')[0]}
                    </button>
                    <button
                      onclick={(e) => { e.stopPropagation(); themeStore.deleteCustomTheme(theme.id); }}
                      class="p-1 rounded bg-brand-main hover:bg-red-950/20 text-brand-text-secondary hover:text-red-400 border border-brand-border hover:border-red-900/30 transition-colors cursor-pointer"
                      title={i18n.t('settings.deleteTheme')}
                    >
                      <Trash2 class="w-3.5 h-3.5" />
                    </button>
                  </div>
                </div>
              {/each}
            </div>
          </div>
        {/if}

        <!-- Custom Theme Builder Form -->
        <div class="bg-brand-sidebar border border-brand-border rounded-xl p-6 space-y-5">
          <div class="flex items-center justify-between border-b border-brand-border pb-3">
            <div class="flex items-center gap-2">
              <Palette class="w-5 h-5 text-brand-accent-text" />
              <h4 class="font-bold text-sm text-brand-text-primary">
                {editingTheme ? i18n.t('settings.editingThemeTitle', { name: editingTheme.name }) : i18n.t('settings.customThemeBuilderTitle')}
              </h4>
            </div>
            <div class="flex items-center gap-2">
              {#if editingThemeId}
                <button
                  onclick={() => { editingThemeId = null; }}
                  class="text-xs text-brand-text-secondary hover:text-brand-text-primary px-3 py-1.5 rounded-lg border border-brand-border hover:border-brand-accent/40 transition-colors cursor-pointer"
                >
                  {i18n.t('settings.cancel')}
                </button>
              {/if}
              <!-- Simple/Advanced Toggle -->
              <div class="flex items-center gap-2 bg-brand-main rounded-full p-0.5 border border-brand-border">
                <button
                  onclick={() => { useAdvancedBuilder = false; }}
                  class="px-3 py-1.5 rounded-full text-xs font-semibold transition-all {!useAdvancedBuilder ? 'bg-brand-accent text-brand-accent-contrast' : 'text-brand-text-secondary hover:text-brand-text-primary'}"
                >
                  {i18n.t('settings.simple')}
                </button>
                <button
                  onclick={() => { useAdvancedBuilder = true; }}
                  class="px-3 py-1.5 rounded-full text-xs font-semibold transition-all {useAdvancedBuilder ? 'bg-brand-accent text-brand-accent-contrast' : 'text-brand-text-secondary hover:text-brand-text-primary'}"
                >
                  {i18n.t('settings.advanced')}
                </button>
              </div>
            </div>
          </div>

          {#if useAdvancedBuilder}
            <DesignTools {customColors} {newThemeName} themeId={editingThemeId} onSaved={() => { editingThemeId = null; }} />
          {:else}
            <div class="space-y-5">
              <div class="flex flex-col md:flex-row gap-4 items-end justify-between">
                <div class="flex flex-col gap-1.5 flex-1 max-w-sm">
                  <label for="theme-name-input" class="text-xs text-brand-text-secondary font-semibold">{i18n.t('settings.themeNameLabel')}</label>
                  <input
                    id="theme-name-input"
                    type="text"
                    bind:value={newThemeName}
                    placeholder={i18n.t('settings.themeNamePlaceholder')}
                    class="bg-brand-main border border-brand-border rounded-lg px-3 py-2 text-xs text-brand-text-primary outline-none focus:border-brand-accent w-full"
                  />
                </div>
                <button
                  onclick={loadActiveThemeColors}
                  class="bg-brand-main hover:bg-brand-sidebar border border-brand-border hover:border-brand-accent/40 text-brand-text-primary px-4 py-2 rounded-lg text-xs font-semibold flex items-center gap-1.5 transition-colors cursor-pointer shrink-0 h-9"
                >
                  <Palette class="w-4 h-4 text-brand-accent-text" /> {i18n.t('settings.importColors')}
                </button>
              </div>

              <div class="grid grid-cols-2 md:grid-cols-4 gap-6 pt-2">
                <!-- Main Background -->
                <div class="flex items-center gap-3">
                  <input type="color" bind:value={customColors['bg-main']} oninput={handleLivePreview} class="w-9 h-9 rounded border border-brand-border cursor-pointer bg-transparent shrink-0" />
                  <div class="flex flex-col">
                    <span class="text-xs font-semibold text-brand-text-primary">{i18n.t('settings.mainViewLabel')}</span>
                    <span class="text-[10px] text-brand-text-secondary/50">bg-main</span>
                  </div>
                </div>

                <!-- Sidebar Background -->
                <div class="flex items-center gap-3">
                  <input type="color" bind:value={customColors['bg-sidebar']} oninput={handleLivePreview} class="w-9 h-9 rounded border border-brand-border cursor-pointer bg-transparent shrink-0" />
                  <div class="flex flex-col">
                    <span class="text-xs font-semibold text-brand-text-primary">{i18n.t('settings.sidebarLabel')}</span>
                    <span class="text-[10px] text-brand-text-secondary/50">bg-sidebar</span>
                  </div>
                </div>

                <!-- Player Bar Background -->
                <div class="flex items-center gap-3">
                  <input type="color" bind:value={customColors['bg-playerbar']} oninput={handleLivePreview} class="w-9 h-9 rounded border border-brand-border cursor-pointer bg-transparent shrink-0" />
                  <div class="flex flex-col">
                    <span class="text-xs font-semibold text-brand-text-primary">{i18n.t('settings.playerBarLabel')}</span>
                    <span class="text-[10px] text-brand-text-secondary/50">bg-playerbar</span>
                  </div>
                </div>

                <!-- Accent Color -->
                <div class="flex items-center gap-3">
                  <input type="color" bind:value={customColors['color-accent']} oninput={handleLivePreview} class="w-9 h-9 rounded border border-brand-border cursor-pointer bg-transparent shrink-0" />
                  <div class="flex flex-col">
                    <span class="text-xs font-semibold text-brand-text-primary">{i18n.t('settings.accentLabel')}</span>
                    <span class="text-[10px] text-brand-text-secondary/50">color-accent</span>
                  </div>
                </div>

                <!-- Accent Hover Color -->
                <div class="flex items-center gap-3">
                  <input type="color" bind:value={customColors['color-accent-hover']} oninput={handleLivePreview} class="w-9 h-9 rounded border border-brand-border cursor-pointer bg-transparent shrink-0" />
                  <div class="flex flex-col">
                    <span class="text-xs font-semibold text-brand-text-primary">{i18n.t('settings.accentHoverLabel')}</span>
                    <span class="text-[10px] text-brand-text-secondary/50">accent-hover</span>
                  </div>
                </div>

                <!-- Primary Text -->
                <div class="flex items-center gap-3">
                  <input type="color" bind:value={customColors['color-text-primary']} oninput={handleLivePreview} class="w-9 h-9 rounded border border-brand-border cursor-pointer bg-transparent shrink-0" />
                  <div class="flex flex-col">
                    <span class="text-xs font-semibold text-brand-text-primary">{i18n.t('settings.primaryTextLabel')}</span>
                    <span class="text-[10px] text-brand-text-secondary/50">text-primary</span>
                  </div>
                </div>

                <!-- Secondary Text -->
                <div class="flex items-center gap-3">
                  <input type="color" bind:value={customColors['color-text-secondary']} oninput={handleLivePreview} class="w-9 h-9 rounded border border-brand-border cursor-pointer bg-transparent shrink-0" />
                  <div class="flex flex-col">
                    <span class="text-xs font-semibold text-brand-text-primary">{i18n.t('settings.secondaryTextLabel')}</span>
                    <span class="text-[10px] text-brand-text-secondary/50">text-secondary</span>
                  </div>
                </div>

                <!-- Border Color -->
                <div class="flex items-center gap-3">
                  <input type="color" bind:value={customColors['color-border']} oninput={handleLivePreview} class="w-9 h-9 rounded border border-brand-border cursor-pointer bg-transparent shrink-0" />
                  <div class="flex flex-col">
                    <span class="text-xs font-semibold text-brand-text-primary">{i18n.t('settings.bordersLabel')}</span>
                    <span class="text-[10px] text-brand-text-secondary/50">color-border</span>
                  </div>
                </div>
              </div>

              <div class="flex items-center gap-3 pt-3 border-t border-brand-border">
                <button
                  onclick={saveCustomTheme}
                  class="bg-brand-accent hover:bg-brand-accent-hover text-brand-accent-contrast px-4 py-2 rounded-lg text-xs font-semibold transition-all shadow-md shadow-brand-accent/10 cursor-pointer"
                >
                  {editingThemeId ? i18n.t('settings.saveChanges') : i18n.t('settings.saveCustom')}
                </button>
                <span class="text-[10px] text-brand-text-secondary/50">{i18n.t('settings.livePreviewInfo')}</span>
              </div>
            </div>
          {/if}
        </div>
      </div>
    {:else if settingsTab === "equalizer"}
      <!-- Equalizer Section -->
      <div class="space-y-6">
        <Equalizer />
      </div>
    {:else if settingsTab === "formats"}
      <!-- File Formats Filter Section -->
      <div class="space-y-6">
        <div>
          <h3 class="text-xs text-brand-text-secondary font-bold tracking-wider uppercase mb-1">{i18n.t('settings.formatsTitle')}</h3>
          <p class="text-xs text-brand-text-secondary/60 mb-4">{i18n.t('settings.formatsSubtitle')}</p>
          
          <div class="grid grid-cols-2 md:grid-cols-4 gap-4">
            {#each ["MP3", "FLAC", "WAV", "AAC", "ALAC", "OGG", "AIFF", "APE"] as format}
              {@const isChecked = !collectionStore.excludedFormats.includes(format)}
              <button
                onclick={() => collectionStore.toggleFormat(format)}
                class="bg-brand-sidebar/40 border rounded-xl p-4 flex items-center justify-between transition-all duration-200 hover:border-brand-accent/40 cursor-pointer w-full text-left {isChecked ? 'border-brand-accent bg-brand-sidebar/60' : 'border-brand-border'}"
              >
                <div class="flex flex-col">
                  <span class="font-semibold text-sm text-brand-text-primary">{format}</span>
                  <span class="text-[10px] text-brand-text-secondary/50 mt-0.5">
                    {isChecked ? i18n.t('settings.enabled') : i18n.t('settings.excluded')}
                  </span>
                </div>
                <div class="w-5 h-5 rounded border flex items-center justify-center transition-colors {isChecked ? 'bg-brand-accent border-brand-accent text-brand-accent-contrast' : 'border-brand-border bg-black/10'}">
                  {#if isChecked}
                    <Check class="w-3 h-3 stroke-[3]" />
                  {/if}
                </div>
              </button>
            {/each}
          </div>
        </div>
      </div>
    {/if}
  </div>
</div>
