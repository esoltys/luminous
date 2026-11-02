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
  } from "phosphor-svelte";
  import { navigationStore } from "../../stores/navigation.svelte";
  import { filesStore } from "../../stores/files.svelte";
  import { playerStore } from "../../stores/player.svelte";
  import { toastStore } from "../../stores/toast.svelte";
  import { i18n } from "../../stores/i18n.svelte";
  import { deviceName } from "../../utils/devices";
  import { openInPicard } from "../../utils/picard";
  import type { FileEntry, FileSource, Song } from "../../types";
  import ContextMenu from "../ContextMenu.svelte";
  import ContextMenuItem from "../ContextMenuItem.svelte";
  import ContextMenuDivider from "../ContextMenuDivider.svelte";
  import SongContextMenu from "../SongContextMenu.svelte";
  import { shownName } from "./shownName";
  import { addFolderToDeviceSync, copyPaths, playFolder, playlistFromFolder, queueFolder, reveal, revealLabel, scanFolder } from "./folderActions";

  /**
   * The right-click menu for rows of the Files view (#1884), one menu per
   * kind of row: a folder gets folder actions; library songs get the song
   * menu, acting on the library songs and saying how many other files it
   * left out; other files get what can be done without the library. File
   * actions (Show in File Explorer, Copy path) close every menu, where the
   * files have a local path. A menu with nothing to offer doesn't open.
   */
  let {
    x,
    y,
    source,
    path,
    folderName,
    entries,
    scannable,
    onEditTags,
    onClose,
  }: {
    x: number;
    y: number;
    source: FileSource;
    /** The folder the entries are in. */
    path: string[];
    /** What that folder is called, for toasts and the queue's context. */
    folderName: string;
    /** What the menu acts on: one folder, or one or more files. */
    entries: FileEntry[];
    /** Whether the folder can be scanned for new files (a local, included folder). */
    scannable: boolean;
    onEditTags: (songId: number) => void;
    onClose: () => void;
  } = $props();

  const isLocal = $derived(source.kind === "folder");
  const dir = $derived(entries.length === 1 && entries[0].isDir ? entries[0] : null);
  const songIds = $derived(entries.flatMap((e) => (e.songId !== null ? [e.songId] : [])));
  const unscanned = $derived(entries.filter((e) => e.songId === null && e.status === "notScanned"));
  const locations = $derived(entries.flatMap((e) => (e.location !== null ? [e.location] : [])));
  const single = $derived(entries.length === 1 ? entries[0] : null);

  /** What the song menu adds to its toasts when files were left out. */
  const skippedNotice = $derived(
    unscanned.length === 0
      ? undefined
      : i18n.plural(isLocal ? "files.skippedScan" : "files.skipped", unscanned.length)
  );

  let songs = $state<Song[] | null>(null);
  $effect(() => {
    const ids = songIds;
    if (dir || ids.length === 0) return;
    invoke<Song[]>("get_songs_by_ids", { songIds: ids })
      .then((loaded) => (songs = loaded))
      .catch((err) => {
        console.error("Failed to load the selected songs:", err);
        onClose();
      });
  });

  const dirName = $derived(dir ? shownName(dir.name, dir.missingTag) : "");
  const dirPath = $derived(dir ? [...path, dir.name] : path);
  const canPlayOutside = $derived(songIds.length === 0 && unscanned.some((e) => e.location !== null));
  const canScanFiles = $derived(songIds.length === 0 && scannable && unscanned.length > 0);
  const canScanDir = $derived(isLocal && dir !== null && dir.status !== "excluded");
  const syncDevices = $derived(isLocal && dir?.location ? filesStore.devices : []);
  const isEmpty = $derived(!dir && songIds.length === 0 && !canPlayOutside && !canScanFiles && locations.length === 0);

  $effect(() => {
    if (isEmpty) onClose();
  });

  function act(action: () => unknown) {
    action();
    onClose();
  }

  async function playSongs() {
    if (single?.songId != null) return playFolder(source, path, folderName, single.songId);
    await playerStore.playSongs(songIds, 0, undefined, undefined, folderName);
    if (skippedNotice) toastStore.show(skippedNotice, "info");
  }
</script>

{#snippet fileItems()}
  {#if single?.location}
    <ContextMenuItem icon={FolderOpenIcon} label={revealLabel()} onclick={() => act(() => reveal(single.location!))} />
  {/if}
  {#if locations.length > 0}
    <ContextMenuItem
      icon={CopyIcon}
      label={i18n.t(locations.length > 1 ? "files.copyPaths" : "files.copyPath")}
      onclick={() => act(() => copyPaths(locations))}
    />
  {/if}
{/snippet}

{#if dir}
  <ContextMenu {x} {y} {onClose}>
    <div class="px-3 py-1 text-[11px] font-bold text-brand-text-primary border-b border-brand-border/40 mb-1 truncate">{dirName}</div>
    <ContextMenuItem icon={PlayIcon} accent label={i18n.t("files.playFolder")} onclick={() => act(() => playFolder(source, dirPath, dirName))} />
    <ContextMenuItem icon={StackIcon} label={i18n.t("playlists.contextMenuAddQueue")} onclick={() => act(() => queueFolder(source, dirPath, dirName))} />
    <ContextMenuItem icon={PlusIcon} label={i18n.t("files.newPlaylistFromFolder")} onclick={() => act(() => playlistFromFolder(source, dirPath, dirName))} />
    {#if canScanDir || syncDevices.length > 0}
      <ContextMenuDivider />
    {/if}
    {#if canScanDir}
      <ContextMenuItem icon={ArrowsClockwiseIcon} label={i18n.t("files.scanFolder")} onclick={() => act(() => scanFolder(source, dirPath))} />
    {/if}
    {#each syncDevices as device (device.id)}
      <ContextMenuItem
        icon={DeviceMobileSpeakerIcon}
        label={syncDevices.length === 1 ? i18n.t("files.addToDeviceSync") : i18n.t("files.addToNamedDeviceSync", { device: deviceName(device) })}
        onclick={() => act(() => addFolderToDeviceSync(device, dir.location!, dirName))}
      />
    {/each}
    {#if locations.length > 0}
      <ContextMenuDivider />
      {@render fileItems()}
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
      onOpenInPicard={() => openInPicard(songs!.map((s) => s.id))}
      trailing={locations.length > 0 ? fileItems : undefined}
      {onClose}
    />
  {/if}
{:else if !isEmpty}
  <ContextMenu {x} {y} {onClose}>
    <div class="px-3 py-1 text-[11px] font-bold text-brand-text-primary border-b border-brand-border/40 mb-1 truncate">
      {single ? single.name : i18n.t("playlists.selectedCount", { count: entries.length })}
    </div>
    {#if canPlayOutside}
      <ContextMenuItem
        icon={PlayIcon}
        accent
        label={i18n.t("files.playWithoutAdding")}
        onclick={() => act(() => playerStore.openAndPlay(unscanned.flatMap((e) => (e.location !== null ? [e.location] : []))))}
      />
    {/if}
    {#if canScanFiles}
      <ContextMenuItem icon={ArrowsClockwiseIcon} label={i18n.t("files.scanFolder")} onclick={() => act(() => scanFolder(source, path))} />
    {/if}
    {#if (canPlayOutside || canScanFiles) && locations.length > 0}
      <ContextMenuDivider />
    {/if}
    {@render fileItems()}
  </ContextMenu>
{/if}
