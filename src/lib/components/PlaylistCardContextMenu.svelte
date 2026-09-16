<script lang="ts">
  import {
    PlayIcon as Play,
    StackIcon as Layers,
    PushPinIcon as Pin,
    PushPinSlashIcon as PinOff
  } from "phosphor-svelte";
  import { i18n } from "../stores/i18n.svelte";
  import { pinnedStore } from "../stores/pinned.svelte";
  import type { Playlist } from "../types";
  import { getPlaylistDisplayName, queuePlaylistAsPlaylist, addPlaylistToQueue } from "../utils/playlist";
  import ContextMenu from "./ContextMenu.svelte";
  import ContextMenuItem from "./ContextMenuItem.svelte";
  import ContextMenuDivider from "./ContextMenuDivider.svelte";

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
</script>

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
