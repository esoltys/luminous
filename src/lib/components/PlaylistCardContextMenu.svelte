<script lang="ts">
  import {
    PlayIcon as Play,
    StackIcon as Layers,
    PushPinIcon as Pin,
    PushPinSlashIcon as PinOff,
    ShareNetworkIcon as Share
  } from "phosphor-svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { i18n } from "../stores/i18n.svelte";
  import { pinnedStore } from "../stores/pinned.svelte";
  import type { Playlist, PlaylistItem, Song } from "../types";
  import { getPlaylistDisplayName, queuePlaylistAsPlaylist, addPlaylistToQueue } from "../utils/playlist";
  import ContextMenu from "./ContextMenu.svelte";
  import ContextMenuItem from "./ContextMenuItem.svelte";
  import ContextMenuDivider from "./ContextMenuDivider.svelte";
  import ShareModal from "./ShareModal.svelte";

  interface Props {
    x: number;
    y: number;
    playlist: Playlist;
    onPlay?: () => void;
    onAddToQueue?: () => void;
    onClose: () => void;
  }

  let {
    x,
    y,
    playlist,
    onPlay,
    onAddToQueue,
    onClose,
  }: Props = $props();

  let title = $derived(getPlaylistDisplayName(playlist) || i18n.t("playlists.untitledPlaylistName"));

  let showShareModal = $state(false);
  let shareSongs = $state<Song[]>([]);
  // See AlbumContextMenu.svelte for why the menu must be hidden (not left
  // mounted) the instant Share is clicked: ContextMenu's outside-click
  // listener would otherwise treat any click inside the portalled
  // ShareModal as "outside this menu" and tear the whole thing down.
  let menuVisible = $state(true);

  async function handleShareCard() {
    try {
      const tracks = await invoke<PlaylistItem[]>("get_playlist_tracks", { playlistId: playlist.id });
      shareSongs = tracks.filter((t) => !!t.song).map((t) => t.song!);
    } catch (err) {
      console.error("Failed to load playlist tracks for share card:", err);
      shareSongs = [];
    }
    menuVisible = false;
    showShareModal = true;
  }
</script>

{#if menuVisible}
<ContextMenu {x} {y} {onClose} estimatedHeight={180}>
  <div class="px-3 py-1 text-[11px] font-bold text-brand-text-primary border-b border-brand-border/40 mb-1 truncate">
    {title}
  </div>

  <ContextMenuItem
    icon={Play}
    accent
    label={i18n.t("playlists.contextMenuPlayPlaylist", {}, "Play Playlist")}
    onclick={async () => {
      if (onPlay) {
        onPlay();
      } else {
        await queuePlaylistAsPlaylist(playlist);
      }
      onClose();
    }}
  />

  <ContextMenuItem
    icon={Layers}
    label={i18n.t("playlists.contextMenuAddQueue", {}, "Add to Queue")}
    onclick={async () => {
      if (onAddToQueue) {
        onAddToQueue();
      } else {
        await addPlaylistToQueue(playlist);
      }
      onClose();
    }}
  />

  {#if !playlist.is_queue}
    <ContextMenuDivider />
    <ContextMenuItem
      icon={Share}
      label={i18n.t("shareModal.menuItem")}
      onclick={() => { handleShareCard(); }}
    />
    <ContextMenuItem
      icon={pinnedStore.isPinned("playlist", String(playlist.id)) ? PinOff : Pin}
      label={pinnedStore.isPinned("playlist", String(playlist.id))
        ? i18n.t("playlists.contextMenuUnpinHome")
        : i18n.t("playlists.contextMenuPinHome")}
      onclick={() => {
        pinnedStore.toggle("playlist", String(playlist.id));
        onClose();
      }}
    />
  {/if}
</ContextMenu>
{/if}

{#if showShareModal}
  <ShareModal entity={{ kind: "playlist", title, songs: shareSongs }} onClose={() => { showShareModal = false; onClose(); }} />
{/if}
