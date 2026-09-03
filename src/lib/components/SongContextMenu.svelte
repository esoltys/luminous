<script lang="ts">
  import {
    PlayIcon as Play,
    PlusIcon as Plus,
    ListPlusIcon as ListPlus,
    MicrophoneStageIcon as Mic2,
    DiscIcon as DiscAlbum,
    PencilSimpleIcon as Edit3,
    FolderIcon as Folder,
    StackIcon as Layers,
    PushPinIcon as Pin,
    PushPinSlashIcon as PinOff,
    ArrowSquareOutIcon as OpenInPicard
  } from "phosphor-svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { i18n } from "../stores/i18n.svelte";
  import { picardStore } from "../stores/picard.svelte";
  import { playlistsStore } from "../stores/playlists.svelte";
  import { pinnedStore } from "../stores/pinned.svelte";
  import { toastStore } from "../stores/toast.svelte";
  import type { Song } from "../types";
  import ContextMenu from "./ContextMenu.svelte";
  import ContextMenuItem from "./ContextMenuItem.svelte";
  import ContextMenuDivider from "./ContextMenuDivider.svelte";

  let {
    x,
    y,
    song,
    selectedCount = 1,
    selectedSongIds,
    onPlay,
    onAddToQueue,
    onAddToPlaylist,
    onGoToArtist,
    onGoToAlbum,
    onEditTags,
    onOpenInPicard,
    onOrganizeFiles,
    onClose,
  }: {
    x: number;
    y: number;
    song: Song;
    selectedCount?: number;
    selectedSongIds?: number[];
    onPlay: () => void;
    onAddToQueue?: () => void;
    onAddToPlaylist?: () => void;
    onGoToArtist?: () => void;
    onGoToAlbum?: () => void;
    onEditTags?: () => void;
    onOpenInPicard?: () => void;
    onOrganizeFiles?: () => void;
    onClose: () => void;
  } = $props();

  async function handleDefaultAddToQueue() {
    const ids = selectedSongIds && selectedSongIds.length > 0 ? selectedSongIds : [song.id];
    await playlistsStore.addSongsToQueue(ids);
    const name = ids.length > 1 ? `${ids.length} songs` : (song.title || i18n.t("collection.unknownSong"));
    toastStore.show(i18n.t("playlists.addedToQueueSuccess", { name }, `Added ${name} to Queue`));
  }
</script>

<ContextMenu {x} {y} {onClose} estimatedHeight={280}>
  <div class="px-3 py-1 text-[11px] font-bold text-brand-text-primary border-b border-brand-border/40 mb-1 truncate">
    {#if selectedCount > 1}
      {i18n.t("playlists.selectedCount", { count: selectedCount })}
    {:else}
      {song.title || i18n.t("collection.unknownSong")}
    {/if}
  </div>

  <ContextMenuItem
    icon={Play}
    accent
    label={selectedCount > 1
      ? i18n.t("playlists.contextMenuPlay")
      : i18n.t("playlists.contextMenuPlaySong")}
    onclick={() => { onPlay(); onClose(); }}
  />

  <ContextMenuItem
    icon={Layers}
    label={i18n.t("playlists.contextMenuAddQueue", {}, "Add to Queue")}
    onclick={async () => {
      if (onAddToQueue) {
        onAddToQueue();
      } else {
        await handleDefaultAddToQueue();
      }
      onClose();
    }}
  />

  {#if onAddToPlaylist}
    <ContextMenuItem
      icon={Plus}
      label={playlistsStore.activeCustomPlaylist && !playlistsStore.activeCustomPlaylist.is_queue
        ? i18n.t("playlists.contextMenuAddToPlaylist", { name: playlistsStore.activeCustomPlaylist.name })
        : i18n.t("playlists.contextMenuAddToPlaylistDefault")}
      onclick={() => { onAddToPlaylist?.(); onClose(); }}
    />
  {/if}

  {#if selectedCount === 1}
    <ContextMenuDivider />

    {#if onGoToArtist && song.artist}
      <ContextMenuItem
        icon={Mic2}
        label={i18n.t("playlists.contextMenuGoArtist")}
        onclick={() => { onGoToArtist?.(); onClose(); }}
      />
    {/if}

    {#if onGoToAlbum && song.album}
      <ContextMenuItem
        icon={DiscAlbum}
        label={i18n.t("playlists.contextMenuGoAlbum")}
        onclick={() => { onGoToAlbum?.(); onClose(); }}
      />
    {/if}

    <ContextMenuItem
      icon={pinnedStore.isPinned("song", String(song.id)) ? PinOff : Pin}
      label={pinnedStore.isPinned("song", String(song.id))
        ? i18n.t("playlists.contextMenuUnpinHome")
        : i18n.t("playlists.contextMenuPinHome")}
      onclick={() => { pinnedStore.toggle("song", String(song.id)); onClose(); }}
    />
  {/if}

  <ContextMenuDivider />

  {#if onOrganizeFiles}
    <ContextMenuItem
      icon={Folder}
      label={i18n.t("organizer.title")}
      onclick={() => { onOrganizeFiles?.(); onClose(); }}
    />
  {/if}

  {#if selectedCount === 1 && onEditTags}
    <ContextMenuItem
      icon={Edit3}
      label={i18n.t("collection.editTagsTooltip")}
      onclick={() => { onEditTags?.(); onClose(); }}
    />
  {/if}

  {#if onOpenInPicard}
    <ContextMenuItem
      icon={OpenInPicard}
      label={i18n.t("picard.openInPicard")}
      onclick={() => { onOpenInPicard?.(); onClose(); }}
      disabled={!picardStore.available}
      title={picardStore.available ? undefined : i18n.t("picard.notFoundTooltip")}
    />
  {/if}
</ContextMenu>
