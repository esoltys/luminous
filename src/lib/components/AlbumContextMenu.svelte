<script lang="ts">
  import { Play, Plus, ListPlus, Mic2, Layers } from "lucide-svelte";
  import { i18n } from "../stores/i18n.svelte";
  import { playlistsStore } from "../stores/playlists.svelte";
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
        const queuePl = await playlistsStore.ensureQueuePlaylist();
        if (queuePl) {
          const songIds = songs.map((s) => s.id);
          await playlistsStore.addSongsToPlaylist(queuePl.id, songIds);
          await invoke("append_songs_to_player_playlist", { songIds });
          toastStore.show(i18n.t("playlists.addedToQueueSuccess", { name: "Queue" }, `Added to Queue`));
        }
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

  {#if onAddToPlaylist && playlistsStore.activeCustomPlaylist && playlistsStore.activeCustomPlaylist.name?.toLowerCase() !== "queue"}
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
</ContextMenu>
