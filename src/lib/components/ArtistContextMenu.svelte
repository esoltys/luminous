<script lang="ts">
  import {
    PlayIcon as Play,
    StackIcon as Layers,
    PushPinIcon as Pin,
    PushPinSlashIcon as PinOff
  } from "phosphor-svelte";
  import { i18n } from "../stores/i18n.svelte";
  import { pinnedStore } from "../stores/pinned.svelte";
  import { queueArtistAsPlaylist, addArtistToQueue } from "../utils/playlist";
  import ContextMenu from "./ContextMenu.svelte";
  import ContextMenuItem from "./ContextMenuItem.svelte";
  import ContextMenuDivider from "./ContextMenuDivider.svelte";

  interface Props {
    x: number;
    y: number;
    artistName: string;
    onPlay?: () => void;
    onAddToQueue?: () => void;
    onClose: () => void;
  }

  let {
    x,
    y,
    artistName,
    onPlay,
    onAddToQueue,
    onClose,
  }: Props = $props();
</script>

<ContextMenu {x} {y} {onClose} estimatedHeight={180}>
  <div class="px-3 py-1 text-[11px] font-bold text-brand-text-primary border-b border-brand-border/40 mb-1 truncate">
    {artistName || i18n.t("collection.unknownArtist")}
  </div>

  <ContextMenuItem
    icon={Play}
    accent
    label={i18n.t("playlists.contextMenuPlayArtist", {}, "Play Artist")}
    onclick={async () => {
      if (onPlay) {
        onPlay();
      } else {
        await queueArtistAsPlaylist(artistName);
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
        await addArtistToQueue(artistName);
      }
      onClose();
    }}
  />

  {#if artistName}
    <ContextMenuDivider />
    <ContextMenuItem
      icon={pinnedStore.isPinned("artist", artistName) ? PinOff : Pin}
      label={pinnedStore.isPinned("artist", artistName)
        ? i18n.t("playlists.contextMenuUnpinHome")
        : i18n.t("playlists.contextMenuPinHome")}
      onclick={() => {
        pinnedStore.toggle("artist", artistName);
        onClose();
      }}
    />
  {/if}
</ContextMenu>
