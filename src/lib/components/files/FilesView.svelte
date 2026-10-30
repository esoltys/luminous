<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { revealItemInDir } from "@tauri-apps/plugin-opener";
  import {
    PlayIcon,
    DeviceMobileSpeakerIcon,
    ArrowSquareOutIcon,
    GearIcon,
    FolderIcon,
    HardDrivesIcon,
    CloudIcon,
    PlusIcon,
    CaretRightIcon,
  } from "phosphor-svelte";
  import { navigationStore } from "../../stores/navigation.svelte";
  import { filesStore } from "../../stores/files.svelte";
  import { collectionStore } from "../../stores/collection.svelte";
  import { playerStore } from "../../stores/player.svelte";
  import { toastStore } from "../../stores/toast.svelte";
  import { i18n } from "../../stores/i18n.svelte";
  import { formatFileSize } from "../../utils/formatters";
  import { deviceName } from "../../utils/devices";
  import { isWindows, isLinux } from "../../platform";
  import type { ConnectedDevice, DeviceSyncState, DirListing, FileEntry, FileSource, Song } from "../../types";
  import Button from "../Button.svelte";
  import FileBrowser from "./FileBrowser.svelte";

  const location = $derived(navigationStore.filesLocation);
  const sourceName = $derived(location ? filesStore.name(location.source) : "");

  let listing = $state<DirListing | null>(null);
  let selectedName = $state<string | null>(null);
  let reloadToken = $state(0);
  let devicePickerOpen = $state(false);

  // Statuses change when the library does; the source list is re-read
  // after every library change and scan, so follow it.
  let seenSources: unknown = null;
  $effect(() => {
    const sources = filesStore.sources;
    if (seenSources !== null && sources !== seenSources) reloadToken++;
    seenSources = sources;
  });

  const isLocal = $derived(location?.source.kind === "folder");
  const revealLabel = $derived(
    i18n.t(isWindows ? "files.revealWindows" : isLinux ? "files.revealLinux" : "files.revealMac")
  );
  const title = $derived(location && location.path.length > 0 ? location.path[location.path.length - 1] : sourceName);
  const unreachableLabel = $derived(i18n.t("files.unreachable", { name: sourceName }));

  function navigate(path: string[]) {
    if (!location) return;
    navigationStore.filesLocation = { source: location.source, path };
  }

  async function songsHere(): Promise<Song[]> {
    if (!location) return [];
    return invoke<Song[]>("get_source_songs", { source: location.source, path: location.path });
  }

  async function playFolder() {
    try {
      const songs = await songsHere();
      if (songs.length === 0) {
        toastStore.show(i18n.t("files.nothingToPlay"), "info");
        return;
      }
      await playerStore.playSongs(songs.map((s) => s.id), 0, undefined, undefined, title);
    } catch (err) {
      console.error("Failed to play folder:", err);
      toastStore.show(i18n.t("files.playFailed"), "error");
    }
  }

  async function playEntry(entry: FileEntry) {
    if (entry.songId === null) {
      toastStore.show(
        i18n.t(isLocal && entry.status === "notScanned" ? "files.notInLibraryScan" : "files.notInLibrary"),
        "info"
      );
      return;
    }
    try {
      const songs = await songsHere();
      const start = Math.max(0, songs.findIndex((s) => s.id === entry.songId));
      const ids = songs.length > 0 ? songs.map((s) => s.id) : [entry.songId];
      await playerStore.playSongs(ids, start, undefined, undefined, title);
    } catch (err) {
      console.error("Failed to play file:", err);
      toastStore.show(i18n.t("files.playFailed"), "error");
    }
  }

  async function addToDeviceSync(device: ConnectedDevice) {
    devicePickerOpen = false;
    const folderPath = listing?.location;
    if (!folderPath) return;
    const name = deviceName(device);
    try {
      const state = await invoke<DeviceSyncState>("get_device_sync_state", { deviceId: device.id });
      const already = state.settings.sources.some((s) => s.kind === "folder" && s.path === folderPath);
      if (already) {
        toastStore.show(i18n.t("files.alreadySyncing", { folder: title, device: name }), "info");
        return;
      }
      // Previewing saves the selection; the device's Sync tab shows the plan.
      await invoke("preview_device_sync", {
        deviceId: device.id,
        settings: { ...state.settings, sources: [...state.settings.sources, { kind: "folder", path: folderPath }] },
      });
      toastStore.show(i18n.t("files.addedToSync", { folder: title, device: name }), "success");
    } catch (err) {
      console.error("Failed to add folder to device sync:", err);
      toastStore.show(i18n.t("files.addToSyncFailed", { device: name }), "error");
    }
  }

  function onAddToDeviceSync() {
    if (filesStore.devices.length === 1) addToDeviceSync(filesStore.devices[0]);
    else devicePickerOpen = !devicePickerOpen;
  }

  async function reveal() {
    if (!listing?.location) return;
    try {
      await revealItemInDir(listing.location);
    } catch (err) {
      console.error("Failed to show folder in the file manager:", err);
    }
  }

  async function scanFolder() {
    if (!location) return;
    try {
      await invoke("scan_source_folder", { source: location.source, path: location.path });
    } catch (err) {
      console.error("Failed to scan folder:", err);
      toastStore.show(i18n.t("files.scanFailed"), "error");
    }
  }

  function summary(l: DirListing): string {
    const files =
      l.totalBytes === null
        ? i18n.plural("files.summaryFilesNoSize", l.fileCount)
        : i18n.plural("files.summaryFiles", l.fileCount, { size: formatFileSize(l.totalBytes) });
    return `${files} ${i18n.plural("files.summaryInLibrary", l.inLibraryCount)}`;
  }

  function sourceIcon(source: FileSource) {
    return source.kind === "subsonic" ? HardDrivesIcon : source.kind === "webdav" ? CloudIcon : FolderIcon;
  }
</script>

<svelte:window onclick={() => (devicePickerOpen = false)} />

<div class="flex-1 flex flex-col h-full min-h-0 bg-brand-main text-brand-text-primary select-none overflow-hidden" data-testid="files-view">
  {#if location}
    <div class="flex-1 min-h-0 flex flex-col px-6 pt-4 gap-4" class:pb-28={!!playerStore.currentSong} class:pb-6={!playerStore.currentSong}>
      <header class="flex items-end justify-between gap-4 flex-wrap">
        <div class="min-w-0">
          <nav class="flex items-center gap-1.5 text-sm text-brand-text-secondary min-w-0" aria-label={i18n.t("files.breadcrumb")}>
            <button class="hover:text-brand-text-primary truncate" onclick={() => navigationStore.viewFiles()}>{i18n.t("sidebar.files")}</button>
            <span aria-hidden="true">/</span>
            <button
              class="truncate {location.path.length === 0 ? 'text-brand-text-primary font-semibold' : 'hover:text-brand-text-primary'}"
              onclick={() => navigate([])}
            >{sourceName}</button>
            {#each location.path as name, i (i)}
              <span aria-hidden="true">/</span>
              <button
                class="truncate {i === location.path.length - 1 ? 'text-brand-text-primary font-semibold' : 'hover:text-brand-text-primary'}"
                onclick={() => navigate(location.path.slice(0, i + 1))}
              >{name}</button>
            {/each}
          </nav>
          <h1 class="text-3xl font-heading font-bold text-brand-text-primary leading-snug truncate py-0.5" title={title}>{title}</h1>
          {#if listing?.location}
            <p class="text-xs text-brand-text-secondary truncate select-text" title={listing.location}>{listing.location}</p>
          {/if}
        </div>
        <div class="flex items-center gap-2 shrink-0">
          <Button variant="primary" onclick={playFolder}>
            <PlayIcon weight="fill" class="w-4 h-4" />
            {i18n.t("files.playFolder")}
          </Button>
          {#if isLocal && filesStore.devices.length > 0 && listing?.location}
            <div class="relative">
              <Button onclick={(e) => { e.stopPropagation(); onAddToDeviceSync(); }}>
                <DeviceMobileSpeakerIcon class="w-4 h-4" />
                {i18n.t("files.addToDeviceSync")}
              </Button>
              {#if devicePickerOpen}
                <div class="absolute right-0 top-full mt-1 z-20 min-w-48 rounded-lg border border-brand-border/60 bg-brand-sidebar shadow-xl p-1" role="menu">
                  {#each filesStore.devices as device (device.id)}
                    <button
                      class="w-full text-left px-3 py-1.5 rounded-md text-sm hover:bg-brand-accent/10"
                      role="menuitem"
                      onclick={(e) => { e.stopPropagation(); addToDeviceSync(device); }}
                    >{deviceName(device)}</button>
                  {/each}
                </div>
              {/if}
            </div>
          {/if}
          {#if isLocal && listing?.location}
            <button
              class="w-9 h-9 flex items-center justify-center rounded-full text-brand-text-secondary hover:text-brand-text-primary hover:bg-brand-accent/10 transition-colors"
              onclick={reveal}
              title={revealLabel}
              aria-label={revealLabel}
            >
              <ArrowSquareOutIcon class="w-4 h-4" />
            </button>
          {/if}
          <button
            class="w-9 h-9 flex items-center justify-center rounded-full text-brand-text-secondary hover:text-brand-text-primary hover:bg-brand-accent/10 transition-colors"
            onclick={() => navigationStore.openSettings("sources")}
            title={i18n.t("files.folderSettings")}
            aria-label={i18n.t("files.folderSettings")}
          >
            <GearIcon class="w-4 h-4" />
          </button>
        </div>
      </header>

      <FileBrowser
        source={location.source}
        path={location.path}
        onnavigate={navigate}
        onplay={playEntry}
        {reloadToken}
        {unreachableLabel}
        bind:listing
        bind:selectedName
      >
        {#snippet footer()}
          {#if listing}
            <div class="flex items-center justify-between gap-3 px-4 py-2.5 text-xs text-brand-text-secondary">
              <span class="truncate">{summary(listing)}</span>
              {#if isLocal && !listing.excluded}
                <Button size="sm" class="shrink-0 whitespace-nowrap" onclick={scanFolder} disabled={collectionStore.isScanning}>
                  {collectionStore.isScanning ? i18n.t("files.scanning") : i18n.t("files.scanFolder")}
                </Button>
              {/if}
            </div>
          {/if}
        {/snippet}
      </FileBrowser>
    </div>
  {:else}
    <div class="flex-1 overflow-y-auto px-6 pt-4" class:pb-28={!!playerStore.currentSong} class:pb-12={!playerStore.currentSong}>
      <header class="flex items-end justify-between gap-4 mb-4">
        <h1 class="text-3xl font-heading font-bold text-brand-text-primary leading-snug">{i18n.t("sidebar.files")}</h1>
        <Button onclick={() => navigationStore.openSettings("sources")}>
          <PlusIcon class="w-4 h-4" />
          {i18n.t("files.addSource")}
        </Button>
      </header>
      {#if filesStore.loaded && filesStore.sources.length === 0}
        <p class="text-sm text-brand-text-secondary">{i18n.t("files.noSources")}</p>
      {:else}
        <ul class="flex flex-col gap-1 max-w-3xl">
          {#each filesStore.sources as entry (`${entry.source.kind}:${entry.source.id}`)}
            {@const Icon = sourceIcon(entry.source)}
            <li>
              <button
                class="w-full flex items-center gap-3 px-4 py-3 rounded-xl border border-brand-border/40 hover:bg-brand-accent/10 transition-colors text-left {entry.available ? '' : 'opacity-60'}"
                onclick={() => navigationStore.viewFiles(entry.source)}
              >
                <Icon class="w-5 h-5 shrink-0 text-brand-text-secondary" />
                <span class="flex-1 min-w-0">
                  <span class="block text-sm font-semibold truncate">{entry.name}</span>
                  <span class="block text-xs text-brand-text-secondary">
                    {i18n.t(`files.kind.${entry.source.kind}`)} · {i18n.plural("files.songCount", entry.songCount)}
                  </span>
                </span>
                <CaretRightIcon class="w-4 h-4 text-brand-text-secondary" />
              </button>
            </li>
          {/each}
        </ul>
      {/if}
    </div>
  {/if}
</div>
