<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import {
    PlayIcon,
    StackIcon,
    PlusIcon,
    ArrowsClockwiseIcon,
    DeviceMobileSpeakerIcon,
    FolderOpenIcon,
    CopyIcon,
    ArrowSquareOutIcon,
    PencilSimpleIcon,
    GearIcon,
  } from "phosphor-svelte";
  import { navigationStore } from "../../stores/navigation.svelte";
  import { filesStore } from "../../stores/files.svelte";
  import { playerStore } from "../../stores/player.svelte";
  import { toastStore } from "../../stores/toast.svelte";
  import { picardStore } from "../../stores/picard.svelte";
  import { i18n } from "../../stores/i18n.svelte";
  import { deviceName } from "../../utils/devices";
  import { openInPicard, openPathsInPicard } from "../../utils/picard";
  import type { FileEntry, FileSource, Song } from "../../types";
  import ContextMenu from "../ContextMenu.svelte";
  import ContextMenuItem from "../ContextMenuItem.svelte";
  import ContextMenuDivider from "../ContextMenuDivider.svelte";
  import SongContextMenu from "../SongContextMenu.svelte";
  import { shownName } from "./shownName";
  import { addFolderToDeviceSync, copyPaths, openFile, playFolder, playlistFromFolder, queueFolder, reveal, revealLabel, scanFolder } from "./folderActions";

  /**
   * The context and overflow menu for rows and headers of the Files view
   * (#1884, #1928): a folder gets folder actions (or from the header, the open
   * folder's actions plus Folder settings, omitting the already-visible primary
   * Play folder button); library songs get the song menu, acting on the library
   * songs and saying how many other files it left out; other files get what can
   * be done without the library, led by Open for a sidecar (#1885), which is what
   * Enter does on its row. File actions (Show in File Explorer, Copy path) close
   * every menu, where the files have a local path. A folder in a folder column is
   * there to move around with, so its menu has the file actions only. A menu with
   * nothing to offer doesn't open.
   */
  let {
    x,
    y,
    source,
    path,
    folderName,
    entries = [],
    scannable,
    inColumn = false,
    fromHeader = false,
    folderLocation = null,
    onFolderSettings,
    onEditTags,
    onEditFileTags,
    onEditFolderTags,
    onEditBatchTags,
    onClose,
  }: {
    x: number;
    y: number;
    source: FileSource;
    /** The folder the entries are in (or the folder itself when fromHeader is true). */
    path: string[];
    /** What that folder is called, for toasts and the queue's context. */
    folderName: string;
    /** What the menu acts on: one folder, or one or more files. */
    entries?: FileEntry[];
    /** Whether the folder can be scanned for new files (a local, included folder). */
    scannable: boolean;
    /** The entry is a folder in a folder column, not a row of the open folder. */
    inColumn?: boolean;
    /** Opened from the Files view header overflow button for the open folder itself (#1928). */
    fromHeader?: boolean;
    /** Local filesystem path of the open folder, used when fromHeader is true. */
    folderLocation?: string | null;
    /** Opens Settings > Sources when picked from the header overflow menu. */
    onFolderSettings?: () => void;
    onEditTags: (songId: number) => void;
    onEditFileTags?: (entry: FileEntry) => void;
    onEditFolderTags?: (dir: FileEntry, dirPath: string[]) => void;
    onEditBatchTags?: (entries: FileEntry[]) => void;
    onClose: () => void;
  } = $props();

  const isLocal = $derived(source.kind === "folder");
  const childDir = $derived(!inColumn && entries.length === 1 && entries[0].isDir ? entries[0] : null);
  const isFolder = $derived(fromHeader || childDir !== null);
  const songIds = $derived(fromHeader ? [] : entries.flatMap((e) => (e.songId !== null ? [e.songId] : [])));
  const unscanned = $derived(fromHeader ? [] : entries.filter((e) => e.songId === null && e.status === "notScanned"));
  const targetLocation = $derived(fromHeader ? folderLocation : childDir?.location ?? null);
  const locations = $derived(
    fromHeader
      ? (targetLocation ? [targetLocation] : [])
      : entries.flatMap((e) => (e.location !== null ? [e.location] : []))
  );
  const unscannedLocations = $derived(unscanned.flatMap((e) => (e.location !== null ? [e.location] : [])));
  const single = $derived(fromHeader ? null : entries.length === 1 ? entries[0] : null);
  const revealLocation = $derived(fromHeader ? targetLocation : single?.location ?? null);

  /** What the song menu adds to its toasts when files were left out. */
  const skippedNotice = $derived(
    unscanned.length === 0
      ? undefined
      : i18n.plural(isLocal ? "files.skippedScan" : "files.skipped", unscanned.length)
  );

  let songs = $state<Song[] | null>(null);
  $effect(() => {
    const ids = songIds;
    if (isFolder || ids.length === 0) return;
    let stale = false;
    invoke<Song[]>("get_songs_by_ids", { songIds: ids })
      .then((loaded) => {
        if (!stale) songs = loaded;
      })
      .catch((err) => {
        console.error("Failed to load the selected songs:", err);
        if (!stale) onClose();
      });
    return () => (stale = true);
  });

  const dirName = $derived(fromHeader ? folderName : childDir ? shownName(childDir.name, childDir.missingTag) : "");
  const dirPath = $derived(fromHeader ? path : childDir ? [...path, childDir.name] : path);
  const canOpenSingle = $derived(single?.opensInApp ?? false);
  const canPlayOutside = $derived(songIds.length === 0 && unscannedLocations.length > 0);
  const canScanFiles = $derived(songIds.length === 0 && scannable && unscanned.length > 0);
  const canScanDir = $derived(isLocal && (fromHeader ? scannable : childDir !== null && childDir.status !== "excluded"));
  const syncDevices = $derived(isLocal && targetLocation ? filesStore.devices : []);
  const canEditUnscanned = $derived(isLocal && !inColumn && !single?.isDir && (single?.kind === "audio" || unscanned.length > 0));
  const isEmpty = $derived(!isFolder && songIds.length === 0 && !canPlayOutside && !canScanFiles && !canEditUnscanned && locations.length === 0);

  $effect(() => {
    if (isEmpty) onClose();
  });

  /**
   * Starts `action` and then closes the menu. `action` runs synchronously
   * before `onClose()` unmounts this component, ensuring any captured `$derived`
   * state (such as `dirPath` or `dirName`) evaluates while the reactive scope is
   * still alive.
   */
  function runAndClose(action: () => unknown) {
    try {
      const result = action();
      onClose();
      if (result instanceof Promise) {
        result.catch((err) => console.error("A Files menu action failed:", err));
      }
    } catch (err) {
      onClose();
      console.error("A Files menu action failed:", err);
    }
  }

  async function playSongs() {
    if (single?.songId != null) return playFolder(source, path, folderName, single.songId);
    await playerStore.playSongs(songIds, 0, undefined, undefined, folderName);
    if (skippedNotice) toastStore.show(skippedNotice, "info");
  }
</script>

{#snippet header(text: string)}
  <div class="px-3 py-1 text-[11px] font-bold text-brand-text-primary border-b border-brand-border/40 mb-1 truncate">{text}</div>
{/snippet}

{#snippet fileItems()}
  {#if revealLocation}
    <ContextMenuItem icon={FolderOpenIcon} label={revealLabel()} onclick={() => runAndClose(() => reveal(revealLocation))} />
  {/if}
  {#if locations.length > 0}
    <ContextMenuItem
      icon={CopyIcon}
      label={i18n.t(locations.length > 1 ? "files.copyPaths" : "files.copyPath")}
      onclick={() => runAndClose(() => copyPaths(locations))}
    />
  {/if}
{/snippet}

{#if isFolder}
  <ContextMenu {x} {y} {onClose}>
    {#if !fromHeader}
      {@render header(dirName)}
      <ContextMenuItem icon={PlayIcon} accent label={i18n.t("files.playFolder")} onclick={() => runAndClose(() => playFolder(source, dirPath, dirName))} />
    {/if}
    <ContextMenuItem icon={StackIcon} label={i18n.t("playlists.contextMenuAddQueue")} onclick={() => runAndClose(() => queueFolder(source, dirPath, dirName))} />
    <ContextMenuItem icon={PlusIcon} label={i18n.t("files.newPlaylistFromFolder")} onclick={() => runAndClose(() => playlistFromFolder(source, dirPath, dirName))} />
    {#if canScanDir || syncDevices.length > 0}
      <ContextMenuDivider />
    {/if}
    {#if canScanDir}
      <ContextMenuItem icon={ArrowsClockwiseIcon} label={i18n.t("files.scanFolder")} onclick={() => runAndClose(() => scanFolder(source, dirPath))} />
    {/if}
    {#each syncDevices as device (device.id)}
      <ContextMenuItem
        icon={DeviceMobileSpeakerIcon}
        label={syncDevices.length === 1 ? i18n.t("files.addToDeviceSync") : i18n.t("files.addToNamedDeviceSync", { device: deviceName(device) })}
        onclick={() => runAndClose(() => addFolderToDeviceSync(device, targetLocation!, dirName))}
      />
    {/each}
    {#if isLocal && targetLocation}
      <ContextMenuDivider />
      <ContextMenuItem
        icon={PencilSimpleIcon}
        label={i18n.t("files.editTagsForFolder")}
        onclick={() => runAndClose(() => {
          const folderEntry: FileEntry = childDir ?? {
            name: path.length > 0 ? path[path.length - 1] : folderName,
            isDir: true,
            size: null,
            modified: null,
            fileType: null,
            kind: null,
            opensInApp: false,
            status: null,
            songId: null,
            missingTag: null,
            location: targetLocation,
          };
          onEditFolderTags?.(folderEntry, dirPath);
        })}
      />
      <ContextMenuItem
        icon={ArrowSquareOutIcon}
        label={i18n.t("picard.openInPicard")}
        disabled={!picardStore.available}
        title={!picardStore.available ? i18n.t("picard.notFoundTooltip") : undefined}
        onclick={() => runAndClose(() => openPathsInPicard([targetLocation]))}
      />
    {/if}
    {#if locations.length > 0}
      <ContextMenuDivider />
      {@render fileItems()}
    {/if}
    {#if fromHeader && onFolderSettings}
      <ContextMenuDivider />
      <ContextMenuItem
        icon={GearIcon}
        label={i18n.t("files.folderSettings")}
        onclick={() => runAndClose(onFolderSettings)}
      />
    {/if}
  </ContextMenu>
{:else if songIds.length > 0}
  {#if songs && songs.length > 0}
    {@const song = songs[0]}
    <SongContextMenu
      {x}
      {y}
      {song}
      selectedCount={songs.length}
      selectedSongIds={songs.map((s) => s.id)}
      selectedSongs={songs}
      notice={skippedNotice}
      showViewInFiles={false}
      onPlay={playSongs}
      onGoToArtist={() => navigationStore.viewArtist(song.album_artist?.trim() || song.artist || "")}
      onGoToAlbum={() => navigationStore.viewAlbum(song.album || "")}
      onEditTags={() => onEditTags(song.id)}
      onEditBatchTags={() => onEditBatchTags?.(entries)}
      onOpenInPicard={() => {
        if (isLocal && locations.length > 0) openPathsInPicard(locations);
        else openInPicard(songs!.map((s) => s.id));
      }}
      trailing={locations.length > 0 ? fileItems : undefined}
      {onClose}
    />
  {/if}
{:else if !isEmpty}
  <ContextMenu {x} {y} {onClose}>
    {@render header(single ? shownName(single.name, single.missingTag) : i18n.t("playlists.selectedCount", { count: entries.length }))}
    {#if canOpenSingle}
      <ContextMenuItem icon={ArrowSquareOutIcon} isDefault label={i18n.t("files.open")} onclick={() => runAndClose(() => openFile(source, path, single!))} />
      <ContextMenuDivider />
    {/if}
    {#if canPlayOutside}
      <ContextMenuItem
        icon={PlayIcon}
        accent
        label={i18n.t("files.playWithoutAdding")}
        onclick={() => runAndClose(() => playerStore.openAndPlay(unscannedLocations))}
      />
    {/if}
    {#if canScanFiles}
      <ContextMenuItem icon={ArrowsClockwiseIcon} label={i18n.t("files.scanFolder")} onclick={() => runAndClose(() => scanFolder(source, path))} />
    {/if}
    {#if isLocal && single?.isDir && single.location}
      <ContextMenuItem
        icon={PencilSimpleIcon}
        label={i18n.t("files.editTagsForFolder")}
        onclick={() => runAndClose(() => onEditFolderTags?.(single!, [...path, single!.name]))}
      />
      <ContextMenuItem
        icon={ArrowSquareOutIcon}
        label={i18n.t("picard.openInPicard")}
        disabled={!picardStore.available}
        title={!picardStore.available ? i18n.t("picard.notFoundTooltip") : undefined}
        onclick={() => runAndClose(() => openPathsInPicard([single!.location!]))}
      />
    {/if}
    {#if isLocal && !inColumn && !single?.isDir && (single?.kind === "audio" || unscanned.length > 0)}
      <ContextMenuDivider />
      <ContextMenuItem
        icon={PencilSimpleIcon}
        label={i18n.t("files.editTags")}
        onclick={() => runAndClose(() => {
          if (single) onEditFileTags?.(single);
          else onEditBatchTags?.(entries);
        })}
      />
      <ContextMenuItem
        icon={ArrowSquareOutIcon}
        label={i18n.t("picard.openInPicard")}
        disabled={!picardStore.available || locations.length === 0}
        title={!picardStore.available ? i18n.t("picard.notFoundTooltip") : undefined}
        onclick={() => runAndClose(() => openPathsInPicard(locations))}
      />
    {/if}
    {#if (canPlayOutside || canScanFiles || (isLocal && (single?.isDir || (!inColumn && (single?.kind === "audio" || unscanned.length > 0))))) && locations.length > 0}
      <ContextMenuDivider />
    {/if}
    {@render fileItems()}
  </ContextMenu>
{/if}
