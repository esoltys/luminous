<script lang="ts">
  import { i18n } from "../stores/i18n.svelte";
  import { pinnedStore } from "../stores/pinned.svelte";
  import { playerStore } from "../stores/player.svelte";
  import { navigationStore } from "../stores/navigation.svelte";
  import type { PinnedItem } from "../types";
  import type { NavigablePin } from "../utils/pinnedNav";
  import PinnedNavItem from "./PinnedNavItem.svelte";
  import { SIDEBAR_SECTION_LABEL_CLASS } from "../utils/sidebarNav";
  import AlbumContextMenu from "./AlbumContextMenu.svelte";
  import SongContextMenu from "./SongContextMenu.svelte";
  import ArtistContextMenu from "./ArtistContextMenu.svelte";
  import PlaylistCardContextMenu from "./PlaylistCardContextMenu.svelte";
  import AutoPlaylistContextMenu from "./AutoPlaylistContextMenu.svelte";
  import {
    queueAlbumAsPlaylist,
    queueArtistAsPlaylist,
    queuePlaylistAsPlaylist,
    queueAutoPlaylistAsPlaylist,
  } from "../utils/playlist";
  import { autoPlaylistLabel } from "../utils/pinnedNav";
  import { startPointerDrag } from "../utils/pointerDrag";

  interface Props {
    collapsed?: boolean;
  }

  let { collapsed = false }: Props = $props();

  let draggedIndex = $state<number | null>(null);
  let dragOverIndex = $state<number | null>(null);

  let contextMenuState = $state<{ x: number; y: number; item: PinnedItem } | null>(null);

  function resetDrag() {
    draggedIndex = null;
    dragOverIndex = null;
  }

  function handlePointerDown(e: PointerEvent, index: number) {
    const handle = startPointerDrag(e, {
      onMove: (pos) => {
        const hit = document.elementFromPoint(pos.x, pos.y);
        if (!hit?.closest("[data-pinned-nav-list]")) {
          dragOverIndex = null;
          return;
        }
        const idx = (hit.closest("[data-pinned-nav-index]") as HTMLElement | null)?.dataset
          .pinnedNavIndex;
        if (idx !== undefined) dragOverIndex = Number(idx);
      },
      onDrop: async () => {
        if (dragOverIndex !== null && dragOverIndex !== index) {
          await pinnedStore.reorderVisible(index, dragOverIndex);
        }
        resetDrag();
      },
      onTap: resetDrag,
      onCancel: resetDrag,
    });
    if (handle) draggedIndex = index;
  }

  function handleContextMenu(e: MouseEvent, item: PinnedItem) {
    e.preventDefault();
    e.stopPropagation();
    contextMenuState = { x: e.clientX, y: e.clientY, item };
  }

  function handleKeyDown(e: KeyboardEvent, index: number, pin: NavigablePin) {
    if (e.altKey && e.key === "ArrowUp" && index > 0) {
      e.preventDefault();
      pinnedStore.reorderVisible(index, index - 1);
    } else if (e.altKey && e.key === "ArrowDown" && index < pinnedStore.navigableItems.length - 1) {
      e.preventDefault();
      pinnedStore.reorderVisible(index, index + 1);
    } else if (e.key === "Enter" || e.key === " ") {
      e.preventDefault();
      pin.open();
    }
  }
</script>

{#if pinnedStore.navigableItems.length > 0}
  <div
    data-pinned-nav-list="true"
    class="w-full flex flex-col {collapsed ? 'items-center py-1' : 'py-0.5'}"
  >
    {#if !collapsed}
      <div class={SIDEBAR_SECTION_LABEL_CLASS}>
        {i18n.t("sidebar.pinned")}
      </div>
    {/if}
    <div class="w-full space-y-0.5 flex flex-col {collapsed ? 'items-center' : ''}">
      {#each pinnedStore.navigableItems as pin, index (pin.id)}
        <PinnedNavItem
          {pin}
          {collapsed}
          {index}
          isDragged={draggedIndex === index}
          isDragOver={dragOverIndex === index && draggedIndex !== null && draggedIndex !== index}
          dropIndicatorPosition={draggedIndex !== null && draggedIndex < index ? "bottom" : "top"}
          onclick={() => pin.open()}
          oncontextmenu={(e) => handleContextMenu(e, pin.rawItem)}
          onpointerdown={(e) => handlePointerDown(e, index)}
          onkeydown={(e) => handleKeyDown(e, index, pin)}
        />
      {/each}
    </div>
  </div>
{/if}

<!-- Context Menus -->
{#if contextMenuState}
  {@const item = contextMenuState.item}
  {#if item.type === "album"}
    <AlbumContextMenu
      x={contextMenuState.x}
      y={contextMenuState.y}
      albumName={item.album.album || ""}
      artistName={item.album.artist || undefined}
      rating={item.album.album ? item.album.rating : undefined}
      onPlay={() => queueAlbumAsPlaylist(item.album)}
      onGoToArtist={item.album.artist
        ? () => navigationStore.viewArtist(item.album.artist!)
        : undefined}
      onClose={() => {
        contextMenuState = null;
      }}
    />
  {:else if item.type === "song"}
    <SongContextMenu
      x={contextMenuState.x}
      y={contextMenuState.y}
      song={item.song}
      onPlay={() => playerStore.playSong(item.song.id)}
      onGoToArtist={item.song.artist
        ? () => navigationStore.viewArtist(item.song.album_artist?.trim() || item.song.artist || "")
        : undefined}
      onGoToAlbum={item.song.album
        ? () => navigationStore.viewAlbum(item.song.album || "")
        : undefined}
      onClose={() => {
        contextMenuState = null;
      }}
    />
  {:else if item.type === "artist"}
    <ArtistContextMenu
      x={contextMenuState.x}
      y={contextMenuState.y}
      artistName={item.artist.name || ""}
      onPlay={() => queueArtistAsPlaylist(item.artist.name || "")}
      onClose={() => {
        contextMenuState = null;
      }}
    />
  {:else if item.type === "playlist"}
    <PlaylistCardContextMenu
      x={contextMenuState.x}
      y={contextMenuState.y}
      playlist={item.playlist}
      onPlay={() => queuePlaylistAsPlaylist(item.playlist)}
      onClose={() => {
        contextMenuState = null;
      }}
    />
  {:else if item.type === "auto_playlist"}
    <AutoPlaylistContextMenu
      x={contextMenuState.x}
      y={contextMenuState.y}
      autoPlaylist={item.autoPlaylist}
      label={autoPlaylistLabel(item.autoPlaylist)}
      onPlay={() =>
        queueAutoPlaylistAsPlaylist(item.autoPlaylist, autoPlaylistLabel(item.autoPlaylist))}
      onClose={() => {
        contextMenuState = null;
      }}
    />
  {/if}
{/if}
