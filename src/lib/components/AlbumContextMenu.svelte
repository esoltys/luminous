<script lang="ts">
  import { Play, Plus, User } from "lucide-svelte";
  import { i18n } from "../stores/i18n.svelte";
  import { playlistsStore } from "../stores/playlists.svelte";
  import ContextMenu from "./ContextMenu.svelte";
  import ContextMenuItem from "./ContextMenuItem.svelte";
  import ContextMenuDivider from "./ContextMenuDivider.svelte";

  let {
    x,
    y,
    albumName,
    artistName,
    onPlay,
    onAddToPlaylist,
    onGoToArtist,
    onClose,
  }: {
    x: number;
    y: number;
    albumName: string;
    artistName?: string;
    onPlay: () => void;
    onAddToPlaylist?: () => void;
    onGoToArtist?: () => void;
    onClose: () => void;
  } = $props();
</script>

<ContextMenu {x} {y} {onClose} estimatedHeight={180}>
  <div class="px-3 py-1 text-[11px] font-bold text-brand-text-primary border-b border-brand-border/40 mb-1 truncate">
    {albumName || i18n.t("collection.unknownAlbum")}
  </div>

  <ContextMenuItem
    icon={Play}
    accent
    label={i18n.t("playlists.contextMenuPlayAlbum")}
    onclick={() => { onPlay(); onClose(); }}
  />

  {#if onAddToPlaylist}
    <ContextMenuItem
      icon={Plus}
      label={playlistsStore.activeCustomPlaylist
        ? i18n.t("playlists.contextMenuAddToPlaylist", { name: playlistsStore.activeCustomPlaylist.name })
        : i18n.t("playlists.contextMenuAddToPlaylistDefault")}
      onclick={() => { onAddToPlaylist?.(); onClose(); }}
    />
  {/if}

  {#if onGoToArtist && artistName}
    <ContextMenuDivider />
    <ContextMenuItem
      icon={User}
      label={i18n.t("playlists.contextMenuGoArtist")}
      onclick={() => { onGoToArtist?.(); onClose(); }}
    />
  {/if}
</ContextMenu>
