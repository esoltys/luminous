<script lang="ts">
  import { collectionStore } from "../stores/collection.svelte";
  import { i18n } from "../stores/i18n.svelte";
  import { loudnessStore } from "../stores/loudness.svelte";
  import { onMount } from "svelte";
  import Toggle from "./Toggle.svelte";
  import Button from "./Button.svelte";
  import {
    FolderIcon as Folder,
    PlusIcon as Plus,
    TrashIcon as Trash2,
    ArrowsClockwiseIcon as RefreshCw,
    ArrowCounterClockwiseIcon as RotateCcw,
    ClockIcon as Clock,
    PulseIcon as Activity,
    WarningIcon as AlertTriangle
  } from "phosphor-svelte";

  onMount(() => {
    loudnessStore.init();
  });

  async function handleRemoveDirectory(path: string) {
    if (confirm(i18n.t('settings.confirmRemoveFolder', { path }))) {
      await collectionStore.removeDirectory(path);
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

<div class="bg-brand-sidebar border border-brand-border rounded-xl p-6 space-y-4">
  <div class="pb-3 flex justify-between items-center">
    <div class="flex items-center gap-3">
      <div class="p-2 rounded-xl bg-brand-accent/15 text-brand-accent-text shrink-0">
        <Folder class="w-5 h-5" />
      </div>
      <div class="space-y-1 min-w-0">
        <h3 class="font-bold text-sm text-brand-text-primary">{i18n.t('settings.watchedFoldersTitle')}</h3>
        <p class="text-xs text-brand-text-secondary leading-relaxed text-pretty">{i18n.t('settings.watchedFoldersSubtitle', {}, 'Manage directories to scan for music files.')}</p>
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
        <p class="text-xs text-brand-text-secondary mb-4 text-pretty">{i18n.t('settings.noFoldersText')}</p>
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
        <p class="text-xs text-brand-text-secondary leading-relaxed text-pretty">{i18n.t('settings.rescanSubtitle')}</p>
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
        <p class="text-xs text-brand-text-secondary text-pretty">{i18n.t('settings.watchRealtimeHint')}</p>
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
        <p class="text-xs text-brand-text-secondary text-pretty">{i18n.t('settings.scanOnStartupHint')}</p>
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
