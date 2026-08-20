<script lang="ts">
  import { collectionStore } from "../stores/collection.svelte";
  import { themeStore, PREDEFINED_THEMES, LUMINOUS_DARK_COLORS, LUMINOUS_LIGHT_COLORS, type ThemeColors, type Theme } from "../stores/theme.svelte";
  import { playerStore } from "../stores/player.svelte";
  import { Folder, Plus, Trash2, Palette, Settings, Check, Wand2, RefreshCw, RotateCcw, Sparkles, Eraser, Clock, Activity, HardDrive, Info, Shield, Sun, Moon, ArrowUp, Download, Eye, EyeOff, AlertTriangle, Home, MessageCircle, Bug, Package } from "lucide-svelte";
  import { i18n, type Locale } from "../stores/i18n.svelte";
  import { toastStore } from "../stores/toast.svelte";
  import { prefs, type RatingStyle } from "../stores/prefs.svelte";
  import { updaterStore, MICROSOFT_STORE_URL } from "../stores/updater.svelte";
  import { loudnessStore } from "../stores/loudness.svelte";
  import { onMount } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import Equalizer from "./Equalizer.svelte";
  import OrganizeFiles from "./OrganizeFiles.svelte";
  import Toggle from "./Toggle.svelte";
  import Select from "./Select.svelte";
  import Button from "./Button.svelte";
  import Input from "./Input.svelte";
  import { rememberScroll } from "../utils/scrollMemory";

  const COPY_FEEDBACK_DURATION_MS = 1500;
  const PRUNE_MESSAGE_DURATION_MS = 8000;

  let settingsTab = $state<"general" | "folders" | "tools" | "themes" | "equalizer" | "about">("general");
  let appVersion = $state("");
  let versionCopied = $state(false);
  let contentEl = $state<HTMLDivElement | null>(null);

  // "Import finished"-style success flash for a manual "Check Now" that
  // comes back up-to-date — only on the checking -> up-to-date transition
  // this view is mounted to watch, not on a silent background check.
  let previousCheckStatus: typeof updaterStore.checkStatus | undefined;
  let justConfirmedUpToDate = $state(false);
  let downloadPercent = $derived(
    updaterStore.downloadProgress?.total
      ? Math.min(100, Math.round((updaterStore.downloadProgress.downloaded / updaterStore.downloadProgress.total) * 100))
      : null
  );

  // Ticks every 30s so the "checked N minutes ago" text stays current without a manual refresh.
  const CHECKED_AGO_TICK_MS = 30_000;
  let checkedAgoTick = $state(0);
  $effect(() => {
    const interval = setInterval(() => { checkedAgoTick++; }, CHECKED_AGO_TICK_MS);
    return () => clearInterval(interval);
  });

  let checkedAgoText = $derived.by(() => {
    checkedAgoTick;
    if (!updaterStore.lastCheckedAt) return "";
    const diffMinutes = Math.floor((Date.now() - updaterStore.lastCheckedAt) / 60_000);
    let relative: string;
    if (diffMinutes < 1) {
      relative = i18n.t("playlists.relativeJustNow");
    } else if (diffMinutes < 60) {
      relative = diffMinutes === 1
        ? i18n.t("playlists.relativeOneMinuteAgo")
        : i18n.t("playlists.relativeMinutesAgo", { count: diffMinutes });
    } else {
      const diffHours = Math.floor(diffMinutes / 60);
      relative = diffHours === 1
        ? i18n.t("playlists.relativeOneHourAgo")
        : i18n.t("playlists.relativeHoursAgo", { count: diffHours });
    }
    // The playlists.* relative-time strings are capitalized for standalone use (e.g. a
    // Date Added column); lowercase the leading letter here since we're splicing it mid-sentence.
    const midSentence = relative.charAt(0).toLocaleLowerCase() + relative.slice(1);
    return i18n.t("settings.updateLastChecked", { time: midSentence });
  });

  let versionOnly = $derived(appVersion.split("#")[0] ?? "");
  let buildHash = $derived(appVersion.includes("#") ? appVersion.split("#")[1] : "");

  let updateHeaderSubtitle = $derived.by(() => {
    const parts: string[] = [];
    if (versionOnly) parts.push(`v${versionOnly}`);
    if (buildHash) parts.push(i18n.t("settings.updateBuildLabel", { hash: buildHash }));
    if (checkedAgoText) parts.push(checkedAgoText);
    return parts.join(" · ");
  });

  let updateHeaderTitle = $derived.by(() => {
    if (!updaterStore.updateCheckEnabled) return i18n.t("settings.updateChecksDisabledTitle");
    switch (updaterStore.checkStatus) {
      case "checking": return i18n.t("settings.updateCheckingTitle");
      case "error": return i18n.t("settings.updateError");
      case "available": return i18n.t("settings.updateAvailableTitle", { version: updaterStore.latestVersion });
      case "up-to-date": return i18n.t("settings.updateUpToDate");
      default: return i18n.t("settings.appAndUpdatesTitle");
    }
  });

  const UPDATE_POLICIES: Array<{ id: "never" | "notify" | "auto"; labelKey: string; hintKey: string }> = [
    { id: "never", labelKey: "settings.updatePolicyNever", hintKey: "settings.updatePolicyNeverHint" },
    { id: "notify", labelKey: "settings.updatePolicyNotify", hintKey: "settings.updatePolicyNotifyHint" },
    { id: "auto", labelKey: "settings.updatePolicyAuto", hintKey: "settings.updatePolicyAutoHint" },
  ];

  $effect(() => {
    const wasChecking = previousCheckStatus === "checking";
    previousCheckStatus = updaterStore.checkStatus;
    if (updaterStore.checkStatus === "up-to-date" && wasChecking) {
      justConfirmedUpToDate = true;
      const timeout = setTimeout(() => { justConfirmedUpToDate = false; }, 320);
      return () => clearTimeout(timeout);
    }
  });

  async function copyVersion() {
    try {
      await navigator.clipboard.writeText(appVersion);
      versionCopied = true;
      setTimeout(() => { versionCopied = false; }, COPY_FEEDBACK_DURATION_MS);
    } catch (e) {
      console.error("Failed to copy version to clipboard:", e);
    }
  }

  async function openExternalUrl(url: string) {
    try {
      const { openUrl } = await import("@tauri-apps/plugin-opener");
      await openUrl(url);
    } catch {
      window.open(url, "_blank");
    }
  }

  function getFormatName(fmt: string, fallback: string): string {
    switch (fmt) {
      case "windows_setup": return i18n.t('settings.formatWindowsSetup', {}, fallback);
      case "msix": return i18n.t('settings.formatMsix', {}, fallback);
      case "appimage": return i18n.t('settings.formatAppImage', {}, fallback);
      case "deb": return i18n.t('settings.formatDeb', {}, fallback);
      case "rpm": return i18n.t('settings.formatRpm', {}, fallback);
      case "flatpak": return i18n.t('settings.formatFlatpak', {}, fallback);
      case "snap": return i18n.t('settings.formatSnap', {}, fallback);
      case "system_pkg": return i18n.t('settings.formatSystemPkg', {}, fallback);
      default: return fallback;
    }
  }
  let isTabInitialized = $state(false);
  let editingThemeId = $state<string | null>(null);
  let useAdvancedBuilder = $state(false);

  let pruneMsg = $state<string | null>(null);
  let organizeRefreshKey = $state(0);
  let showAcoustidKey = $state(false);
  let hasEnvKey = $state(false);

  async function handlePruneMissing() {
    const { deletedSongs, removedFolders, mergedDuplicates } = await collectionStore.pruneMissing();
    if (removedFolders > 0 && mergedDuplicates > 0) {
      pruneMsg = i18n.t('settings.pruneCompleteMsgWithFoldersAndDuplicates', { count: deletedSongs, folders: removedFolders, duplicates: mergedDuplicates });
    } else if (mergedDuplicates > 0) {
      pruneMsg = i18n.t('settings.pruneCompleteMsgWithDuplicates', { count: deletedSongs, duplicates: mergedDuplicates });
    } else if (removedFolders > 0) {
      pruneMsg = i18n.t('settings.pruneCompleteMsgWithFolders', { count: deletedSongs, folders: removedFolders });
    } else {
      pruneMsg = i18n.t('settings.pruneCompleteMsg', { count: deletedSongs });
    }
    organizeRefreshKey++;
    setTimeout(() => { pruneMsg = null; }, PRUNE_MESSAGE_DURATION_MS);
  }

  onMount(async () => {
    loudnessStore.init();

    let ver = "";
    try {
      const { getVersion } = await import("@tauri-apps/api/app");
      ver = await getVersion();
    } catch {
      // Not in Tauri context
    }

    let hash = "";
    try {
      hash = await invoke<string>("get_commit_hash");
    } catch {
      hash = (import.meta as any).env?.VITE_COMMIT_HASH || "";
    }

    if (ver && hash) {
      appVersion = `${ver}#${hash}`;
    } else if (ver) {
      appVersion = ver;
    } else if (hash) {
      appVersion = `#${hash}`;
    }

    updaterStore.init();


    try {
      const settings = await invoke<Record<string, string>>("get_all_app_settings");
      if (settings && settings.active_settings_tab) {
        const savedTab = settings.active_settings_tab;
        if (savedTab === "general" || savedTab === "folders" || savedTab === "tools" || savedTab === "themes" || savedTab === "equalizer" || savedTab === "about") {
          settingsTab = savedTab;
        }
      }
      if (appVersion === "") {
        appVersion = await invoke("get_app_version");
      }
      hasEnvKey = await invoke("has_acoustid_env_key");
    } catch (e) {
      console.error("Failed to fetch settings on mount:", e);
    } finally {
      isTabInitialized = true;
    }
  });

  $effect(() => {
    if (isTabInitialized) {
      invoke("set_app_setting", { key: "active_settings_tab", value: settingsTab });
    }
  });

  async function handleRemoveDirectory(path: string) {
    if (confirm(i18n.t('settings.confirmRemoveFolder', { path }))) {
      await collectionStore.removeDirectory(path);
    }
  }

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

  const DYNAMIC_THEMES = PREDEFINED_THEMES.filter((t) => t.id === "dynamic-artwork" || t.id === "system");
  const STATIC_PREDEFINED_THEMES = PREDEFINED_THEMES.filter((t) => t.id !== "dynamic-artwork" && t.id !== "system");

  // The System theme's live colors depend on the OS light/dark preference,
  // not the static (dark) preview baked into its PREDEFINED_THEMES entry —
  // use the current system scheme so its swatch matches what's on screen.
  function getPreviewColors(theme: Theme): ThemeColors {
    if (theme.id === "system") {
      return themeStore.systemColorScheme === "dark" ? LUMINOUS_DARK_COLORS : LUMINOUS_LIGHT_COLORS;
    }
    if (theme.id === "dynamic-artwork") {
      return themeStore.resolvedColors;
    }
    return theme.colors;
  }

  function loadActiveThemeColors() {
    if (editingTheme) {
      customColors = { ...editingTheme.colors };
    } else {
      customColors = { ...themeStore.resolvedColors };
    }
  }

  // Pre-fill theme builder with current active theme colors on mount and
  // updates — skipped while editing an existing custom theme, since that
  // case is seeded from the theme being edited instead (below).
  $effect(() => {
    const colors = themeStore.resolvedColors;
    if (!editingThemeId) {
      customColors = { ...colors };
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

  $effect(() => {
    if (customColors) {
      // deep read to trigger reactivity
      const _ = customColors["bg-main"] + customColors["bg-sidebar"] + customColors["bg-playerbar"] + customColors["color-accent"] + customColors["color-accent-hover"] + customColors["color-border"];
      themeStore.applyThemeColorsPreview(customColors);
    }
  });

  $effect(() => {
    if (settingsTab !== "themes") {
      themeStore.applyActiveTheme();
    }
  });

  import { onDestroy } from 'svelte';
  onDestroy(() => {
    themeStore.applyActiveTheme();
  });

  async function saveCustomTheme() {
    if (newThemeName.trim() === "") {
      toastStore.show(i18n.t('settings.enterThemeNameAlert'));
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

  function getPhaseDisplayName(phase: string | undefined): string {
    if (!phase) return i18n.t('sidebar.scanning');
    switch (phase) {
      case "discovering":
        return i18n.t('settings.phaseDiscovering');
      case "reading_tags":
        return i18n.t('settings.phaseReadingTags');
      case "updating":
        return i18n.t('settings.phaseUpdating');
      case "done":
        return i18n.t('settings.phaseDone');
      default:
        return phase;
    }
  }
</script>

<div class="flex-1 flex flex-col overflow-hidden bg-brand-main text-brand-text-secondary h-full">
  <div class="h-16 pl-6 pr-8 border-b border-brand-border flex items-center justify-between shrink-0">
    <div class="flex items-center gap-3">
      <Settings class="w-5 h-5 text-brand-accent-text" />
      <h2 class="text-base font-bold text-brand-text-primary">{i18n.t('settings.title')}</h2>
    </div>

    <div class="flex bg-brand-sidebar border border-brand-border rounded-xl p-0.5 text-xs shadow-sm">
      <button
        onclick={() => { settingsTab = "general"; }}
        class="px-4 py-1.5 rounded-lg font-semibold transition-all {settingsTab === 'general' ? 'bg-brand-accent text-brand-accent-contrast shadow-md' : 'text-brand-text-secondary hover:text-brand-text-primary'}"
      >
        {i18n.t('settings.tabGeneral')}
      </button>
      <button
        onclick={() => { settingsTab = "folders"; }}
        class="px-4 py-1.5 rounded-lg font-semibold transition-all {settingsTab === 'folders' ? 'bg-brand-accent text-brand-accent-contrast shadow-md' : 'text-brand-text-secondary hover:text-brand-text-primary'}"
      >
        {i18n.t('settings.tabFolders')}
      </button>
      <button
        onclick={() => { settingsTab = "tools"; }}
        class="px-4 py-1.5 rounded-lg font-semibold transition-all {settingsTab === 'tools' ? 'bg-brand-accent text-brand-accent-contrast shadow-md' : 'text-brand-text-secondary hover:text-brand-text-primary'}"
      >
        {i18n.t('settings.tabTools')}
      </button>
      <button
        onclick={() => { settingsTab = "themes"; editingThemeId = null; }}
        class="px-4 py-1.5 rounded-lg font-semibold transition-all {settingsTab === 'themes' ? 'bg-brand-accent text-brand-accent-contrast shadow-md' : 'text-brand-text-secondary hover:text-brand-text-primary'}"
      >
        {i18n.t('settings.tabThemes')}
      </button>
      <button
        onclick={() => { settingsTab = "equalizer"; }}
        class="px-4 py-1.5 rounded-lg font-semibold transition-all {settingsTab === 'equalizer' ? 'bg-brand-accent text-brand-accent-contrast shadow-md' : 'text-brand-text-secondary hover:text-brand-text-primary'}"
      >
        {i18n.t('settings.tabEqualizer')}
      </button>
      <button
        onclick={() => { settingsTab = "about"; }}
        class="px-4 py-1.5 rounded-lg font-semibold transition-all {settingsTab === 'about' ? 'bg-brand-accent text-brand-accent-contrast shadow-md' : 'text-brand-text-secondary hover:text-brand-text-primary'}"
      >
        {i18n.t('settings.tabAbout')}
      </button>
    </div>
  </div>

  <div bind:this={contentEl} class="flex-1 overflow-y-scroll p-6" class:pb-28={!!playerStore.currentSong} use:rememberScroll={`settings:${settingsTab}`}>
  <div class="max-w-3xl mx-auto space-y-6">
    {#if settingsTab === "general"}
      <div class="bg-brand-sidebar border border-brand-border rounded-xl p-6">
        <div class="pb-3 flex items-center justify-between">
          <div class="flex items-center gap-3">
            <div class="p-2 rounded-xl bg-brand-accent/15 text-brand-accent-text shrink-0">
              <Settings class="w-5 h-5" />
            </div>
            <div class="space-y-1 min-w-0">
              <h3 class="font-bold text-sm text-brand-text-primary">{i18n.t('settings.generalTitle')}</h3>
              <p class="text-xs text-brand-text-secondary leading-relaxed">{i18n.t('settings.generalSubtitle', {}, 'Configure application language and formatting preferences.')}</p>
            </div>
          </div>
        </div>

        <div class="flex items-center justify-between gap-4 py-4">
          <div class="flex flex-col gap-0.5 min-w-0">
            <label for="language-select" class="text-sm font-medium text-brand-text-primary">{i18n.t('settings.selectLanguage')}</label>
            <!-- Invisible placeholder matching the description line's height
                 in the rows below, so this row's control centers at the same
                 relative position as the ones with an actual description. -->
            <p class="text-xs invisible" aria-hidden="true">&nbsp;</p>
          </div>
          <Select
            id="language-select"
            value={i18n.currentLocale}
            onchange={(e) => i18n.setLocale(e.currentTarget.value as Locale)}
            class="shrink-0 bg-brand-main border border-brand-border hover:border-brand-accent/60 text-brand-text-primary text-xs rounded-full pl-3.5 pr-8 py-1.5 focus:outline-none focus:border-brand-accent transition-all font-medium"
          >
            <option value="en">{i18n.t('settings.languageEnglish')}</option>
            <option value="fr">{i18n.t('settings.languageFrench')}</option>
          </Select>
        </div>

        <div class="flex items-center justify-between gap-4 py-4">
          <div class="flex flex-col gap-0.5 min-w-0">
            <label for="rating-style-select" class="text-sm font-medium text-brand-text-primary">{i18n.t('settings.ratingStyle')}</label>
            <p class="text-xs text-brand-text-secondary">{i18n.t('settings.ratingStyleHint')}</p>
          </div>
          <Select
            id="rating-style-select"
            value={prefs.ratingStyle}
            onchange={(e) => prefs.setRatingStyle(e.currentTarget.value as RatingStyle)}
            class="shrink-0 bg-brand-main border border-brand-border hover:border-brand-accent/60 text-brand-text-primary text-xs rounded-full pl-3.5 pr-8 py-1.5 focus:outline-none focus:border-brand-accent transition-all font-medium"
          >
            <option value="heart">{i18n.t('settings.ratingStyleHeart')}</option>
            <option value="stars">{i18n.t('settings.ratingStyleStars')}</option>
          </Select>
        </div>

        <div class="flex items-center justify-between gap-4 py-4">
          <div class="flex flex-col gap-0.5 min-w-0">
            <span class="text-sm font-medium text-brand-text-primary">{i18n.t('settings.minimizeToTrayLabel')}</span>
            <p class="text-xs text-brand-text-secondary">{i18n.t('settings.minimizeToTrayHint')}</p>
          </div>
          <Toggle
            checked={prefs.minimizeToTray}
            onchange={(v) => prefs.setMinimizeToTray(v)}
            label={i18n.t('settings.minimizeToTrayLabel')}
          />
        </div>
      </div>

      {#if updaterStore.isExternallyManaged}
      <div class="bg-brand-sidebar border border-brand-border rounded-xl p-6 space-y-5">
        <div class="pb-4 border-b border-brand-border/50 flex items-center gap-3 min-w-0">
          <div class="w-9 h-9 rounded-full shrink-0 flex items-center justify-center bg-brand-accent/15 text-brand-accent-text">
            <Package class="w-4.5 h-4.5" />
          </div>
          <button
            onclick={copyVersion}
            class="min-w-0 text-left group"
            title={i18n.t('settings.copyVersionHint')}
          >
            <h3 class="font-bold text-sm text-brand-text-primary truncate">{i18n.t('settings.appAndUpdatesTitle')}</h3>
            <p class="text-xs text-brand-text-secondary leading-relaxed truncate group-hover:text-brand-text-primary transition-colors">
              {versionCopied ? i18n.t('settings.copiedLabel') : (updateHeaderSubtitle || `v${versionOnly}`)}
            </p>
          </button>
        </div>

        <div class="flex items-center gap-3">
          <div class="min-w-0 space-y-1">
            {#if updaterStore.externallyManagedFormat !== 'msix'}
              <h4 class="font-bold text-sm text-brand-text-primary">
                {#if updaterStore.externallyManagedFormat === 'flatpak'}
                  {i18n.t('settings.updateManagedByFlatpakTitle', {}, 'Managed by Flatpak')}
                {:else}
                  {i18n.t('settings.updateManagedByPackageManagerTitle', {}, 'Managed by your package manager')}
                {/if}
              </h4>
            {/if}
            <p class="text-xs text-brand-text-secondary leading-relaxed">
              {#if updaterStore.externallyManagedFormat === 'msix'}
                {i18n.t('settings.updateManagedByStoreDesc', {}, 'Updates are installed automatically.')}
              {:else if updaterStore.externallyManagedFormat === 'flatpak'}
                {i18n.t('settings.updateManagedByFlatpakDesc', {}, 'Run flatpak update, or update through your software center.')}
              {:else}
                {i18n.t('settings.updateManagedByPackageManagerDesc', { format: getFormatName(updaterStore.installFormat.format, updaterStore.installFormat.human_name) }, 'Updates for your {format} install are handled by your system package manager.')}
              {/if}
            </p>
          </div>
        </div>

        {#if updaterStore.externallyManagedFormat === 'msix'}
          <button onclick={() => openExternalUrl(MICROSOFT_STORE_URL)} class="inline-block rounded-md overflow-hidden focus:outline-none focus-visible:ring-2 focus-visible:ring-brand-accent">
            <img src="/microsoft-store-badge.svg" alt={i18n.t('settings.updateViewInStore', {}, 'View in Microsoft Store')} class="h-11 w-auto" />
          </button>
        {:else}
          <div class="pt-3 border-t border-brand-border/50 text-xs text-brand-text-secondary">
            {i18n.t('settings.updateInstalledAsFooter', { format: getFormatName(updaterStore.installFormat.format, updaterStore.installFormat.human_name) })}
          </div>
        {/if}
      </div>
      {:else}
      <div class="bg-brand-sidebar border border-brand-border rounded-xl p-6 space-y-5">
        <div class="pb-4 border-b border-brand-border/50 flex items-center justify-between gap-4">
          <div class="flex items-center gap-3 min-w-0">
            <div class="relative w-9 h-9 rounded-full shrink-0 flex items-center justify-center {
              updaterStore.checkStatus === 'error' ? 'bg-red-950/30 text-red-400 border border-red-900/30'
              : updaterStore.checkStatus === 'available' ? 'bg-brand-accent/20 text-brand-accent-text border border-brand-accent/30'
              : 'bg-brand-accent/15 text-brand-accent-text'
            }">
              {#if !updaterStore.updateCheckEnabled}
                <Download class="w-4.5 h-4.5" />
              {:else if updaterStore.checkStatus === 'checking'}
                <RefreshCw class="w-4.5 h-4.5 animate-spin" />
              {:else if updaterStore.checkStatus === 'error'}
                <AlertTriangle class="w-4.5 h-4.5" />
              {:else if updaterStore.checkStatus === 'available'}
                <ArrowUp class="w-4.5 h-4.5 stroke-[2.5]" />
              {:else}
                <span class="relative inline-flex items-center justify-center">
                  {#if justConfirmedUpToDate}
                    <span class="absolute inset-0 rounded-full anim-glow-ring"></span>
                  {/if}
                  <Check class="w-4.5 h-4.5 {justConfirmedUpToDate ? 'anim-check-pop' : ''}" />
                </span>
              {/if}
            </div>
            <button
              onclick={copyVersion}
              class="min-w-0 text-left group"
              title={i18n.t('settings.copyVersionHint')}
            >
              <h3 class="font-bold text-sm text-brand-text-primary truncate">{updateHeaderTitle}</h3>
              <p class="text-xs text-brand-text-secondary leading-relaxed truncate group-hover:text-brand-text-primary transition-colors">
                {#if versionCopied}
                  {i18n.t('settings.copiedLabel')}
                {:else if updaterStore.checkStatus === 'error' && updaterStore.errorMessage}
                  {updaterStore.errorMessage}
                {:else}
                  {updateHeaderSubtitle}
                {/if}
              </p>
            </button>
          </div>
          <Button onclick={() => updaterStore.checkForUpdates()} disabled={updaterStore.checkStatus === 'checking'} variant="secondary" size="sm" class="shrink-0">
            <RefreshCw class="w-3.5 h-3.5 {updaterStore.checkStatus === 'checking' ? 'animate-spin text-brand-accent-text' : ''}" />
            {updaterStore.checkStatus === 'checking' ? i18n.t('settings.updateChecking') : i18n.t('settings.updateCheckNowBtn')}
          </Button>
        </div>

        {#if updaterStore.updateAvailable || updaterStore.installStatus !== 'idle'}
          <div class="bg-brand-accent/10 border border-brand-accent/30 rounded-xl p-4 flex items-center justify-between gap-4 anim-card-materialize">
            <div class="flex items-center gap-3 min-w-0">
              <div class="relative w-9 h-9 rounded-lg bg-brand-accent/20 text-brand-accent-text border border-brand-accent/30 flex items-center justify-center shrink-0">
                {#if updaterStore.installStatus === 'ready-to-restart'}
                  <Check class="w-5 h-5 stroke-[2.5]" />
                {:else}
                  <span class="absolute -top-1 -right-1 w-2.5 h-2.5 rounded-full bg-brand-accent anim-badge-glow"></span>
                  <ArrowUp class="w-5 h-5 stroke-[2.5]" />
                {/if}
              </div>
              <div class="min-w-0 space-y-1">
                <p class="text-sm font-bold text-brand-text-primary">
                  {i18n.t('settings.updateAvailableTitle', { version: updaterStore.latestVersion })}
                </p>
                {#if updaterStore.installStatus === 'downloading'}
                  <p class="text-xs text-brand-text-secondary/80">
                    {downloadPercent !== null
                      ? i18n.t('settings.updateDownloadingProgress', { percent: downloadPercent }, 'Downloading update... {percent}%')
                      : i18n.t('settings.updateDownloading', {}, 'Downloading update...')}
                  </p>
                  {#if downloadPercent !== null}
                    <div class="w-40 h-1.5 rounded-full bg-brand-border/60 overflow-hidden">
                      <div class="h-full bg-brand-accent transition-all duration-200" style="width: {downloadPercent}%"></div>
                    </div>
                  {/if}
                {:else if updaterStore.installStatus === 'ready-to-restart'}
                  <p class="text-xs text-brand-text-secondary/80">{i18n.t('settings.updateReadyToRestart', {}, 'Update downloaded — restart to finish installing.')}</p>
                {:else if updaterStore.installStatus === 'error'}
                  <p class="text-xs text-brand-text-secondary/80">{updaterStore.errorMessage || i18n.t('settings.updateInstallError', {}, 'Update failed.')}</p>
                {:else}
                  <p class="text-xs text-brand-text-secondary/80">
                    {updaterStore.installFormat.supports_self_update
                      ? i18n.t('settings.updateDirectReady', {}, 'In-app update ready for direct installation.')
                      : i18n.t('settings.updateGithubLink', {}, 'Download update payload directly from GitHub Releases.')}
                  </p>
                {/if}
              </div>
            </div>

            {#if updaterStore.installFormat.supports_self_update}
              {#if updaterStore.installStatus === 'ready-to-restart'}
                <Button onclick={() => updaterStore.restartNow()} variant="primary" size="sm" class="shrink-0">
                  <RefreshCw class="w-4 h-4" />
                  {i18n.t('settings.updateRestartBtn', {}, 'Restart to Update')}
                </Button>
              {:else if updaterStore.installStatus === 'downloading'}
                <Button disabled variant="primary" size="sm" class="shrink-0">
                  <RefreshCw class="w-4 h-4 animate-spin" />
                  {i18n.t('settings.updateDownloadingBtn', {}, 'Downloading...')}
                </Button>
              {:else}
                <Button onclick={() => updaterStore.downloadAndInstall()} variant="primary" size="sm" class="shrink-0">
                  <Download class="w-4 h-4" />
                  {updaterStore.installStatus === 'error' ? i18n.t('settings.updateRetryBtn', {}, 'Retry') : i18n.t('settings.updateDownloadBtn')}
                </Button>
              {/if}
            {:else}
              <Button onclick={() => openExternalUrl(updaterStore.releaseUrl)} variant="primary" size="sm" class="shrink-0">
                <Download class="w-4 h-4" />
                {i18n.t('settings.updateDownloadGithubBtn')}
              </Button>
            {/if}
          </div>
        {/if}

        <div>
          <h4 class="text-xs text-brand-text-secondary font-bold tracking-wider uppercase mb-3">{i18n.t('settings.updatePolicyTitle')}</h4>
          <div class="grid grid-cols-1 sm:grid-cols-3 gap-3">
            {#each UPDATE_POLICIES as policy}
              {@const isSelected = updaterStore.updatePolicy === policy.id}
              {@const isDisabled = policy.id === 'auto' && !updaterStore.installFormat.supports_self_update}
              <button
                type="button"
                role="radio"
                aria-checked={isSelected}
                disabled={isDisabled}
                onclick={() => updaterStore.setUpdatePolicy(policy.id)}
                class="bg-brand-main/50 border-2 rounded-xl p-4 flex flex-col items-start gap-1 text-left transition-colors duration-200 w-full disabled:opacity-40 disabled:cursor-not-allowed {isSelected ? 'border-brand-accent shadow-md shadow-brand-accent/5' : 'border-brand-border/60 hover:border-brand-accent/40'}"
              >
                <span class="font-semibold text-sm text-brand-text-primary">{i18n.t(policy.labelKey)}</span>
                <span class="text-xs text-brand-text-secondary leading-relaxed">
                  {isDisabled ? i18n.t('settings.updateGithubLink', {}, 'Download update payload directly from GitHub Releases.') : i18n.t(policy.hintKey)}
                </span>
              </button>
            {/each}
          </div>
        </div>

        <div class="flex items-center justify-between gap-4 pt-3 border-t border-brand-border/50 text-xs text-brand-text-secondary">
          <span>
            {i18n.t('settings.updateInstalledAsFooter', { format: getFormatName(updaterStore.installFormat.format, updaterStore.installFormat.human_name) })}
            — {updaterStore.installFormat.supports_self_update
              ? i18n.t('settings.updateAutoSupportedFooter')
              : updaterStore.installFormat.format === 'appimage'
                ? i18n.t('settings.updateNotifyOnlyAppImageFooter', {}, 'notify only, rebuild AppImage locally')
                : i18n.t('settings.updateNotifyOnlyFooter')}
          </span>
          <button onclick={() => openExternalUrl(updaterStore.releaseUrl)} class="text-brand-accent-text hover:underline font-medium shrink-0">
            {i18n.t('settings.releaseNotesLink')}
          </button>
        </div>
      </div>
      {/if}
    {:else if settingsTab === "folders"}
      <div class="bg-brand-sidebar border border-brand-border rounded-xl p-6 space-y-4">
        <div class="pb-3 flex justify-between items-center">
          <div class="flex items-center gap-3">
            <div class="p-2 rounded-xl bg-brand-accent/15 text-brand-accent-text shrink-0">
              <Folder class="w-5 h-5" />
            </div>
            <div class="space-y-1 min-w-0">
              <h3 class="font-bold text-sm text-brand-text-primary">{i18n.t('settings.watchedFoldersTitle')}</h3>
              <p class="text-xs text-brand-text-secondary leading-relaxed">{i18n.t('settings.watchedFoldersSubtitle', {}, 'Manage directories to scan for music files.')}</p>
            </div>
          </div>
          <Button onclick={() => collectionStore.addDirectoryDialog()} variant="primary" size="sm">
            <Plus class="w-4 h-4" /> {i18n.t('settings.addFolder')}
          </Button>
        </div>

        {#if loudnessStore.enabled && loudnessStore.analysisRemaining > 0}
          <div class="flex items-center gap-2.5 bg-brand-accent/10 border border-brand-accent/30 rounded-xl px-4 py-2.5 text-xs text-brand-text-secondary">
            <Activity class="w-4 h-4 text-brand-accent-text shrink-0" />
            <span>{i18n.t('settings.loudnessAnalysisActive', { remaining: loudnessStore.analysisRemaining })}</span>
          </div>
        {/if}

        <div class="space-y-2">
          {#each collectionStore.directories as dir}
            <div class="flex items-center justify-between bg-brand-main/50 border border-brand-border/60 rounded-xl p-4 hover:border-brand-border transition-colors">
              <div class="flex items-center gap-3.5 min-w-0">
                <div class="min-w-0">
                  <p class="text-sm font-medium text-brand-text-primary truncate" title={dir.path}>{dir.path}</p>
                  <p class="text-xs mt-0.5" class:text-brand-text-secondary={dir.is_available !== false} class:text-red-400={dir.is_available === false}>
                    {#if dir.is_available === false}
                      <span class="flex items-center gap-1">
                        <AlertTriangle class="w-3 h-3" />
                        {i18n.t('settings.folderItemUnavailable', {}, 'Unavailable (Drive disconnected?)')}
                      </span>
                    {:else}
                      {collectionStore.watchFoldersRealtime
                        ? i18n.t('settings.folderItemRecursive')
                        : i18n.t('settings.folderItemRecursiveWatchOff')}
                    {/if}
                  </p>
                </div>
              </div>
              <button
                onclick={() => handleRemoveDirectory(dir.path)}
                class="p-2 rounded-lg bg-brand-main hover:bg-red-950/20 text-brand-text-secondary hover:text-red-400 border border-brand-border hover:border-red-900/30 transition-colors"
                title={i18n.t('settings.folderItemStopWatch')}
              >
                <Trash2 class="w-4 h-4 text-brand-accent-text" />
              </button>
            </div>
          {/each}

          {#if collectionStore.directories.length === 0}
            <div class="border border-dashed border-brand-border rounded-xl py-12 text-center text-brand-text-secondary">
              <Folder class="w-12 h-12 mx-auto mb-2 text-brand-text-secondary/50" />
              <h4 class="font-semibold text-brand-text-primary mb-1">{i18n.t('settings.noFoldersTitle')}</h4>
              <p class="text-xs text-brand-text-secondary mb-4">{i18n.t('settings.noFoldersText')}</p>
            </div>
          {/if}
        </div>
      </div>

      <div class="bg-brand-sidebar border border-brand-border rounded-xl p-6 space-y-5">
        <div class="pb-3 flex items-center justify-between">
          <div class="flex items-center gap-3">
            <div class="p-2 rounded-xl bg-brand-accent/15 text-brand-accent-text shrink-0">
              <RefreshCw class="w-5 h-5" />
            </div>
            <div class="space-y-1 min-w-0">
              <h3 class="font-bold text-sm text-brand-text-primary">{i18n.t('settings.rescanTitle')}</h3>
              <p class="text-xs text-brand-text-secondary leading-relaxed">{i18n.t('settings.rescanSubtitle')}</p>
            </div>
          </div>
        </div>

        {#if collectionStore.isScanning}
          <div class="bg-brand-main/60 border border-brand-accent/30 rounded-xl p-4 space-y-2">
            <div class="flex justify-between items-center text-xs font-semibold text-brand-text-primary">
              <span class="flex items-center gap-2">
                <RefreshCw class="w-4 h-4 animate-spin text-brand-accent-text" />
                {i18n.t('settings.scanningPhase', { phase: getPhaseDisplayName(collectionStore.scanProgress?.phase) })}
              </span>
              <span>{collectionStore.scanProgress?.scanned || 0} / {collectionStore.scanProgress?.total || 0}</span>
            </div>
            <div class="w-full bg-brand-sidebar rounded-full h-2 overflow-hidden border border-brand-border/40">
              <div
                class="bg-brand-accent h-2 rounded-full transition-all duration-300"
                style="width: {collectionStore.scanProgress?.total ? (collectionStore.scanProgress.scanned / collectionStore.scanProgress.total) * 100 : 0}%"
              ></div>
            </div>
            <p class="text-xs text-brand-text-secondary truncate">{collectionStore.scanProgress?.current_path || ""}</p>
          </div>
        {/if}

        <div class="flex flex-wrap items-center gap-3">
          <Button
            onclick={() => collectionStore.startScan(false)}
            disabled={collectionStore.isScanning}
            variant="primary"
            size="sm"
            title={i18n.t('settings.incrementalRescanHint')}
          >
            <RefreshCw class="w-4 h-4" />
            {i18n.t('settings.incrementalRescanBtn')}
          </Button>

          <Button
            onclick={() => collectionStore.startScan(true)}
            disabled={collectionStore.isScanning}
            variant="secondary"
            size="sm"
            title={i18n.t('settings.forceFullScanHint')}
          >
            <RotateCcw class="w-4 h-4 text-brand-accent-text" />
            {i18n.t('settings.forceFullScanBtn')}
          </Button>
        </div>

        <div class="pt-3 space-y-4">
          <div class="flex items-center justify-between gap-4">
            <div class="flex flex-col gap-0.5 min-w-0">
              <span class="text-sm font-medium text-brand-text-primary">{i18n.t('settings.watchRealtimeLabel')}</span>
              <p class="text-xs text-brand-text-secondary">{i18n.t('settings.watchRealtimeHint')}</p>
            </div>
            <Toggle
              checked={collectionStore.watchFoldersRealtime}
              onchange={(v) => collectionStore.setWatchFoldersRealtime(v)}
              label={i18n.t('settings.watchRealtimeLabel')}
            />
          </div>

          <div class="flex items-center justify-between gap-4">
            <div class="flex flex-col gap-0.5 min-w-0">
              <span class="text-sm font-medium text-brand-text-primary">{i18n.t('settings.scanOnStartupLabel')}</span>
              <p class="text-xs text-brand-text-secondary">{i18n.t('settings.scanOnStartupHint')}</p>
            </div>
            <Toggle
              checked={collectionStore.scanOnStartup}
              onchange={(v) => collectionStore.setScanOnStartup(v)}
              label={i18n.t('settings.scanOnStartupLabel')}
            />
          </div>
        </div>

        <div class="pt-3 border-t border-brand-border/50 space-y-3">
          {#if collectionStore.lastScanTime}
            <div class="text-xs text-brand-text-secondary flex items-center justify-between font-medium">
              <span class="flex items-center gap-1.5">
                <Clock class="w-3.5 h-3.5 text-brand-accent-text shrink-0" />
                {i18n.t('settings.lastScanned', { time: collectionStore.lastScanTime })}
              </span>
            </div>
          {/if}

          <div class="grid grid-cols-2 md:grid-cols-4 gap-4 text-xs">
          <div class="bg-brand-main/40 border border-brand-border rounded-lg p-3">
            <span class="text-xs text-brand-text-secondary uppercase font-semibold">{i18n.t('settings.statsSongs')}</span>
            <p class="text-base font-bold text-brand-text-primary mt-0.5">{collectionStore.stats.total_songs.toLocaleString()}</p>
          </div>
          <div class="bg-brand-main/40 border border-brand-border rounded-lg p-3">
            <span class="text-xs text-brand-text-secondary uppercase font-semibold">{i18n.t('settings.statsAlbums')}</span>
            <p class="text-base font-bold text-brand-text-primary mt-0.5">{collectionStore.stats.total_albums.toLocaleString()}</p>
          </div>
          <div class="bg-brand-main/40 border border-brand-border rounded-lg p-3">
            <span class="text-xs text-brand-text-secondary uppercase font-semibold">{i18n.t('settings.statsArtists')}</span>
            <p class="text-base font-bold text-brand-text-primary mt-0.5">{collectionStore.stats.total_artists.toLocaleString()}</p>
          </div>
          <div class="bg-brand-main/40 border border-brand-border rounded-lg p-3">
            <span class="text-xs text-brand-text-secondary uppercase font-semibold">{i18n.t('settings.statsSize')}</span>
            <p class="text-base font-bold text-brand-text-primary mt-0.5">{(collectionStore.stats.total_filesize_bytes / (1024 * 1024 * 1024)).toFixed(2)} GB</p>
          </div>
        </div>
      </div>
      </div>
    {:else if settingsTab === "tools"}
      <OrganizeFiles embedded songIds={[]} initialScope="library" refreshKey={organizeRefreshKey} />

      <div class="bg-brand-sidebar border border-brand-border rounded-xl p-6 space-y-5">
        <div class="flex flex-wrap items-center gap-3">
          <Button onclick={handlePruneMissing} disabled={collectionStore.isScanning} variant="secondary" size="sm">
            <Eraser class="w-4 h-4" />
            {i18n.t('settings.pruneMissingBtn')}
          </Button>
          <span class="text-xs text-brand-text-secondary">{i18n.t('settings.pruneMissingHint')}</span>

          {#if pruneMsg}
            <span class="text-xs text-brand-accent-text font-medium transition-all">{pruneMsg}</span>
          {/if}
        </div>
      </div>

      <div class="bg-brand-sidebar border border-brand-border rounded-xl p-6 space-y-4">
        <div class="pb-3 flex justify-between items-center">
          <div class="flex items-center gap-3">
            <div class="p-2 rounded-xl bg-brand-accent/15 text-brand-accent-text shrink-0">
              <Sparkles class="w-5 h-5" />
            </div>
            <div class="space-y-1 min-w-0">
              <h3 class="font-bold text-sm text-brand-text-primary">{i18n.t('settings.acoustidIntegration')}</h3>
              <p class="text-xs text-brand-text-secondary leading-relaxed">
                {i18n.t('settings.acoustidDesc1')}<button onclick={() => openExternalUrl("https://acoustid.org")} class="text-brand-accent hover:underline">AcoustID</button>{i18n.t('settings.acoustidDesc2')}
                <br />
                {i18n.t('settings.acoustidDesc3')}<button onclick={() => collectionStore.activeTab = 'help'} class="text-brand-accent hover:underline">{i18n.t('settings.acoustidUserGuide')}</button>{i18n.t('settings.acoustidDesc4')}
              </p>
            </div>
          </div>
        </div>
          
        <div>
          {#if hasEnvKey}
            <div class="mb-4 text-xs font-medium text-brand-accent-text flex items-center gap-2">
              <Check class="w-3.5 h-3.5" />
              <span>{i18n.t('settings.acoustidEnvKeyFound', { env: 'ACOUSTID_API_KEY' })}</span>
            </div>
          {/if}

          <div class="flex items-center gap-3 max-w-md">
            <div class="relative flex-1">
              <Input
                type={showAcoustidKey ? "text" : "password"}
                bind:value={prefs.acoustidApiKey}
                onchange={() => prefs.setAcoustidApiKey(prefs.acoustidApiKey)}
                placeholder={i18n.t('settings.acoustidPlaceholder')}
                class="w-full pr-10"
              />
              <button 
                type="button"
                onclick={() => showAcoustidKey = !showAcoustidKey}
                class="absolute right-3 top-1/2 -translate-y-1/2 text-brand-text-secondary hover:text-brand-text-primary transition-colors"
                title={showAcoustidKey ? "Hide key" : "Show key"}
              >
                {#if showAcoustidKey}
                  <EyeOff class="w-4 h-4" />
                {:else}
                  <Eye class="w-4 h-4" />
                {/if}
              </button>
            </div>
          </div>
        </div>
      </div>
    {:else if settingsTab === "themes"}
      <div class="space-y-6">
        <div class="bg-brand-sidebar border border-brand-border rounded-xl p-6 space-y-6">
        <div class="pb-1 flex justify-between items-center">
          <div class="flex items-center gap-3">
            <div class="p-2 rounded-xl bg-brand-accent/15 text-brand-accent-text shrink-0">
              <Palette class="w-5 h-5" />
            </div>
            <div class="space-y-1 min-w-0">
              <h3 class="font-bold text-sm text-brand-text-primary">{i18n.t('settings.tabThemes')}</h3>
              <p class="text-xs text-brand-text-secondary leading-relaxed">{i18n.t('settings.themesSubtitle', {}, 'Customize the visual appearance and colours of Luminous.')}</p>
            </div>
          </div>
        </div>

        <div>
          <h4 class="text-xs text-brand-text-secondary font-bold tracking-wider uppercase mb-3">{i18n.t('settings.dynamicThemes', {}, 'Dynamic Themes')}</h4>
          <div class="grid grid-cols-1 sm:grid-cols-2 gap-4">
            {#each DYNAMIC_THEMES as theme}
              {@const previewColors = getPreviewColors(theme)}
              <button
                onclick={() => themeStore.setTheme(theme.id)}
                class="bg-brand-main/50 border-2 rounded-xl p-4 flex flex-col items-start gap-3 text-left transition-colors duration-200 group hover:border-brand-accent/40 w-full relative {themeStore.activeThemeId === theme.id ? 'border-brand-accent shadow-md shadow-brand-accent/5' : 'border-brand-border/60'}"
              >
                <div class="flex items-center justify-between w-full">
                  <span class="font-semibold text-sm text-brand-text-primary flex items-center gap-1.5">
                    {#if theme.id === 'system'}
                      <span title={themeStore.systemColorScheme === 'dark' ? i18n.t('settings.systemThemeDark') : i18n.t('settings.systemThemeLight')}>
                        {#if themeStore.systemColorScheme === 'dark'}
                          <Moon class="w-3.5 h-3.5 text-brand-text-secondary" />
                        {:else}
                          <Sun class="w-3.5 h-3.5 text-brand-text-secondary" />
                        {/if}
                      </span>
                    {/if}
                    {theme.isCustom ? theme.name : i18n.t('themes.' + theme.id, {}, theme.name)}
                  </span>
                </div>
                <!-- Miniature colors preview matching 1-6 Theme Builder archetype order -->
                <div class="flex gap-0.5 w-full h-8 rounded-lg overflow-hidden border border-brand-border/40 bg-black/10">
                  <div class="flex-1" style="background-color: {previewColors['bg-main']}" title={i18n.t('settings.mainViewLabel')}></div>
                  <div class="flex-1" style="background-color: {previewColors['bg-sidebar']}" title={i18n.t('settings.sidebarLabel')}></div>
                  <div class="flex-1" style="background-color: {previewColors['bg-playerbar']}" title={i18n.t('settings.playerBarLabel')}></div>
                  <div class="flex-1" style="background-color: {previewColors['color-accent']}" title={i18n.t('settings.accentLabel')}></div>
                  <div class="flex-1" style="background-color: {previewColors['color-accent-hover']}" title={i18n.t('settings.accentHoverLabel')}></div>
                  <div class="flex-1" style="background-color: {previewColors['color-border']}" title={i18n.t('settings.bordersLabel')}></div>
                </div>
                {#if theme.id === 'dynamic-artwork'}
                  <span class="text-xs text-brand-text-secondary leading-relaxed">{i18n.t('settings.luminousFootnote', {}, 'Colors shift to match whatever album art is playing now')}</span>
                {:else if theme.id === 'system'}
                  <span class="text-xs text-brand-text-secondary leading-relaxed">{i18n.t('settings.systemFootnote', {}, 'Switches between light and dark to match your OS')}</span>
                {/if}
              </button>
            {/each}
          </div>
        </div>

        <div>
          <h4 class="text-xs text-brand-text-secondary font-bold tracking-wider uppercase mb-3">{i18n.t('settings.predefinedThemes', {}, 'Predefined Themes')}</h4>
          <div class="grid grid-cols-1 md:grid-cols-3 lg:grid-cols-5 gap-4">
            {#each STATIC_PREDEFINED_THEMES as theme}
              {@const previewColors = getPreviewColors(theme)}
              <button
                onclick={() => themeStore.setTheme(theme.id)}
                class="bg-brand-main/50 border-2 rounded-xl p-4 flex flex-col items-start gap-3 text-left transition-colors duration-200 group hover:border-brand-accent/40 w-full relative {themeStore.activeThemeId === theme.id ? 'border-brand-accent shadow-md shadow-brand-accent/5' : 'border-brand-border/60'}"
              >
                <div class="flex items-center justify-between w-full">
                  <span class="font-semibold text-sm text-brand-text-primary flex items-center gap-1.5">
                    {theme.isCustom ? theme.name : i18n.t('themes.' + theme.id, {}, theme.name)}
                  </span>
                </div>
                <!-- Miniature colors preview matching 1-6 Theme Builder archetype order -->
                <div class="flex gap-0.5 w-full h-8 rounded-lg overflow-hidden border border-brand-border/40 bg-black/10">
                  <div class="flex-1" style="background-color: {previewColors['bg-main']}" title={i18n.t('settings.mainViewLabel')}></div>
                  <div class="flex-1" style="background-color: {previewColors['bg-sidebar']}" title={i18n.t('settings.sidebarLabel')}></div>
                  <div class="flex-1" style="background-color: {previewColors['bg-playerbar']}" title={i18n.t('settings.playerBarLabel')}></div>
                  <div class="flex-1" style="background-color: {previewColors['color-accent']}" title={i18n.t('settings.accentLabel')}></div>
                  <div class="flex-1" style="background-color: {previewColors['color-accent-hover']}" title={i18n.t('settings.accentHoverLabel')}></div>
                  <div class="flex-1" style="background-color: {previewColors['color-border']}" title={i18n.t('settings.bordersLabel')}></div>
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
                  class="bg-brand-main/50 border-2 rounded-xl p-4 flex flex-col gap-3 text-left transition-colors w-full {themeStore.activeThemeId === theme.id ? 'border-brand-accent shadow-md shadow-brand-accent/5' : 'border-brand-border/60 hover:border-brand-border'}"
                >
                  <div class="flex items-center justify-between w-full">
                    <span class="font-semibold text-sm text-brand-text-primary truncate">{theme.name}</span>
                  </div>
                  <!-- Miniature colors preview matching 1-6 Theme Builder archetype order -->
                  <div class="flex gap-0.5 w-full h-8 rounded-lg overflow-hidden border border-brand-border/40 bg-black/10">
                    <div class="flex-1" style="background-color: {theme.colors['bg-main']}" title={i18n.t('settings.mainViewLabel')}></div>
                    <div class="flex-1" style="background-color: {theme.colors['bg-sidebar']}" title={i18n.t('settings.sidebarLabel')}></div>
                    <div class="flex-1" style="background-color: {theme.colors['bg-playerbar']}" title={i18n.t('settings.playerBarLabel')}></div>
                    <div class="flex-1" style="background-color: {theme.colors['color-accent']}" title={i18n.t('settings.accentLabel')}></div>
                    <div class="flex-1" style="background-color: {theme.colors['color-accent-hover']}" title={i18n.t('settings.accentHoverLabel')}></div>
                    <div class="flex-1" style="background-color: {theme.colors['color-border']}" title={i18n.t('settings.bordersLabel')}></div>
                  </div>
                  <div class="flex gap-2">
                    <button
                      onclick={(e) => { e.stopPropagation(); editingThemeId = theme.id; }}
                      class="flex-1 px-2 py-1 rounded text-xs font-semibold bg-brand-accent hover:bg-brand-accent-hover text-brand-accent-contrast transition-colors"
                      title={i18n.t('settings.editTheme')}
                    >
                      {i18n.t('settings.editThemeShort')}
                    </button>
                    <button
                      onclick={(e) => { e.stopPropagation(); themeStore.deleteCustomTheme(theme.id); }}
                      class="p-1 rounded bg-brand-main hover:bg-red-950/20 text-brand-text-secondary hover:text-red-400 border border-brand-border hover:border-red-900/30 transition-colors"
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
        </div>

        <div class="bg-brand-sidebar border border-brand-border rounded-xl p-6 space-y-5">
          <div class="flex items-center justify-between border-b border-brand-border pb-3">
            <div class="flex items-center gap-3">
              <div class="p-2 rounded-xl bg-brand-accent/15 text-brand-accent-text shrink-0">
                <Palette class="w-5 h-5" />
              </div>
              <h4 class="font-bold text-sm text-brand-text-primary">
                {editingTheme ? i18n.t('settings.editingThemeTitle', { name: editingTheme.name }) : i18n.t('settings.customThemeBuilderTitle')}
              </h4>
            </div>
            {#if editingThemeId}
              <Button onclick={() => { editingThemeId = null; themeStore.applyActiveTheme(); }} variant="secondary" size="sm">
                {i18n.t('settings.cancel')}
              </Button>
            {/if}
          </div>

          <div class="space-y-5">
              <div class="flex flex-col md:flex-row gap-4 items-end justify-between">
                <div class="flex flex-col gap-1.5 flex-1 max-w-sm">
                  <label for="theme-name-input" class="text-xs text-brand-text-secondary font-semibold">{i18n.t('settings.themeNameLabel')}</label>
                  <Input
                    id="theme-name-input"
                    type="text"
                    bind:value={newThemeName}
                    placeholder={i18n.t('settings.themeNamePlaceholder')}
                    size="md"
                    class="w-full"
                  />
                </div>
                <Button onclick={loadActiveThemeColors} variant="secondary" size="sm" class="shrink-0 h-9">
                  <RotateCcw class="w-4 h-4 text-brand-accent-text" /> {i18n.t('settings.importColors')}
                </Button>
              </div>

              <div class="grid grid-cols-2 md:grid-cols-3 gap-6 pt-2">
                <!-- Dark Muted (Main Background) -->
                <div class="flex items-center gap-3">
                  <div class="flex items-center rounded border border-brand-border bg-brand-main overflow-hidden shrink-0">
                    <input type="color" bind:value={customColors['bg-main']} class="w-9 h-9 bg-transparent border-none shrink-0" />
                    <input type="text" bind:value={customColors['bg-main']} maxlength="7" class="w-16 h-9 px-2 text-[11px] bg-transparent text-brand-text-primary outline-none font-mono uppercase" />
                  </div>
                  <div class="flex flex-col min-w-0">
                    <span class="text-xs font-semibold text-brand-text-primary">{i18n.t('settings.mainViewLabel')}</span>
                    <span class="text-[10px] text-brand-text-secondary font-medium">{i18n.t('settings.mainViewDescription')}</span>
                  </div>
                </div>

                <!-- Dark Vibrant (Sidebar Background) -->
                <div class="flex items-center gap-3">
                  <div class="flex items-center rounded border border-brand-border bg-brand-main overflow-hidden shrink-0">
                    <input type="color" bind:value={customColors['bg-sidebar']} class="w-9 h-9 bg-transparent border-none shrink-0" />
                    <input type="text" bind:value={customColors['bg-sidebar']} maxlength="7" class="w-16 h-9 px-2 text-[11px] bg-transparent text-brand-text-primary outline-none font-mono uppercase" />
                  </div>
                  <div class="flex flex-col min-w-0">
                    <span class="text-xs font-semibold text-brand-text-primary">{i18n.t('settings.sidebarLabel')}</span>
                    <span class="text-[10px] text-brand-text-secondary font-medium">{i18n.t('settings.sidebarDescription')}</span>
                  </div>
                </div>

                <!-- Light Muted (Player Bar Background) -->
                <div class="flex items-center gap-3">
                  <div class="flex items-center rounded border border-brand-border bg-brand-main overflow-hidden shrink-0">
                    <input type="color" bind:value={customColors['bg-playerbar']} class="w-9 h-9 bg-transparent border-none shrink-0" />
                    <input type="text" bind:value={customColors['bg-playerbar']} maxlength="7" class="w-16 h-9 px-2 text-[11px] bg-transparent text-brand-text-primary outline-none font-mono uppercase" />
                  </div>
                  <div class="flex flex-col min-w-0">
                    <span class="text-xs font-semibold text-brand-text-primary">{i18n.t('settings.playerBarLabel')}</span>
                    <span class="text-[10px] text-brand-text-secondary font-medium">{i18n.t('settings.playerBarDescription')}</span>
                  </div>
                </div>

                <!-- Vibrant (Accent Color) -->
                <div class="flex items-center gap-3">
                  <div class="flex items-center rounded border border-brand-border bg-brand-main overflow-hidden shrink-0">
                    <input type="color" bind:value={customColors['color-accent']} class="w-9 h-9 bg-transparent border-none shrink-0" />
                    <input type="text" bind:value={customColors['color-accent']} maxlength="7" class="w-16 h-9 px-2 text-[11px] bg-transparent text-brand-text-primary outline-none font-mono uppercase" />
                  </div>
                  <div class="flex flex-col min-w-0">
                    <span class="text-xs font-semibold text-brand-text-primary">{i18n.t('settings.accentLabel')}</span>
                    <span class="text-[10px] text-brand-text-secondary font-medium">{i18n.t('settings.accentDescription')}</span>
                  </div>
                </div>

                <!-- Light Vibrant (Accent Hover Color) -->
                <div class="flex items-center gap-3">
                  <div class="flex items-center rounded border border-brand-border bg-brand-main overflow-hidden shrink-0">
                    <input type="color" bind:value={customColors['color-accent-hover']} class="w-9 h-9 bg-transparent border-none shrink-0" />
                    <input type="text" bind:value={customColors['color-accent-hover']} maxlength="7" class="w-16 h-9 px-2 text-[11px] bg-transparent text-brand-text-primary outline-none font-mono uppercase" />
                  </div>
                  <div class="flex flex-col min-w-0">
                    <span class="text-xs font-semibold text-brand-text-primary">{i18n.t('settings.accentHoverLabel')}</span>
                    <span class="text-[10px] text-brand-text-secondary font-medium">{i18n.t('settings.accentHoverDescription')}</span>
                  </div>
                </div>

                <!-- Muted (Border Color) -->
                <div class="flex items-center gap-3">
                  <div class="flex items-center rounded border border-brand-border bg-brand-main overflow-hidden shrink-0">
                    <input type="color" bind:value={customColors['color-border']} class="w-9 h-9 bg-transparent border-none shrink-0" />
                    <input type="text" bind:value={customColors['color-border']} maxlength="7" class="w-16 h-9 px-2 text-[11px] bg-transparent text-brand-text-primary outline-none font-mono uppercase" />
                  </div>
                  <div class="flex flex-col min-w-0">
                    <span class="text-xs font-semibold text-brand-text-primary">{i18n.t('settings.bordersLabel')}</span>
                    <span class="text-[10px] text-brand-text-secondary font-medium">{i18n.t('settings.bordersDescription')}</span>
                  </div>
                </div>
              </div>

              <div class="flex flex-wrap items-center gap-3 pt-3 border-t border-brand-border">
                <Button onclick={saveCustomTheme} variant="primary" size="sm">
                  {editingThemeId ? i18n.t('settings.saveChanges') : i18n.t('settings.saveCustom')}
                </Button>
              </div>
            </div>
        </div>
      </div>
    {:else if settingsTab === "equalizer"}
      <Equalizer />
    {:else if settingsTab === "about"}
      <div class="space-y-6 max-w-4xl">
        <div class="bg-brand-sidebar border border-brand-border rounded-2xl p-6 flex flex-col md:flex-row items-center gap-6 shadow-md">
          <img src="/app-icon.svg" alt="Luminous Logo" class="w-20 h-20 shrink-0 drop-shadow-md" />
          <div class="space-y-2 text-center md:text-left flex-1 min-w-0">
            <div class="flex flex-wrap items-center justify-center md:justify-start gap-2.5">
              <h3 class="text-2xl font-bold text-brand-text-primary">{i18n.t('settings.aboutAppName', {}, 'Luminous Music Player')}</h3>
            </div>
            <p class="text-sm text-brand-text-secondary">{i18n.t('settings.aboutTagline')}</p>
            <p class="text-xs text-brand-text-secondary">
              {i18n.t('settings.aboutCreatedByPrefix', {}, 'Created by ')}
              <button
                onclick={() => openExternalUrl("https://esoltys.dev/")}
                class="text-brand-accent-text hover:text-brand-accent-text-hover font-bold hover:underline transition-colors"
                title="Eric Soltys — esoltys.dev"
              >
                Eric Soltys 🍁
              </button>
              {i18n.t('settings.aboutCreatedBySuffix', {}, ' in the BC Kootenays, Canada')}
            </p>
          </div>
        </div>

        <div class="grid grid-cols-1 md:grid-cols-3 gap-4">
          <button
            onclick={() => openExternalUrl("https://esoltys.dev/luminous")}
            class="bg-brand-sidebar/60 border border-brand-border hover:border-brand-accent/50 rounded-xl p-4 flex items-center gap-3 text-left transition-all group"
          >
            <div class="w-9 h-9 rounded-lg bg-brand-main border border-brand-border flex items-center justify-center text-brand-accent-text shrink-0 group-hover:scale-105 transition-transform">
              <Home class="w-5 h-5" />
            </div>
            <div class="min-w-0">
              <p class="text-sm font-bold text-brand-text-primary truncate">{i18n.t('settings.aboutHomepage')}</p>
              <p class="text-xs text-brand-text-secondary truncate">esoltys.dev/luminous</p>
            </div>
          </button>

          <button
            onclick={() => openExternalUrl("https://github.com/esoltys/luminous/blob/main/CREDITS.md")}
            class="bg-brand-sidebar/60 border border-brand-border hover:border-brand-accent/50 rounded-xl p-4 flex items-center gap-3 text-left transition-all group"
          >
            <div class="w-9 h-9 rounded-lg bg-brand-main border border-brand-border flex items-center justify-center text-brand-accent-text shrink-0 group-hover:scale-105 transition-transform">
              <Info class="w-5 h-5" />
            </div>
            <div class="min-w-0">
              <p class="text-sm font-bold text-brand-text-primary truncate">{i18n.t('settings.aboutViewCredits')}</p>
              <p class="text-xs text-brand-text-secondary truncate">CREDITS.md</p>
            </div>
          </button>

          <button
            onclick={() => openExternalUrl("https://github.com/esoltys/luminous/blob/main/LICENSE")}
            class="bg-brand-sidebar/60 border border-brand-border hover:border-brand-accent/50 rounded-xl p-4 flex items-center gap-3 text-left transition-all group"
          >
            <div class="w-9 h-9 rounded-lg bg-brand-main border border-brand-border flex items-center justify-center text-brand-accent-text shrink-0 group-hover:scale-105 transition-transform">
              <Shield class="w-5 h-5" />
            </div>
            <div class="min-w-0">
              <p class="text-sm font-bold text-brand-text-primary truncate">{i18n.t('settings.aboutViewLicense')}</p>
              <p class="text-xs text-brand-text-secondary truncate">{i18n.t('settings.aboutLicenseSubtitle', {}, 'MIT License')}</p>
            </div>
          </button>

          <button
            onclick={() => openExternalUrl("https://github.com/esoltys/luminous")}
            class="bg-brand-sidebar/60 border border-brand-border hover:border-brand-accent/50 rounded-xl p-4 flex items-center gap-3 text-left transition-all group"
          >
            <div class="w-9 h-9 rounded-lg bg-brand-main border border-brand-border flex items-center justify-center text-brand-accent-text shrink-0 group-hover:scale-105 transition-transform">
              <svg viewBox="0 0 24 24" fill="currentColor" class="w-5 h-5" aria-hidden="true">
                <path d="M12 .297c-6.63 0-12 5.373-12 12 0 5.303 3.438 9.8 8.205 11.385.6.113.82-.258.82-.577 0-.285-.01-1.04-.015-2.04-3.338.724-4.042-1.61-4.042-1.61C4.422 18.07 3.633 17.7 3.633 17.7c-1.087-.744.084-.729.084-.729 1.205.084 1.838 1.238 1.838 1.238 1.07 1.833 2.809 1.304 3.495.997.108-.775.417-1.305.76-1.605-2.665-.3-5.466-1.332-5.466-5.93 0-1.31.465-2.38 1.235-3.22-.135-.303-.54-1.523.105-3.176 0 0 1.005-.322 3.3 1.23.96-.267 1.98-.399 3-.405 1.02.006 2.04.138 3 .405 2.28-1.552 3.285-1.23 3.285-1.23.645 1.653.24 2.873.12 3.176.765.84 1.23 1.91 1.23 3.22 0 4.61-2.805 5.625-5.475 5.92.42.36.81 1.096.81 2.22 0 1.606-.015 2.896-.015 3.286 0 .315.21.69.825.57C20.565 22.092 24 17.592 24 12.297c0-6.627-5.373-12-12-12"/>
              </svg>
            </div>
            <div class="min-w-0">
              <p class="text-sm font-bold text-brand-text-primary truncate">{i18n.t('settings.aboutGitHubRepo')}</p>
              <p class="text-xs text-brand-text-secondary truncate">github.com/esoltys/luminous</p>
            </div>
          </button>

          <button
            onclick={() => openExternalUrl("https://github.com/esoltys/luminous/discussions")}
            class="bg-brand-sidebar/60 border border-brand-border hover:border-brand-accent/50 rounded-xl p-4 flex items-center gap-3 text-left transition-all group"
          >
            <div class="w-9 h-9 rounded-lg bg-brand-main border border-brand-border flex items-center justify-center text-brand-accent-text shrink-0 group-hover:scale-105 transition-transform">
              <MessageCircle class="w-5 h-5" />
            </div>
            <div class="min-w-0">
              <p class="text-sm font-bold text-brand-text-primary truncate">{i18n.t('settings.aboutDiscussions')}</p>
              <p class="text-xs text-brand-text-secondary truncate">github.com/esoltys/luminous/discussions</p>
            </div>
          </button>

          <button
            onclick={() => openExternalUrl("https://github.com/esoltys/luminous/issues")}
            class="bg-brand-sidebar/60 border border-brand-border hover:border-brand-accent/50 rounded-xl p-4 flex items-center gap-3 text-left transition-all group"
          >
            <div class="w-9 h-9 rounded-lg bg-brand-main border border-brand-border flex items-center justify-center text-brand-accent-text shrink-0 group-hover:scale-105 transition-transform">
              <Bug class="w-5 h-5" />
            </div>
            <div class="min-w-0">
              <p class="text-sm font-bold text-brand-text-primary truncate">{i18n.t('settings.aboutIssues')}</p>
              <p class="text-xs text-brand-text-secondary truncate">github.com/esoltys/luminous/issues</p>
            </div>
          </button>
        </div>

        <div class="bg-brand-sidebar border border-brand-border rounded-2xl p-6 space-y-4">
          <h4 class="text-xs text-brand-text-secondary font-bold tracking-wider uppercase border-b border-brand-border pb-2">
            {i18n.t('settings.aboutCoreTech')}
          </h4>

          <div class="grid grid-cols-1 md:grid-cols-2 gap-4 text-xs">
            <div class="bg-brand-main/40 border border-brand-border/60 rounded-xl p-3.5 space-y-1">
              <p class="font-bold text-brand-text-primary">Rust & Tauri v2</p>
              <p class="text-brand-text-secondary leading-relaxed">{i18n.t('settings.aboutTechRust')}</p>
            </div>

            <div class="bg-brand-main/40 border border-brand-border/60 rounded-xl p-3.5 space-y-1">
              <p class="font-bold text-brand-text-primary">Svelte 5 & TypeScript</p>
              <p class="text-brand-text-secondary leading-relaxed">{i18n.t('settings.aboutTechSvelte')}</p>
            </div>
          </div>

          <h4 class="text-xs text-brand-text-secondary font-bold tracking-wider uppercase border-b border-brand-border pb-2 pt-2">
            {i18n.t('settings.aboutAudioEngine')}
          </h4>

          <div class="grid grid-cols-1 md:grid-cols-3 gap-3 text-xs">
            <div class="bg-brand-main/40 border border-brand-border/60 rounded-xl p-3">
              <p class="font-bold text-brand-text-primary">Symphonia & CPAL</p>
              <p class="text-xs text-brand-text-secondary mt-0.5">{i18n.t('settings.aboutTechSymphonia')}</p>
            </div>
            <div class="bg-brand-main/40 border border-brand-border/60 rounded-xl p-3">
              <p class="font-bold text-brand-text-primary">rubato & bs1770</p>
              <p class="text-xs text-brand-text-secondary mt-0.5">{i18n.t('settings.aboutTechRubato')}</p>
            </div>
            <div class="bg-brand-main/40 border border-brand-border/60 rounded-xl p-3">
              <p class="font-bold text-brand-text-primary">rustfft & Lofty</p>
              <p class="text-xs text-brand-text-secondary mt-0.5">{i18n.t('settings.aboutTechRustfft')}</p>
            </div>
          </div>

          <h4 class="text-xs text-brand-text-secondary font-bold tracking-wider uppercase border-b border-brand-border pb-2 pt-2">
            {i18n.t('settings.aboutInfluences')}
          </h4>

          <p class="text-xs text-brand-text-secondary leading-relaxed">
            {i18n.t('settings.aboutInfluencesText')}
          </p>
          <p class="text-xs text-brand-text-secondary leading-relaxed">
            {i18n.t('settings.aboutDedication')}
          </p>
        </div>
      </div>
    {/if}
  </div>
  </div>
</div>
