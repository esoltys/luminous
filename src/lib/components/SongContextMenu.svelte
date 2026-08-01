<script lang="ts">
  import { Play, Plus, Mic2, DiscAlbum, Edit3, Folder } from "lucide-svelte";
  import { i18n } from "../stores/i18n.svelte";
  import { playlistsStore } from "../stores/playlists.svelte";
  import type { Song } from "../types";
  import ContextMenu from "./ContextMenu.svelte";
  import ContextMenuItem from "./ContextMenuItem.svelte";
  import ContextMenuDivider from "./ContextMenuDivider.svelte";

  let {
    x,
    y,
    song,
    selectedCount = 1,
    onPlay,
    onAddToPlaylist,
    onGoToArtist,
    onGoToAlbum,
    onEditTags,
    onOrganizeFiles,
    onClose,
  }: {
    x: number;
    y: number;
    song: Song;
    selectedCount?: number;
    onPlay: () => void;
    onAddToPlaylist?: () => void;
    onGoToArtist?: () => void;
    onGoToAlbum?: () => void;
    onEditTags?: () => void;
    onOrganizeFiles?: () => void;
    onClose: () => void;
  } = $props();
</script>

<ContextMenu {x} {y} {onClose} estimatedHeight={250}>
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

  {#if onAddToPlaylist}
    <ContextMenuItem
      icon={Plus}
      label={playlistsStore.activeCustomPlaylist
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
</ContextMenu>
