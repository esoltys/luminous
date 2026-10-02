<script lang="ts">
  import { collectionStore } from "../stores/collection.svelte";
  import { playlistsStore } from "../stores/playlists.svelte";
  import { navigationStore } from "../stores/navigation.svelte";
  import { i18n } from "../stores/i18n.svelte";
  import { loudnessStore } from "../stores/loudness.svelte";
  import { tasksStore } from "../stores/tasks.svelte";
  import { onMount } from "svelte";
  import { hierarchySidecarStore } from "../stores/hierarchySidecar.svelte";
  import { prefs } from "../stores/prefs.svelte";
  import { confirm } from "@tauri-apps/plugin-dialog";
  import Toggle from "./Toggle.svelte";
  import Select from "./Select.svelte";
  import HelpTip from "./HelpTip.svelte";
  import Button from "./Button.svelte";
  import LibraryBadge from "./LibraryBadge.svelte";
  import FolderEditModal from "./FolderEditModal.svelte";
  import WebDavModal from "./WebDavModal.svelte";
  import SubsonicModal from "./SubsonicModal.svelte";
  import ConfirmDialog from "./ConfirmDialog.svelte";
  import { toastStore } from "../stores/toast.svelte";
  import type { MusicDirectory, SubsonicServer, SubsonicSyncStats, WebDavServer } from "../types";
  import { combineWebdavPath } from "../webdavDisplay";
  import { stripEnclosingQuotes } from "../utils/filterParser";
  import { getDirectoryDisplayName } from "../utils/pathUtils";
  import { formatFileSize, formatNumber } from "../utils/formatters";
  import { invoke } from "@tauri-apps/api/core";
  import {
    FolderIcon as Folder,
    PlusIcon as Plus,
    TrashIcon as Trash2,
    PencilSimpleIcon as Edit3,
    ArrowsClockwiseIcon as RefreshCw,
    ArrowCounterClockwiseIcon as RotateCcw,
    ClockIcon as Clock,
    PulseIcon as Activity,
    WarningIcon as AlertTriangle,
    CloudIcon as Cloud,
    CircleNotchIcon as LoaderCircle,
    PlaylistIcon as ListMusic,
  } from "phosphor-svelte";

  let editingDirectory = $state<MusicDirectory | null>(null);
  let isWebdavModalOpen = $state(false);
  let editingWebdavServer = $state<WebDavServer | null>(null);
  let syncingServerId = $state<number | null>(null);
  let syncFeedback = $state<string | null>(null);
  let showSidecarConfirmModal = $state(false);
  let isSweepingArtwork = $state(false);

  function handleToggleSaveArtwork(v: boolean) {
    if (v) {
      showSidecarConfirmModal = true;
    } else {
      prefs.setSaveArtworkToFolders(false);
    }
  }

  async function handleConfirmSaveArtwork() {
    showSidecarConfirmModal = false;
    await prefs.setSaveArtworkToFolders(true);
    isSweepingArtwork = true;
    const taskId = "artwork-sweep";
    tasksStore.startTask({
      id: taskId,
      label: i18n.t("tasks.exportingArtwork", {}, "Exporting artwork…"),
    });
    try {
      const res = await invoke<{
        album_covers_exported: number;
        artist_portraits_exported: number;
        band_logos_exported: number;
        banners_exported: number;
      }>("sweep_artwork_to_folders");
      const total =
        (res?.album_covers_exported ?? 0) +
        (res?.artist_portraits_exported ?? 0) +
        (res?.band_logos_exported ?? 0) +
        (res?.banners_exported ?? 0);
      if (total > 0) {
        tasksStore.completeTask(taskId, i18n.t("settings.artworkSweepSuccess", { count: total }));
      } else {
        tasksStore.completeTask(taskId, i18n.t("settings.artworkSweepNone"));
      }
    } catch (e: any) {
      console.error("Failed to sweep artwork:", e);
      const errMsg = String(e?.message || e);
      tasksStore.failTask(taskId, errMsg);
    } finally {
      isSweepingArtwork = false;
    }
  }

  function handleCancelSaveArtwork() {
    showSidecarConfirmModal = false;
  }
  /** Live reachability per server id, refreshed whenever the list loads —
   * `undefined` while the check is still in flight. This is a network call,
   * unlike a watched folder's `is_available` (a cheap local `Path::exists()`
   * recomputed on every fetch), so it runs async per-server rather than
   * blocking the list render. */
  let webdavConnected = $state<Record<number, boolean | undefined>>({});

  async function loadWebdavServers() {
    try {
      await collectionStore.refreshWebDavServers();
      checkWebdavConnections();
    } catch (e) {
      console.error("Failed to load WebDAV servers:", e);
    }
  }

  function checkWebdavConnections() {
    for (const server of collectionStore.webdavServers) {
      invoke<boolean>("check_webdav_connection", { id: server.id })
        .then((ok) => { webdavConnected[server.id] = ok; })
        .catch(() => { webdavConnected[server.id] = false; });
    }
  }

  async function handleRemoveWebdavServer(server: WebDavServer) {
    if (await confirm(i18n.t("settings.confirmRemoveWebdavServer", { name: server.name }))) {
      try {
        await invoke("delete_webdav_server", { id: server.id });
        await loadWebdavServers();
        await collectionStore.refreshLibrary();
        await collectionStore.refreshStats();
      } catch (e) {
        console.error("Failed to delete WebDAV server:", e);
      }
    }
  }

  function isServerSyncing(serverId: number): boolean {
    return syncingServerId === serverId || tasksStore.isTaskActive(`webdav-sync-${serverId}`);
  }

  async function handleSyncWebdavServer(server: WebDavServer) {
    if (isServerSyncing(server.id)) return;
    syncingServerId = server.id;
    syncFeedback = null;
    const taskId = `webdav-sync-${server.id}`;
    tasksStore.startTask({
      id: taskId,
      label: i18n.t("tasks.syncingWebdav", { name: server.name }, `Syncing ${server.name}...`),
      contextName: server.name,
    });
    try {
      const stats = await invoke<{ added: number; updated: number; removed: number; errors: number }>(
        "sync_webdav_server",
        { id: server.id }
      );
      syncFeedback = i18n.t("settings.webdavSyncComplete", {
        added: stats.added,
        updated: stats.updated,
        errors: stats.errors,
      });
      tasksStore.completeTask(taskId, `${server.name}: ${syncFeedback}`);
      await loadWebdavServers();
    } catch (e: any) {
      console.error("Failed to sync WebDAV server:", e);
      const errMsg = String(e?.message || e);
      syncFeedback = errMsg;
      tasksStore.failTask(taskId, errMsg);
    } finally {
      syncingServerId = null;
    }
  }

  // OpenSubsonic media servers (#916) — mirrors the WebDAV list above.
  // Disk Size covers the music files plus the covers cache; the tooltip
  // breaks the total down (values come from get_library_stats).
  // GB at the headline's two decimals so the parts visibly add up to it.
  const formatDiskSize = (bytes: number) =>
    bytes >= 1073741824 ? `${formatNumber(bytes / 1073741824, { minimumFractionDigits: 2, maximumFractionDigits: 2 })} GB` : formatFileSize(bytes);
  const diskSizeLabel = $derived.by(() => {
    const { total_filesize_bytes, album_art_bytes, artist_art_bytes } = collectionStore.stats;
    const total = total_filesize_bytes + (album_art_bytes ?? 0) + (artist_art_bytes ?? 0);
    return `${formatNumber(total / 1073741824, { minimumFractionDigits: 2, maximumFractionDigits: 2 })} GB`;
  });
  const diskSizeBreakdown = $derived(
    [
      i18n.t('settings.statsSizeMusic', { size: formatDiskSize(collectionStore.stats.total_filesize_bytes) }),
      i18n.t('settings.statsSizeAlbumArt', { size: formatDiskSize(collectionStore.stats.album_art_bytes) }),
      i18n.t('settings.statsSizeArtistArt', { size: formatDiskSize(collectionStore.stats.artist_art_bytes) }),
    ].join("\n"),
  );

  let isSubsonicModalOpen = $state(false);
  let editingSubsonicServer = $state<SubsonicServer | null>(null);
  let syncingSubsonicId = $state<number | null>(null);
  let subsonicSyncFeedback = $state<string | null>(null);
  let subsonicConnected = $state<Record<number, boolean | undefined>>({});

  async function loadSubsonicServers() {
    await collectionStore.refreshSubsonicServers();
    checkSubsonicConnections();
  }

  function checkSubsonicConnections() {
    for (const server of collectionStore.subsonicServers) {
      invoke("check_subsonic_connection", { id: server.id })
        .then(() => { subsonicConnected[server.id] = true; })
        .catch(() => { subsonicConnected[server.id] = false; });
    }
  }

  function isSubsonicSyncing(server: SubsonicServer): boolean {
    return (
      syncingSubsonicId === server.id ||
      server.syncStatus === "syncing" ||
      tasksStore.isTaskActive(`subsonic-sync-${server.id}`)
    );
  }

  async function handleSyncSubsonicServer(server: SubsonicServer) {
    if (isSubsonicSyncing(server)) return;
    syncingSubsonicId = server.id;
    subsonicSyncFeedback = null;
    // Task progress/completion is driven by the `subsonic-sync-progress`
    // listener in collectionStore, so auto-syncs get the same task row.
    try {
      const stats = await invoke<SubsonicSyncStats>("sync_subsonic_server", { id: server.id });
      subsonicSyncFeedback = `${server.name}: ${i18n.t("settings.subsonicSyncComplete", { ...stats })}`;
    } catch (e: any) {
      console.error("Failed to sync media server:", e);
      subsonicSyncFeedback = i18n.t("settings.subsonicSyncFailed", {
        name: server.name,
        error: String(e?.message || e),
      });
    } finally {
      syncingSubsonicId = null;
      await loadSubsonicServers();
    }
  }

  async function handleRemoveSubsonicServer(server: SubsonicServer) {
    if (!(await confirm(i18n.t("settings.confirmRemoveSubsonicServer", { name: server.name })))) return;
    try {
      await invoke("delete_subsonic_server", { id: server.id });
      await loadSubsonicServers();
      await collectionStore.refreshLibrary();
      await collectionStore.refreshStats();
    } catch (e) {
      console.error("Failed to delete media server:", e);
    }
  }

  function getSubsonicStatusText(server: SubsonicServer): string {
    if (isSubsonicSyncing(server)) return i18n.t("settings.webdavStatusSyncing");
    if (server.lastSyncedAt) {
      return i18n.t("settings.webdavStatusSynced", { time: new Date(server.lastSyncedAt * 1000).toLocaleString() });
    }
    return i18n.t("settings.webdavStatusNeverSynced");
  }

  function getSubsonicNextSyncText(server: SubsonicServer): string | null {
    if (!server.autoSyncEnabled || !server.enabled || isSubsonicSyncing(server)) return null;
    if (!server.nextAutoSyncAt) return null;
    const minutes = Math.max(1, Math.round((server.nextAutoSyncAt * 1000 - Date.now()) / 60000));
    return i18n.t("settings.webdavNextSyncIn", { minutes });
  }

  function getSubsonicServerLabel(server: SubsonicServer): string {
    const kind = [server.serverType, server.serverVersion].filter(Boolean).join(" ");
    return kind ? `${server.url} · ${kind}` : server.url;
  }

  onMount(() => {
    loudnessStore.init();
    loadWebdavServers();
    loadSubsonicServers();
    hierarchySidecarStore.refresh().catch((e) => console.error("Failed to load the default library:", e));
  });

  async function handleRemoveDirectory(path: string) {
    if (await confirm(i18n.t('settings.confirmRemoveFolder', { path }))) {
      await collectionStore.removeDirectory(path);
      // Removing the default library's folder unlinks it backend-side.
      await hierarchySidecarStore.refresh().catch(() => {});
    }
  }

  /** A failed link says why; otherwise a linked file that can't be loaded
   * (malformed JSON) stays flagged here until it's fixed. */
  let linkError = $state<string | null>(null);
  let defaultLibraryError = $derived(linkError ?? hierarchySidecarStore.errorText);

  async function handleDefaultLibraryChange(select: HTMLSelectElement) {
    linkError = null;
    try {
      await hierarchySidecarStore.set(select.value || null);
    } catch (e) {
      linkError = String(e);
      // The link was refused — put the picker back on what's actually linked.
      select.value = hierarchySidecarStore.path ?? '';
    }
  }

  async function handleCreateFolderPlaylist(dir: MusicDirectory) {
    const cleanPath = stripEnclosingQuotes(dir.path);
    const name = getDirectoryDisplayName({ nickname: dir.nickname, path: cleanPath });
    const escapedPath = cleanPath.replace(/\\/g, "\\\\").replace(/"/g, '\\"');
    try {
      const playlist = await playlistsStore.createPlaylist(name);
      await playlistsStore.updatePlaylistSpec(playlist.id, `folder:="${escapedPath}"`);
      navigationStore.viewPlaylist(playlist.id);
    } catch (err) {
      console.error("Failed to create smart playlist from folder:", err);
    }
  }

  function getWebdavStatusText(server: WebDavServer): string {
    if (isServerSyncing(server.id) || server.syncStatus === "syncing") return i18n.t("settings.webdavStatusSyncing");
    if (server.lastSyncedAt) {
      return i18n.t("settings.webdavStatusSynced", { time: new Date(server.lastSyncedAt * 1000).toLocaleString() });
    }
    return i18n.t("settings.webdavStatusNeverSynced");
  }

  function getWebdavNextSyncText(server: WebDavServer): string | null {
    if (!server.autoSyncEnabled || !server.enabled) return null;
    if (isServerSyncing(server.id) || server.syncStatus === "syncing") return null;
    if (!server.nextAutoSyncAt) return null;
    const minutes = Math.max(1, Math.round((server.nextAutoSyncAt * 1000 - Date.now()) / 60000));
    return i18n.t("settings.webdavNextSyncIn", { minutes });
  }

  function getPhaseDisplayName(phase: string | undefined): string {
    if (!phase) return i18n.t('sidebar.scanning');
    switch (phase) {
      case "discovering":
        return i18n.t('settings.phaseDiscovering');
      case "reading_tags":
        return i18n.t('settings.phaseReadingTags');
      case "checking_missing":
        return i18n.t('settings.phaseCheckingMissing');
      case "resolving_artwork":
        return i18n.t('settings.phaseResolvingArtwork');
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
        <div class="flex items-center gap-3.5 min-w-0 flex-1">
          <div class="min-w-0 space-y-1">
            <div class="flex items-center gap-2.5 min-w-0">
              <LibraryBadge directory={dir} size="sm" />
              <p class="text-xs text-brand-text-secondary truncate" title={dir.path}>{dir.path}</p>
            </div>
            <p class="text-xs" class:text-brand-text-secondary={dir.is_available !== false} class:text-red-400={dir.is_available === false}>
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
        <div class="flex items-center gap-1.5 shrink-0 ml-3">
          <button
            onclick={() => collectionStore.startScan(false)}
            disabled={collectionStore.isScanning}
            class="p-2 rounded-lg bg-brand-main hover:bg-brand-sidebar text-brand-text-secondary hover:text-brand-text-primary border border-brand-border hover:border-brand-accent/40 transition-colors disabled:opacity-50"
            title={i18n.t('settings.folderSyncNowHint')}
          >
            {#if collectionStore.isScanning}
              <LoaderCircle class="w-4 h-4 animate-spin text-brand-accent-text" />
            {:else}
              <RefreshCw class="w-4 h-4 text-brand-accent-text" />
            {/if}
          </button>
          <button
            onclick={() => handleCreateFolderPlaylist(dir)}
            class="p-2 rounded-lg bg-brand-main hover:bg-brand-sidebar text-brand-text-secondary hover:text-brand-text-primary border border-brand-border hover:border-brand-accent/40 transition-colors"
            title={i18n.t('settings.folderCreatePlaylist', {}, 'Create Smart Playlist from folder')}
          >
            <ListMusic class="w-4 h-4 text-brand-accent-text" />
          </button>
          <button
            onclick={() => { editingDirectory = dir; }}
            class="p-2 rounded-lg bg-brand-main hover:bg-brand-sidebar text-brand-text-secondary hover:text-brand-text-primary border border-brand-border hover:border-brand-accent/40 transition-colors"
            title={i18n.t('settings.folderItemEdit', {}, 'Edit folder details')}
          >
            <Edit3 class="w-4 h-4" />
          </button>
          <button
            onclick={() => handleRemoveDirectory(dir.path)}
            class="p-2 rounded-lg bg-brand-main hover:bg-red-950/20 text-brand-text-secondary hover:text-red-400 border border-brand-border hover:border-red-900/30 transition-colors"
            title={i18n.t('settings.folderItemStopWatch')}
          >
            <Trash2 class="w-4 h-4 text-brand-accent-text" />
          </button>
        </div>
      </div>
    {/each}

    {#if collectionStore.directories.length > 0}
      <div class="flex items-center justify-between gap-4 pt-2">
        <div class="flex flex-col gap-0.5 min-w-0">
          <label for="default-library-select" class="text-sm font-medium text-brand-text-primary">{i18n.t('settings.defaultLibraryLabel')}</label>
          <p class="text-xs text-brand-text-secondary">{i18n.t('settings.defaultLibraryHint')}</p>
          {#if defaultLibraryError}
            <p class="text-xs text-red-400 break-words" role="alert">{defaultLibraryError}</p>
          {/if}
        </div>
        <Select
          id="default-library-select"
          value={hierarchySidecarStore.path ?? ''}
          onchange={(e) => handleDefaultLibraryChange(e.currentTarget)}
          class="shrink-0 max-w-[45%] truncate bg-brand-main border border-brand-border hover:border-brand-accent/60 text-brand-text-primary text-xs rounded-full pl-3.5 pr-8 py-1.5 focus:outline-none focus:border-brand-accent transition-all font-medium"
        >
          <option value="">{i18n.t('settings.defaultLibraryNone')}</option>
          {#each collectionStore.directories as dir (dir.path)}
            <option value={dir.path}>{getDirectoryDisplayName(dir)}</option>
          {/each}
        </Select>
      </div>
    {/if}

    {#if editingDirectory}
      <FolderEditModal
        directory={editingDirectory}
        onClose={() => { editingDirectory = null; }}
      />
    {/if}

    {#if collectionStore.directories.length === 0}
      <div class="border border-dashed border-brand-border rounded-xl py-12 text-center text-brand-text-secondary">
        <Folder class="w-12 h-12 mx-auto mb-2 text-brand-text-secondary/50" />
        <h4 class="font-semibold text-brand-text-primary mb-1">{i18n.t('settings.noFoldersTitle')}</h4>
        <p class="text-xs text-brand-text-secondary mb-4 text-pretty">{i18n.t('settings.noFoldersText')}</p>
      </div>
    {/if}
  </div>
</div>

<!-- WebDAV Remote Libraries (#682) -->
<div class="bg-brand-sidebar border border-brand-border rounded-xl p-6 space-y-4">
  <div class="pb-3 flex justify-between items-center">
    <div class="flex items-center gap-3">
      <div class="p-2 rounded-xl bg-brand-accent/15 text-brand-accent-text shrink-0">
        <Cloud class="w-5 h-5" />
      </div>
      <div class="space-y-1 min-w-0">
        <h3 class="font-bold text-sm text-brand-text-primary">{i18n.t('settings.webdavTitle')}</h3>
        <p class="text-xs text-brand-text-secondary leading-relaxed text-pretty">{i18n.t('settings.webdavSubtitle')}</p>
      </div>
    </div>
    <Button
      onclick={() => {
        editingWebdavServer = null;
        isWebdavModalOpen = true;
      }}
      variant="primary"
      size="sm"
    >
      <Plus class="w-4 h-4" /> {i18n.t('settings.addWebdavServer')}
    </Button>
  </div>

  {#if syncFeedback}
    <div class="p-3 bg-brand-main/60 border border-brand-border rounded-xl text-xs text-brand-text-secondary flex items-center justify-between">
      <span>{syncFeedback}</span>
      <button
        onclick={() => { syncFeedback = null; }}
        class="text-brand-text-secondary hover:text-brand-text-primary font-bold ml-2"
      >
        ✕
      </button>
    </div>
  {/if}

  <div class="space-y-2">
    {#each collectionStore.webdavServers as server (server.id)}
      <div class="flex items-center justify-between bg-brand-main/50 border border-brand-border/60 rounded-xl p-4 hover:border-brand-border transition-colors">
        <div class="flex items-center gap-3.5 min-w-0 flex-1">
          <div class="min-w-0 space-y-1">
            <div class="flex items-center gap-2.5 min-w-0">
              <LibraryBadge
                directory={{
                  path: combineWebdavPath(server.url, server.remotePath),
                  nickname: server.nickname,
                  icon: server.icon,
                  color: server.color,
                  is_available: webdavConnected[server.id] !== false,
                }}
                size="sm"
              />
              <p class="text-xs text-brand-text-secondary truncate" title={combineWebdavPath(server.url, server.remotePath)}>
                {combineWebdavPath(server.url, server.remotePath)}
              </p>
            </div>
            <p class="text-xs" class:text-brand-text-secondary={webdavConnected[server.id] !== false} class:text-red-400={webdavConnected[server.id] === false}>
              {#if webdavConnected[server.id] === false}
                <span class="flex items-center gap-1">
                  <AlertTriangle class="w-3 h-3" />
                  {i18n.t('settings.webdavStatusDisconnected')}
                </span>
              {:else}
                <span class="flex items-center gap-1">
                  {#if isServerSyncing(server.id) || server.syncStatus === "syncing"}
                    <LoaderCircle class="w-3 h-3 animate-spin text-brand-accent-text" />
                  {/if}
                  {getWebdavStatusText(server)}
                </span>
              {/if}
            </p>
            {#if getWebdavNextSyncText(server)}
              <p class="text-xs text-brand-text-secondary/70">{getWebdavNextSyncText(server)}</p>
            {/if}
          </div>
        </div>

        <div class="flex items-center gap-1.5 shrink-0 ml-3">
          <button
            onclick={() => handleSyncWebdavServer(server)}
            disabled={isServerSyncing(server.id)}
            class="p-2 rounded-lg bg-brand-main hover:bg-brand-sidebar text-brand-text-secondary hover:text-brand-text-primary border border-brand-border hover:border-brand-accent/40 transition-colors disabled:opacity-50"
            title={i18n.t('settings.webdavSyncBtn')}
          >
            {#if isServerSyncing(server.id)}
              <LoaderCircle class="w-4 h-4 animate-spin text-brand-accent-text" />
            {:else}
              <RefreshCw class="w-4 h-4 text-brand-accent-text" />
            {/if}
          </button>
          <button
            onclick={() => {
              editingWebdavServer = server;
              isWebdavModalOpen = true;
            }}
            class="p-2 rounded-lg bg-brand-main hover:bg-brand-sidebar text-brand-text-secondary hover:text-brand-text-primary border border-brand-border hover:border-brand-accent/40 transition-colors"
            title={i18n.t('settings.webdavItemEdit')}
          >
            <Edit3 class="w-4 h-4" />
          </button>
          <button
            onclick={() => handleRemoveWebdavServer(server)}
            class="p-2 rounded-lg bg-brand-main hover:bg-red-950/20 text-brand-text-secondary hover:text-red-400 border border-brand-border hover:border-red-900/30 transition-colors"
            title={i18n.t('settings.confirmRemoveWebdavServer', { name: server.name })}
          >
            <Trash2 class="w-4 h-4 text-brand-accent-text" />
          </button>
        </div>
      </div>
    {/each}

    {#if collectionStore.webdavServers.length === 0}
      <div class="border border-dashed border-brand-border rounded-xl py-8 text-center text-brand-text-secondary">
        <Cloud class="w-10 h-10 mx-auto mb-2 text-brand-text-secondary/50" />
        <h4 class="font-semibold text-brand-text-primary mb-1 text-xs">{i18n.t('settings.webdavNoServersTitle')}</h4>
        <p class="text-xs text-brand-text-secondary text-pretty">{i18n.t('settings.webdavNoServersText')}</p>
      </div>
    {/if}
  </div>
</div>

{#if isWebdavModalOpen}
  <WebDavModal
    server={editingWebdavServer}
    onClose={() => { isWebdavModalOpen = false; }}
    onSaved={() => {
      isWebdavModalOpen = false;
      loadWebdavServers();
    }}
  />
{/if}

<!-- OpenSubsonic media servers (#916) -->
<div class="bg-brand-sidebar border border-brand-border rounded-xl p-6 space-y-4">
  <div class="pb-3 flex justify-between items-center">
    <div class="flex items-center gap-3">
      <div class="p-2 rounded-xl bg-brand-accent/15 text-brand-accent-text shrink-0">
        <Cloud class="w-5 h-5" />
      </div>
      <div class="space-y-1 min-w-0">
        <h3 class="font-bold text-sm text-brand-text-primary">{i18n.t('settings.subsonicTitle')}</h3>
        <p class="text-xs text-brand-text-secondary leading-relaxed text-pretty">{i18n.t('settings.subsonicSubtitle')}</p>
      </div>
    </div>
    <Button
      onclick={() => {
        editingSubsonicServer = null;
        isSubsonicModalOpen = true;
      }}
      variant="primary"
      size="sm"
    >
      <Plus class="w-4 h-4" /> {i18n.t('settings.addSubsonicServer')}
    </Button>
  </div>

  {#if subsonicSyncFeedback}
    <div class="p-3 bg-brand-main/60 border border-brand-border rounded-xl text-xs text-brand-text-secondary flex items-center justify-between">
      <span>{subsonicSyncFeedback}</span>
      <button
        onclick={() => { subsonicSyncFeedback = null; }}
        class="text-brand-text-secondary hover:text-brand-text-primary font-bold ml-2"
      >
        ✕
      </button>
    </div>
  {/if}

  <div class="space-y-2">
    {#each collectionStore.subsonicServers as server (server.id)}
      <div class="flex items-center justify-between bg-brand-main/50 border border-brand-border/60 rounded-xl p-4 hover:border-brand-border transition-colors" data-testid="subsonic-server-row">
        <div class="flex items-center gap-3.5 min-w-0 flex-1">
          <div class="min-w-0 space-y-1">
            <div class="flex items-center gap-2.5 min-w-0">
              <LibraryBadge
                directory={{
                  path: server.url,
                  nickname: server.nickname || server.name,
                  icon: server.icon,
                  color: server.color,
                  is_available: subsonicConnected[server.id] !== false,
                }}
                size="sm"
              />
              <p class="text-xs text-brand-text-secondary truncate" title={getSubsonicServerLabel(server)}>
                {getSubsonicServerLabel(server)}
              </p>
            </div>
            <p class="text-xs" class:text-brand-text-secondary={subsonicConnected[server.id] !== false} class:text-red-400={subsonicConnected[server.id] === false}>
              {#if subsonicConnected[server.id] === false}
                <span class="flex items-center gap-1">
                  <AlertTriangle class="w-3 h-3" />
                  {i18n.t('settings.webdavStatusDisconnected')}
                </span>
              {:else}
                <span class="flex items-center gap-1">
                  {#if isSubsonicSyncing(server)}
                    <LoaderCircle class="w-3 h-3 animate-spin text-brand-accent-text" />
                  {/if}
                  {getSubsonicStatusText(server)}
                </span>
              {/if}
            </p>
            {#if getSubsonicNextSyncText(server)}
              <p class="text-xs text-brand-text-secondary/70">{getSubsonicNextSyncText(server)}</p>
            {/if}
          </div>
        </div>

        <div class="flex items-center gap-1.5 shrink-0 ml-3">
          <button
            onclick={() => handleSyncSubsonicServer(server)}
            disabled={isSubsonicSyncing(server)}
            class="p-2 rounded-lg bg-brand-main hover:bg-brand-sidebar text-brand-text-secondary hover:text-brand-text-primary border border-brand-border hover:border-brand-accent/40 transition-colors disabled:opacity-50"
            title={i18n.t('settings.webdavSyncBtn')}
            aria-label={i18n.t('settings.webdavSyncBtn')}
          >
            {#if isSubsonicSyncing(server)}
              <LoaderCircle class="w-4 h-4 animate-spin text-brand-accent-text" />
            {:else}
              <RefreshCw class="w-4 h-4 text-brand-accent-text" />
            {/if}
          </button>
          <button
            onclick={() => {
              editingSubsonicServer = server;
              isSubsonicModalOpen = true;
            }}
            class="p-2 rounded-lg bg-brand-main hover:bg-brand-sidebar text-brand-text-secondary hover:text-brand-text-primary border border-brand-border hover:border-brand-accent/40 transition-colors"
            title={i18n.t('settings.subsonicItemEdit')}
            aria-label={i18n.t('settings.subsonicItemEdit')}
          >
            <Edit3 class="w-4 h-4" />
          </button>
          <button
            onclick={() => handleRemoveSubsonicServer(server)}
            class="p-2 rounded-lg bg-brand-main hover:bg-red-950/20 text-brand-text-secondary hover:text-red-400 border border-brand-border hover:border-red-900/30 transition-colors"
            title={i18n.t('settings.removeSubsonicServer')}
            aria-label={i18n.t('settings.removeSubsonicServer')}
          >
            <Trash2 class="w-4 h-4 text-brand-accent-text" />
          </button>
        </div>
      </div>
    {/each}

    {#if collectionStore.subsonicServers.length === 0}
      <div class="border border-dashed border-brand-border rounded-xl py-8 text-center text-brand-text-secondary">
        <Cloud class="w-10 h-10 mx-auto mb-2 text-brand-text-secondary/50" />
        <h4 class="font-semibold text-brand-text-primary mb-1 text-xs">{i18n.t('settings.subsonicNoServersTitle')}</h4>
        <p class="text-xs text-brand-text-secondary text-pretty">{i18n.t('settings.subsonicNoServersText')}</p>
      </div>
    {/if}
  </div>
</div>

{#if isSubsonicModalOpen}
  <SubsonicModal
    server={editingSubsonicServer}
    onClose={() => { isSubsonicModalOpen = false; }}
    onSaved={() => {
      isSubsonicModalOpen = false;
      loadSubsonicServers();
    }}
  />
{/if}

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

    <div class="flex items-center justify-between gap-4">
      <div class="flex flex-col gap-0.5 min-w-0">
        <span class="text-sm font-medium text-brand-text-primary">{i18n.t('settings.saveArtworkToFoldersLabel')}</span>
        <p class="text-xs text-brand-text-secondary text-pretty">{i18n.t('settings.saveArtworkToFoldersHint')}</p>
      </div>
      <Toggle
        checked={prefs.saveArtworkToFolders}
        onchange={handleToggleSaveArtwork}
        disabled={isSweepingArtwork}
        label={i18n.t('settings.saveArtworkToFoldersLabel')}
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
      <p class="text-base font-bold text-brand-text-primary mt-0.5">{formatNumber(collectionStore.stats.total_songs)}</p>
    </div>
    <div class="bg-brand-main/40 border border-brand-border rounded-lg p-3">
      <span class="text-xs text-brand-text-secondary uppercase font-semibold">{i18n.t('settings.statsAlbums')}</span>
      <p class="text-base font-bold text-brand-text-primary mt-0.5">{formatNumber(collectionStore.stats.total_albums)}</p>
    </div>
    <div class="bg-brand-main/40 border border-brand-border rounded-lg p-3">
      <span class="text-xs text-brand-text-secondary uppercase font-semibold">{i18n.t('settings.statsArtists')}</span>
      <p class="text-base font-bold text-brand-text-primary mt-0.5">{formatNumber(collectionStore.stats.total_artists)}</p>
    </div>
    <HelpTip
      text={diskSizeBreakdown}
      label={`${i18n.t('settings.statsSize')}: ${diskSizeLabel}`}
      class="w-full bg-brand-main/40 border border-brand-border rounded-lg"
    >
      <div class="w-full p-3 text-left">
        <span class="text-xs text-brand-text-secondary uppercase font-semibold">{i18n.t('settings.statsSize')}</span>
        <p class="text-base font-bold text-brand-text-primary mt-0.5">{diskSizeLabel}</p>
      </div>
    </HelpTip>
  </div>
</div>
</div>

{#if showSidecarConfirmModal}
  <ConfirmDialog
    title={i18n.t('settings.saveArtworkToFoldersModalTitle')}
    message={i18n.t('settings.saveArtworkToFoldersModalMessage')}
    confirmLabel={i18n.t('settings.saveArtworkToFoldersModalConfirm')}
    cancelLabel={i18n.t('settings.saveArtworkToFoldersModalCancel')}
    danger={false}
    onConfirm={handleConfirmSaveArtwork}
    onCancel={handleCancelSaveArtwork}
  />
{/if}
