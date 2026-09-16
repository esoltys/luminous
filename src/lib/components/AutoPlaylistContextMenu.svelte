<script lang="ts">
  import {
    PlayIcon as Play,
    StackIcon as Layers,
    PushPinIcon as Pin,
    PushPinSlashIcon as PinOff
  } from "phosphor-svelte";
  import { i18n } from "../stores/i18n.svelte";
  import { pinnedStore } from "../stores/pinned.svelte";
  import type { AutoPlaylistItem } from "../types";
  import { autoPlaylistRefKeyFor } from "../types";
  import { queueAutoPlaylistAsPlaylist, addAutoPlaylistToQueue } from "../utils/playlist";
  import ContextMenu from "./ContextMenu.svelte";
  import ContextMenuItem from "./ContextMenuItem.svelte";
  import ContextMenuDivider from "./ContextMenuDivider.svelte";

  interface Props {
    x: number;
    y: number;
    autoPlaylist: AutoPlaylistItem;
    label: string;
    onPlay?: () => void;
    onAddToQueue?: () => void;
    onClose: () => void;
  }

  let {
    x,
    y,
    autoPlaylist,
    label,
    onPlay,
    onAddToQueue,
    onClose,
  }: Props = $props();

  let refKey = $derived(autoPlaylistRefKeyFor(autoPlaylist));
</script>

<ContextMenu {x} {y} {onClose} estimatedHeight={180}>
  <div class="px-3 py-1 text-[11px] font-bold text-brand-text-primary border-b border-brand-border/40 mb-1 truncate">
    {label}
  </div>

  <ContextMenuItem
    icon={Play}
    accent
    label={i18n.t("playlists.contextMenuPlayPlaylist", {}, "Play Playlist")}
    onclick={async () => {
      if (onPlay) {
        onPlay();
      } else {
        await queueAutoPlaylistAsPlaylist(autoPlaylist, label);
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
        await addAutoPlaylistToQueue(autoPlaylist, label);
      }
      onClose();
    }}
  />

  <ContextMenuDivider />

  <ContextMenuItem
    icon={pinnedStore.isPinned("auto_playlist", refKey) ? PinOff : Pin}
    label={pinnedStore.isPinned("auto_playlist", refKey)
      ? i18n.t("playlists.contextMenuUnpinHome")
      : i18n.t("playlists.contextMenuPinHome")}
    onclick={() => {
      pinnedStore.toggle("auto_playlist", refKey);
      onClose();
    }}
  />
</ContextMenu>
