<script lang="ts">
  import {
    PlayIcon as Play,
    PlusIcon as Plus,
    ListPlusIcon as ListPlus,
    MicrophoneStageIcon as Mic2,
    StackIcon as Layers,
    PushPinIcon as Pin,
    PushPinSlashIcon as PinOff
  } from "phosphor-svelte";
  import { i18n } from "../stores/i18n.svelte";
  import { playlistsStore } from "../stores/playlists.svelte";
  import { pinnedStore } from "../stores/pinned.svelte";
  import { toastStore } from "../stores/toast.svelte";
  import { invoke } from "@tauri-apps/api/core";
  import type { Song } from "../types";
  import ContextMenu from "./ContextMenu.svelte";
  import ContextMenuItem from "./ContextMenuItem.svelte";
  import ContextMenuDivider from "./ContextMenuDivider.svelte";

  let {
    x,
    y,
    albumName,
    artistName,
    onPlay,
    onAddToQueue,
    onAddToPlaylist,
    onGoToArtist,
    onClose,
  }: {
    x: number;
    y: number;
    albumName: string;
    artistName?: string;
    onPlay: () => void;
    onAddToQueue?: () => void;
    onAddToPlaylist?: () => void;
    onGoToArtist?: () => void;
    onClose: () => void;
  } = $props();

  async function handleDefaultAddToQueue() {
    try {
      const songs = await invoke<Song[]>("get_songs_by_album", { album: albumName || "" });
      if (songs.length > 0) {
        const songIds = songs.map((s) => s.id);
        await playlistsStore.addSongsToQueue(songIds);
        const name = albumName || i18n.t("collection.unknownAlbum");
        toastStore.show(i18n.t("playlists.addedToQueueSuccess", { name }, `Added ${name} to Queue`));
      }
    } catch (err) {
      console.error("Failed to add album to Queue:", err);
    }
  }
</script>

<ContextMenu {x} {y} {onClose} estimatedHeight={220}>
  <div class="px-3 py-1 text-[11px] font-bold text-brand-text-primary border-b border-brand-border/40 mb-1 truncate">
    {albumName || i18n.t("collection.unknownAlbum")}
  </div>

  <ContextMenuItem
    icon={Play}
    accent
    label={i18n.t("playlists.contextMenuPlayAlbum")}
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

  {#if onGoToArtist && artistName}
    <ContextMenuDivider />
    <ContextMenuItem
      icon={Mic2}
      label={i18n.t("playlists.contextMenuGoArtist")}
      onclick={() => { onGoToArtist?.(); onClose(); }}
    />
  {/if}

  {#if albumName}
    {#if !(onGoToArtist && artistName)}
      <ContextMenuDivider />
    {/if}
    <ContextMenuItem
      icon={pinnedStore.isPinned("album", albumName) ? PinOff : Pin}
      label={pinnedStore.isPinned("album", albumName)
        ? i18n.t("playlists.contextMenuUnpinHome")
        : i18n.t("playlists.contextMenuPinHome")}
      onclick={() => { pinnedStore.toggle("album", albumName); onClose(); }}
    />
  {/if}
</ContextMenu>
