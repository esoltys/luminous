<script lang="ts">
  import { Play, Plus, ListPlus, Mic2, DiscAlbum, Edit3, Folder, Layers, Tag } from "lucide-svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { i18n } from "../stores/i18n.svelte";
  import { playlistsStore } from "../stores/playlists.svelte";
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
    onManageTags,
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
    onManageTags?: () => void;
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

  {#if onAddToPlaylist && playlistsStore.activeCustomPlaylist && !playlistsStore.activeCustomPlaylist.is_queue}
    <ContextMenuItem
      icon={Plus}
      label={i18n.t("playlists.contextMenuAddToPlaylist", { name: playlistsStore.activeCustomPlaylist.name })}
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

  {#if selectedCount === 1 && onManageTags}
    <ContextMenuItem
      icon={Tag}
      label={i18n.t("songTags.contextMenuLabel", {}, "Manage Tags")}
      onclick={() => { onManageTags?.(); onClose(); }}
    />
  {/if}
</ContextMenu>
