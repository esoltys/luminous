<script lang="ts">
  import { i18n } from "../stores/i18n.svelte";
  import { prefs } from "../stores/prefs.svelte";
  import { updaterStore, MICROSOFT_STORE_URL } from "../stores/updater.svelte";
  import { onMount } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { save } from "@tauri-apps/plugin-dialog";
  import { openExternalUrl } from "../utils/openExternalUrl";
  import { toastStore } from "../stores/toast.svelte";
  import { isWindows } from "../platform";
  import Toggle from "./Toggle.svelte";
  import Button from "./Button.svelte";
  import {
    DesktopIcon as Desktop,
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
  // comes back up-to-date — fired from the click handler only, never for a
  // silent background check.
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
      relative = i18n.plural("playlists.relativeMinutesAgo", diffMinutes);
    } else {
      const diffHours = Math.floor(diffMinutes / 60);
      relative = i18n.plural("playlists.relativeHoursAgo", diffHours);
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
    if (!prefs.onlineEnabled) return i18n.t("settings.updateChecksOfflineTitle");
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

  let confirmedTimer: ReturnType<typeof setTimeout> | undefined;

  async function checkNow() {
    await updaterStore.checkForUpdates();
    if (updaterStore.checkStatus !== "up-to-date") return;
    clearTimeout(confirmedTimer);
    justConfirmedUpToDate = true;
    confirmedTimer = setTimeout(() => { justConfirmedUpToDate = false; }, 320);
  }

  async function copyVersion() {
    try {
      await navigator.clipboard.writeText(appVersion);
      versionCopied = true;
      setTimeout(() => { versionCopied = false; }, COPY_FEEDBACK_DURATION_MS);
    } catch (e) {
      console.error("Failed to copy version to clipboard:", e);
    }
  }

  let exportingDiagnostics = $state(false);

  async function exportDiagnostics() {
    if (exportingDiagnostics) return;
    exportingDiagnostics = true;
    try {
      const timestamp = new Date().toISOString().replace(/[:.]/g, "-");
      const exportPath = await save({
        title: i18n.t("settings.exportDiagnosticsLabel", {}, "Export Diagnostics"),
        defaultPath: `luminous-diagnostics-${timestamp}.txt`,
        filters: [{ name: i18n.t("settings.textFilesFilter"), extensions: ["txt"] }],
      });
      if (exportPath && typeof exportPath === "string") {
        await invoke("export_diagnostics", { exportPath });
        toastStore.show(
          i18n.t("settings.exportDiagnosticsSuccess", {}, "Diagnostics exported"),
          "success"
        );
      }
    } catch (err) {
      console.error("Failed to export diagnostics:", err);
      toastStore.show(
        i18n.t("settings.exportDiagnosticsError", {}, "Failed to export diagnostics"),
        "error"
      );
    } finally {
      exportingDiagnostics = false;
    }
  }

  async function openDefaultAppsSettings() {
    try {
      await invoke("open_default_apps_settings");
    } catch (err) {
      console.error("Failed to open Default Apps settings:", err);
      toastStore.show(
        i18n.t("settings.defaultPlayerError", {}, "Couldn't open Windows Default Apps"),
        "error"
      );
    }
  }

  function getFormatName(fmt: string, fallback: string): string {
    switch (fmt) {
      case "windows_setup": return i18n.t('settings.formatWindowsSetup', {}, fallback);
      case "windows_portable": return i18n.t('settings.formatWindowsPortable', {}, fallback);
      case "msix": return i18n.t('settings.formatMsix', {}, fallback);
      case "appimage": return i18n.t('settings.formatAppImage', {}, fallback);
      case "deb": return i18n.t('settings.formatDeb', {}, fallback);
      case "rpm": return i18n.t('settings.formatRpm', {}, fallback);
      case "snap": return i18n.t('settings.formatSnap', {}, fallback);
      case "system_pkg": return i18n.t('settings.formatSystemPkg', {}, fallback);
      default: return fallback;
    }
  }

  interface DataDirectoryInfo {
    path: string;
    is_portable: boolean;
  }

  let dataDirectory = $state<DataDirectoryInfo | null>(null);
  let pathCopied = $state(false);

  async function copyDataDirectory() {
    if (!dataDirectory?.path) return;
    try {
      await navigator.clipboard.writeText(dataDirectory.path);
      pathCopied = true;
      setTimeout(() => { pathCopied = false; }, COPY_FEEDBACK_DURATION_MS);
      toastStore.show(i18n.t("settings.dataStorageCopiedToast", {}, "Data directory path copied to clipboard"), "success", 2000);
    } catch (e) {
      console.error("Failed to copy data path:", e);
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

    try {
      dataDirectory = await invoke<DataDirectoryInfo>("get_data_directory_info");
    } catch (e) {
      console.error("Failed to fetch data directory info on mount:", e);
    }
  });
</script>

<div class="bg-brand-sidebar border border-brand-border rounded-xl p-6">
  <div class="pb-3 flex items-center justify-between">
    <div class="flex items-center gap-3">
      <div class="p-2 rounded-xl bg-brand-accent/15 text-brand-accent-text shrink-0">
        <Desktop class="w-5 h-5" />
      </div>
      <div class="space-y-1 min-w-0">
        <h3 class="font-bold text-sm text-brand-text-primary">{i18n.t('settings.systemTitle')}</h3>
        <p class="text-xs text-brand-text-secondary leading-relaxed">{i18n.t('settings.systemSubtitle')}</p>
      </div>
    </div>
  </div>

  {#if isWindows}
    <div class="flex items-center justify-between gap-4 py-4">
      <div class="flex flex-col gap-0.5 min-w-0">
        <span class="text-sm font-medium text-brand-text-primary">{i18n.t('settings.defaultPlayerLabel', {}, 'Make Luminous the default music player')}</span>
        <p class="text-xs text-brand-text-secondary">{i18n.t('settings.defaultPlayerHint', {}, 'Open Windows Default Apps and choose which file types Luminous opens.')}</p>
      </div>
      <Button onclick={openDefaultAppsSettings} class="shrink-0 text-xs px-3.5 py-1.5">
        {i18n.t('settings.defaultPlayerButton', {}, 'Open Default Apps')}
      </Button>
    </div>
  {/if}

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

  <div class="flex items-center justify-between gap-4 py-4">
    <div class="flex flex-col gap-0.5 min-w-0">
      <span class="text-sm font-medium text-brand-text-primary">{i18n.t('settings.launchAtLoginLabel')}</span>
      <p class="text-xs text-brand-text-secondary">{i18n.t('settings.launchAtLoginHint')}</p>
    </div>
    <Toggle
      checked={prefs.autostartEnabled}
      onchange={(v) => prefs.setAutostart(v)}
      label={i18n.t('settings.launchAtLoginLabel')}
    />
  </div>

  <div class="flex items-center justify-between gap-4 py-4">
    <div class="flex flex-col gap-0.5 min-w-0">
      <span class="text-sm font-medium text-brand-text-primary">{i18n.t('settings.exportDiagnosticsLabel', {}, 'Export Diagnostics')}</span>
      <p class="text-xs text-brand-text-secondary">{i18n.t('settings.exportDiagnosticsHint', {}, 'Save a log file of recent crashes and errors to attach to a bug report.')}</p>
    </div>
    <Button onclick={exportDiagnostics} disabled={exportingDiagnostics} class="shrink-0 text-xs px-3.5 py-1.5">
      {i18n.t('settings.exportDiagnosticsLabel', {}, 'Export Diagnostics')}
    </Button>
  </div>

  {#if dataDirectory}
    <div class="flex flex-col gap-1 py-4 border-t border-brand-border/40">
      <div class="flex items-center gap-2">
        <span class="text-sm font-medium text-brand-text-primary">{i18n.t('settings.dataStorageLabel', {}, 'Data Storage Location')}</span>
        <span
          class="px-2 py-0.5 text-[10px] font-bold rounded-full {dataDirectory.is_portable ? 'bg-brand-accent/20 text-brand-accent-text border border-brand-accent/40' : 'bg-brand-border/60 text-brand-text-secondary border border-brand-border'}"
        >
          {dataDirectory.is_portable ? i18n.t('settings.dataStoragePortableBadge', {}, 'Portable Mode') : i18n.t('settings.dataStorageStandardBadge', {}, 'Standard Mode')}
        </span>
      </div>
      <p class="text-xs text-brand-text-secondary">{i18n.t('settings.dataStorageHint', {}, 'Folder where Luminous stores your library database, cover art cache, and preferences.')}</p>
      <div class="flex items-center gap-3 mt-0.5">
        <div class="text-xs font-mono text-brand-text-secondary/90 truncate select-all bg-brand-main/60 px-2 py-1 rounded border border-brand-border/40 flex-1" title={dataDirectory.path}>
          {dataDirectory.path}
        </div>
        <Button onclick={copyDataDirectory} class="shrink-0 text-xs px-3.5 py-1.5">
          {pathCopied ? i18n.t('settings.copiedLabel', {}, 'Copied!') : i18n.t('settings.dataStorageCopyButton', {}, 'Copy Path')}
        </Button>
      </div>
    </div>
  {/if}
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
    <Button onclick={checkNow} disabled={updaterStore.checkStatus === 'checking' || !prefs.onlineEnabled} variant="secondary" size="sm" class="shrink-0">
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
    <div class="grid grid-cols-1 @xl:grid-cols-3 gap-3">
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
          ? i18n.t('settings.updateNotifyOnlyAppImageFooter', {}, 'notify only, download the new AppImage')
          : updaterStore.installFormat.is_portable
            ? i18n.t('settings.updateNotifyOnlyPortableFooter', {}, 'notify only, download the new portable release ZIP')
            : i18n.t('settings.updateNotifyOnlyFooter')}
    </span>
    <button onclick={() => openExternalUrl(updaterStore.releaseUrl)} class="text-brand-accent-text hover:underline font-medium shrink-0">
      {i18n.t('settings.releaseNotesLink')}
    </button>
  </div>
</div>
{/if}
