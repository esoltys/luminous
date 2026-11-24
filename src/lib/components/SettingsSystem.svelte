<script lang="ts">
  import { i18n } from "../stores/i18n.svelte";
  import { prefs } from "../stores/prefs.svelte";
  import { playerStore } from "../stores/player.svelte";
  import { onMount } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { save } from "@tauri-apps/plugin-dialog";
  import { toastStore } from "../stores/toast.svelte";
  import { isWindows } from "../platform";
  import {
    AUDIO_BACKEND_LABEL_KEYS,
    isDirectHardware,
    noDevicesKey,
    offeredAudioBackends,
  } from "../audioOutput";
  import type { AudioBackendKind } from "../types";
  import Toggle from "./Toggle.svelte";
  import Button from "./Button.svelte";
  import {
    DesktopIcon as Desktop,
    ArrowsClockwiseIcon as RefreshCw,
    WarningIcon as AlertTriangle,
    SpeakerHighIcon as SpeakerHigh,
    SlidersHorizontalIcon as Sliders,
  } from "phosphor-svelte";
  import Select from "./Select.svelte";

  const COPY_FEEDBACK_DURATION_MS = 1500;

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
          "success",
        );
      }
    } catch (err) {
      console.error("Failed to export diagnostics:", err);
      toastStore.show(
        i18n.t("settings.exportDiagnosticsError", {}, "Failed to export diagnostics"),
        "error",
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
        "error",
      );
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
      setTimeout(() => {
        pathCopied = false;
      }, COPY_FEEDBACK_DURATION_MS);
      toastStore.show(
        i18n.t("settings.dataStorageCopiedToast", {}, "Data directory path copied to clipboard"),
        "success",
        2000,
      );
    } catch (e) {
      console.error("Failed to copy data path:", e);
    }
  }

  onMount(async () => {
    try {
      dataDirectory = await invoke<DataDirectoryInfo>("get_data_directory_info");
    } catch (e) {
      console.error("Failed to fetch data directory info on mount:", e);
    }

    await prefs.refreshAudioDevices();
  });

  let currentBackendDevices = $derived(
    prefs.audioOutputDevices.filter((d) => d.backend === prefs.audioOutputConfig.backend),
  );

  const audioBackends = offeredAudioBackends();
  let directHardware = $derived(isDirectHardware(prefs.audioOutputConfig.backend));
  let noDevicesMessageKey = $derived(noDevicesKey(prefs.audioOutputConfig.backend));

  let selectedDeviceId = $derived.by(() => {
    const id = prefs.audioOutputConfig.device_id;
    if (id) return id;
    if (directHardware) {
      return currentBackendDevices.length > 0 ? currentBackendDevices[0].id : "";
    }
    return "";
  });

  // The chosen bit-perfect output couldn't open and shared output stands in.
  let playingShared = $derived(!!playerStore.audioPipeline?.output_fell_back);

  // Volume safety ceiling (#1890): the slider shows its drag position and
  // saves the ceiling on release, so a drag isn't a stream of writes.
  let volumeSafety = $derived(playerStore.volumeSafety);
  let ceilingDraft = $state<number | null>(null);
  let ceilingShown = $derived(ceilingDraft ?? volumeSafety?.ceiling ?? 0);

  function commitCeiling(e: Event) {
    const ceiling = parseFloat((e.currentTarget as HTMLInputElement).value);
    ceilingDraft = null;
    if (volumeSafety) void playerStore.setVolumeSafety(volumeSafety.enabled, ceiling);
  }

  async function handleBackendChange(backend: AudioBackendKind) {
    const devices = prefs.audioOutputDevices.filter((d) => d.backend === backend);
    // Shared and exclusive list the same endpoints, so a chosen device carries
    // over between them; ASIO and ALSA hardware have no system default, so
    // they take the first device.
    const current = prefs.audioOutputConfig.device_id;
    const newDeviceId =
      current && devices.some((d) => d.id === current)
        ? current
        : isDirectHardware(backend) && devices.length > 0
          ? devices[0].id
          : null;
    await prefs.setAudioOutput({ backend, device_id: newDeviceId });
  }

  async function handleDeviceChange(deviceId: string | null) {
    await prefs.setAudioOutput({
      backend: prefs.audioOutputConfig.backend,
      device_id: deviceId,
    });
  }

  async function openAsioControlPanel() {
    try {
      await prefs.openAsioControlPanel(prefs.audioOutputConfig.device_id);
    } catch (e) {
      toastStore.show(String(e), "error");
    }
  }
</script>

<div class="bg-brand-sidebar border border-brand-border rounded-xl p-6">
  <div class="pb-3 flex items-center justify-between">
    <div class="flex items-center gap-3">
      <div class="p-2 rounded-xl bg-brand-accent/15 text-brand-accent-text shrink-0">
        <Desktop class="w-5 h-5" />
      </div>
      <div class="space-y-1 min-w-0">
        <h3 class="font-bold text-sm text-brand-text-primary">{i18n.t("settings.systemTitle")}</h3>
        <p class="text-xs text-brand-text-secondary leading-relaxed">
          {i18n.t("settings.systemSubtitle")}
        </p>
      </div>
    </div>
  </div>

  {#if isWindows}
    <div class="flex items-center justify-between gap-4 py-4">
      <div class="flex flex-col gap-0.5 min-w-0">
        <span class="text-sm font-medium text-brand-text-primary"
          >{i18n.t(
            "settings.defaultPlayerLabel",
            {},
            "Make Luminous the default music player",
          )}</span
        >
        <p class="text-xs text-brand-text-secondary">
          {i18n.t(
            "settings.defaultPlayerHint",
            {},
            "Open Windows Default Apps and choose which file types Luminous opens.",
          )}
        </p>
      </div>
      <Button onclick={openDefaultAppsSettings} class="shrink-0 text-xs px-3.5 py-1.5">
        {i18n.t("settings.defaultPlayerButton", {}, "Open Default Apps")}
      </Button>
    </div>
  {/if}

  <div class="flex items-center justify-between gap-4 py-4">
    <div class="flex flex-col gap-0.5 min-w-0">
      <span class="text-sm font-medium text-brand-text-primary"
        >{i18n.t("settings.minimizeToTrayLabel")}</span
      >
      <p class="text-xs text-brand-text-secondary">{i18n.t("settings.minimizeToTrayHint")}</p>
    </div>
    <Toggle
      checked={prefs.minimizeToTray}
      onchange={(v) => prefs.setMinimizeToTray(v)}
      label={i18n.t("settings.minimizeToTrayLabel")}
    />
  </div>

  <div class="flex items-center justify-between gap-4 py-4">
    <div class="flex flex-col gap-0.5 min-w-0">
      <span class="text-sm font-medium text-brand-text-primary"
        >{i18n.t("settings.launchAtLoginLabel")}</span
      >
      <p class="text-xs text-brand-text-secondary">{i18n.t("settings.launchAtLoginHint")}</p>
    </div>
    <Toggle
      checked={prefs.autostartEnabled}
      onchange={(v) => prefs.setAutostart(v)}
      label={i18n.t("settings.launchAtLoginLabel")}
    />
  </div>

  <div class="flex items-center justify-between gap-4 py-4">
    <div class="flex flex-col gap-0.5 min-w-0">
      <span class="text-sm font-medium text-brand-text-primary"
        >{i18n.t("settings.exportDiagnosticsLabel", {}, "Export Diagnostics")}</span
      >
      <p class="text-xs text-brand-text-secondary">
        {i18n.t(
          "settings.exportDiagnosticsHint",
          {},
          "Save a log file of recent crashes and errors to attach to a bug report.",
        )}
      </p>
    </div>
    <Button
      onclick={exportDiagnostics}
      disabled={exportingDiagnostics}
      class="shrink-0 text-xs px-3.5 py-1.5"
    >
      {i18n.t("settings.exportDiagnosticsLabel", {}, "Export Diagnostics")}
    </Button>
  </div>

  {#if dataDirectory}
    <div class="flex flex-col gap-1 py-4 border-t border-brand-border/40">
      <div class="flex items-center gap-2">
        <span class="text-sm font-medium text-brand-text-primary"
          >{i18n.t("settings.dataStorageLabel", {}, "Data Storage Location")}</span
        >
        <span
          class="px-2 py-0.5 text-[10px] font-bold rounded-full {dataDirectory.is_portable
            ? 'bg-brand-accent/20 text-brand-accent-text border border-brand-accent/40'
            : 'chip-neutral'}"
        >
          {dataDirectory.is_portable
            ? i18n.t("settings.dataStoragePortableBadge", {}, "Portable Mode")
            : i18n.t("settings.dataStorageStandardBadge", {}, "Standard Mode")}
        </span>
      </div>
      <p class="text-xs text-brand-text-secondary">
        {i18n.t(
          "settings.dataStorageHint",
          {},
          "Folder where Luminous stores your library database, cover art cache, and preferences.",
        )}
      </p>
      <div class="flex items-center gap-3 mt-0.5">
        <div
          class="text-xs font-mono text-brand-text-secondary truncate select-all bg-brand-main/60 px-2 py-1 rounded border border-brand-border/40 flex-1"
          title={dataDirectory.path}
        >
          {dataDirectory.path}
        </div>
        <Button onclick={copyDataDirectory} class="shrink-0 text-xs px-3.5 py-1.5">
          {pathCopied
            ? i18n.t("settings.copiedLabel", {}, "Copied!")
            : i18n.t("settings.dataStorageCopyButton", {}, "Copy Path")}
        </Button>
      </div>
    </div>
  {/if}
</div>

<!-- Audio Output Card -->
<div class="bg-brand-sidebar border border-brand-border rounded-xl p-6 space-y-4">
  <div class="pb-3 flex items-center justify-between border-b border-brand-border/50">
    <div class="flex items-center gap-3">
      <div class="p-2 rounded-xl bg-brand-accent/15 text-brand-accent-text shrink-0">
        <SpeakerHigh class="w-5 h-5" />
      </div>
      <div class="space-y-1 min-w-0">
        <h3 class="font-bold text-sm text-brand-text-primary">
          {i18n.t("settings.audioOutputTitle")}
        </h3>
        <p class="text-xs text-brand-text-secondary leading-relaxed">
          {i18n.t("settings.audioOutputSubtitle")}
        </p>
      </div>
    </div>
    <Button
      onclick={() => prefs.refreshAudioDevices()}
      variant="secondary"
      size="sm"
      class="shrink-0 text-xs px-3.5 py-1.5"
      title={i18n.t("settings.audioRefreshDevicesHint")}
    >
      <RefreshCw class="w-3.5 h-3.5" />
      {i18n.t("settings.audioRefreshDevicesBtn")}
    </Button>
  </div>

  {#if audioBackends.length > 0}
    <!-- Output Backend: shared, or the platform's bit-perfect modes -->
    <div class="flex items-center justify-between gap-4 py-2">
      <div class="flex flex-col gap-0.5 min-w-0">
        <label for="audio-backend-select" class="text-sm font-medium text-brand-text-primary"
          >{i18n.t("settings.audioBackendLabel")}</label
        >
        <p class="text-xs text-brand-text-secondary">{i18n.t("settings.audioBackendHint")}</p>
      </div>
      <Select
        id="audio-backend-select"
        value={prefs.audioOutputConfig.backend}
        onchange={(e) => handleBackendChange(e.currentTarget.value as AudioBackendKind)}
        class="shrink-0 bg-brand-main border border-brand-border hover:border-brand-accent/60 text-brand-text-primary text-xs rounded-full pl-3.5 pr-8 py-1.5 focus:outline-none focus:border-brand-accent transition-all font-medium"
      >
        {#each audioBackends as backend}
          <option value={backend}>{i18n.t(AUDIO_BACKEND_LABEL_KEYS[backend])}</option>
        {/each}
      </Select>
    </div>
  {/if}

  <!-- Device Selection -->
  <div class="flex items-center justify-between gap-4 py-2">
    <div class="flex flex-col gap-0.5 min-w-0">
      <label for="audio-device-select" class="text-sm font-medium text-brand-text-primary"
        >{i18n.t("settings.audioDeviceLabel")}</label
      >
      <p class="text-xs text-brand-text-secondary">{i18n.t("settings.audioDeviceHint")}</p>
    </div>
    {#if currentBackendDevices.length === 0 && noDevicesMessageKey}
      <span class="flex items-center gap-1.5 text-xs text-brand-text-primary font-medium">
        <AlertTriangle class="w-3.5 h-3.5 shrink-0" />
        {i18n.t(noDevicesMessageKey)}
      </span>
    {:else}
      <Select
        id="audio-device-select"
        value={selectedDeviceId}
        onchange={(e) => handleDeviceChange(e.currentTarget.value || null)}
        class="shrink-0 max-w-[280px] bg-brand-main border border-brand-border hover:border-brand-accent/60 text-brand-text-primary text-xs rounded-full pl-3.5 pr-8 py-1.5 focus:outline-none focus:border-brand-accent transition-all font-medium truncate"
      >
        {#if !directHardware}
          <option value="">{i18n.t("settings.audioDeviceDefault")}</option>
        {/if}
        {#each currentBackendDevices as dev}
          <option value={dev.id}>
            {#if dev.connected}
              {dev.name}{dev.is_default ? ` (${i18n.t("settings.audioDeviceDefault")})` : ""}
            {:else}
              {i18n.t("settings.audioDeviceDisconnected", { name: dev.name })}
            {/if}
          </option>
        {/each}
      </Select>
    {/if}
  </div>

  {#if playingShared}
    <div class="flex items-center justify-between gap-4 py-2">
      <span class="flex items-center gap-1.5 text-xs text-brand-text-primary font-medium">
        <AlertTriangle class="w-3.5 h-3.5 shrink-0" />
        {i18n.t("settings.audioOutputPlayingShared")}
      </span>
      <Button
        onclick={() => void prefs.reapplyAudioOutput().catch(() => {})}
        variant="secondary"
        size="sm"
        class="shrink-0 text-xs px-3.5 py-1.5"
      >
        <RefreshCw class="w-3.5 h-3.5" />
        {i18n.t("playerBar.outputFallbackRetry")}
      </Button>
    </div>
  {/if}

  {#if volumeSafety}
    <div class="flex items-center justify-between gap-4 py-2">
      <div class="flex flex-col gap-0.5 min-w-0">
        <span class="text-sm font-medium text-brand-text-primary"
          >{i18n.t("settings.volumeSafetyLabel")}</span
        >
        <p class="text-xs text-brand-text-secondary">{i18n.t("settings.volumeSafetyHint")}</p>
      </div>
      <Toggle
        checked={volumeSafety.enabled}
        onchange={(v) => playerStore.setVolumeSafety(v, volumeSafety.ceiling)}
        label={i18n.t("settings.volumeSafetyLabel")}
      />
    </div>
    {#if volumeSafety.enabled}
      <div class="flex items-center justify-between gap-4 py-2">
        <div class="flex flex-col gap-0.5 min-w-0">
          <label for="volume-safety-ceiling" class="text-sm font-medium text-brand-text-primary"
            >{i18n.t("settings.volumeSafetyCeilingLabel")}</label
          >
          <p class="text-xs text-brand-text-secondary">
            {i18n.t("settings.volumeSafetyCeilingHint")}
          </p>
        </div>
        <div class="flex items-center gap-3 shrink-0">
          <input
            id="volume-safety-ceiling"
            type="range"
            min={volumeSafety.min}
            max={volumeSafety.max}
            step="0.01"
            value={volumeSafety.ceiling}
            oninput={(e) => (ceilingDraft = parseFloat(e.currentTarget.value))}
            onchange={commitCeiling}
            class="w-32 accent-brand-accent"
          />
          <span class="w-10 text-right text-xs font-medium tabular-nums text-brand-text-primary"
            >{Math.round(ceilingShown * 100)}%</span
          >
        </div>
      </div>
    {/if}
  {/if}

  <!-- ASIO Driver Control Panel Button -->
  {#if prefs.audioOutputConfig.backend === "asio" && currentBackendDevices.length > 0}
    <div class="flex items-center justify-between gap-4 pt-2 border-t border-brand-border/40">
      <div class="flex flex-col gap-0.5 min-w-0">
        <span class="text-sm font-medium text-brand-text-primary"
          >{i18n.t("settings.audioAsioControlPanelBtn")}</span
        >
        <p class="text-xs text-brand-text-secondary">
          {i18n.t("settings.audioAsioControlPanelHint")}
        </p>
      </div>
      <Button
        onclick={openAsioControlPanel}
        variant="secondary"
        size="sm"
        class="shrink-0 text-xs px-3.5 py-1.5"
      >
        <Sliders class="w-3.5 h-3.5" />
        {i18n.t("settings.audioAsioControlPanelBtn")}
      </Button>
    </div>
  {/if}
</div>
