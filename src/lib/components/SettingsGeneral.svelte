<script lang="ts">
  import { i18n, type Locale } from "../stores/i18n.svelte";
  import { prefs, type RatingStyle } from "../stores/prefs.svelte";
  import { updaterStore, MICROSOFT_STORE_URL } from "../stores/updater.svelte";
  import { onMount } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { openExternalUrl } from "../utils/openExternalUrl";
  import Toggle from "./Toggle.svelte";
  import Select from "./Select.svelte";
  import Button from "./Button.svelte";
  import {
    GearIcon as Settings,
    CheckIcon as Check,
    ArrowsClockwiseIcon as RefreshCw,
    ArrowUpIcon as ArrowUp,
    DownloadSimpleIcon as Download,
    WarningIcon as AlertTriangle,
    PackageIcon as Package
  } from "phosphor-svelte";

  const COPY_FEEDBACK_DURATION_MS = 1500;

  let appVersion = $state("");
  let versionCopied = $state(false);

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

  function getFormatName(fmt: string, fallback: string): string {
    switch (fmt) {
      case "windows_setup": return i18n.t('settings.formatWindowsSetup', {}, fallback);
      case "msix": return i18n.t('settings.formatMsix', {}, fallback);
      case "appimage": return i18n.t('settings.formatAppImage', {}, fallback);
      case "deb": return i18n.t('settings.formatDeb', {}, fallback);
      case "rpm": return i18n.t('settings.formatRpm', {}, fallback);
      case "snap": return i18n.t('settings.formatSnap', {}, fallback);
      case "system_pkg": return i18n.t('settings.formatSystemPkg', {}, fallback);
      default: return fallback;
    }
  }

  onMount(async () => {
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
      if (appVersion === "") {
        appVersion = await invoke("get_app_version");
      }
    } catch (e) {
      console.error("Failed to fetch app version on mount:", e);
    }
  });
</script>

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
          {i18n.t('settings.updateManagedByPackageManagerTitle', {}, 'Managed by your package manager')}
        </h4>
      {/if}
      <p class="text-xs text-brand-text-secondary leading-relaxed">
        {#if updaterStore.externallyManagedFormat === 'msix'}
          {i18n.t('settings.updateManagedByStoreDesc', {}, 'Updates are installed automatically.')}
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
