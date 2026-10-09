<script lang="ts">
  import { pinnedStore } from "../stores/pinned.svelte";
  import { playerStore } from "../stores/player.svelte";
  import { navigationStore } from "../stores/navigation.svelte";
  import type { PinnedItem } from "../types";
  import type { NavigablePin } from "../utils/pinnedNav";
  import PinnedNavItem from "./PinnedNavItem.svelte";
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

  interface Props {
    collapsed?: boolean;
  }

  let { collapsed = false }: Props = $props();

  const POINTER_DRAG_THRESHOLD_PX = 4;

  let draggedIndex = $state<number | null>(null);
  let dragOverIndex = $state<number | null>(null);
  let pointerDragArmed = false;
  let pointerDragStartX = 0;
  let pointerDragStartY = 0;
  let wasDragged = false;

  let contextMenuState = $state<{ x: number; y: number; item: PinnedItem } | null>(null);

  function handlePointerDown(e: PointerEvent, index: number) {
    if (e.button !== 0) return;
    const target = e.target as HTMLElement;
    if (target.closest("button, a, input, select, textarea, [data-interactive]")) return;

    pointerDragArmed = false;
    wasDragged = false;
    pointerDragStartX = e.clientX;
    pointerDragStartY = e.clientY;
    draggedIndex = index;

    // Immediately capture pointer so WebView2 doesn't hijack into a native OS drag gesture
    (e.currentTarget as HTMLElement).setPointerCapture?.(e.pointerId);

    window.addEventListener("pointermove", handlePointerMove);
    window.addEventListener("pointerup", handlePointerUp);
    window.addEventListener("pointercancel", handlePointerCancel);
  }

  function handlePointerMove(e: PointerEvent) {
    if (draggedIndex === null) return;

    if (!pointerDragArmed) {
      const dx = e.clientX - pointerDragStartX;
      const dy = e.clientY - pointerDragStartY;
      if (Math.hypot(dx, dy) < POINTER_DRAG_THRESHOLD_PX) return;
      pointerDragArmed = true;
    }

    const listEl = document.elementFromPoint(e.clientX, e.clientY)?.closest("[data-pinned-nav-list]");
    if (!listEl) {
      dragOverIndex = null;
      return;
    }

    const rowEl = document
      .elementFromPoint(e.clientX, e.clientY)
      ?.closest("[data-pinned-nav-index]") as HTMLElement | null;
    const idx = rowEl?.dataset.pinnedNavIndex;
    if (idx !== undefined) {
      dragOverIndex = Number(idx);
    }
  }

  async function handlePointerUp() {
    window.removeEventListener("pointermove", handlePointerMove);
    window.removeEventListener("pointerup", handlePointerUp);
    window.removeEventListener("pointercancel", handlePointerCancel);

    if (pointerDragArmed) {
      wasDragged = true;
      if (dragOverIndex !== null && draggedIndex !== null && dragOverIndex !== draggedIndex) {
        await pinnedStore.reorderVisible(draggedIndex, dragOverIndex);
      }
    }

    draggedIndex = null;
    dragOverIndex = null;
    pointerDragArmed = false;
  }

  function handlePointerCancel() {
    window.removeEventListener("pointermove", handlePointerMove);
    window.removeEventListener("pointerup", handlePointerUp);
    window.removeEventListener("pointercancel", handlePointerCancel);
    draggedIndex = null;
    dragOverIndex = null;
    pointerDragArmed = false;
  }

  function handleRowClick(pin: NavigablePin) {
    if (wasDragged) {
      wasDragged = false;
      return;
    }
    pin.open();
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
    } else if (
      e.altKey &&
      e.key === "ArrowDown" &&
      index < pinnedStore.navigableItems.length - 1
    ) {
      e.preventDefault();
      pinnedStore.reorderVisible(index, index + 1);
    } else if (e.key === "Enter" || e.key === " ") {
      e.preventDefault();
      pin.open();
    }
  }
</script>

{#if pinnedStore.navigableItems.length > 0}
  <div data-pinned-nav-list="true" class="w-full flex flex-col {collapsed ? 'items-center py-1' : 'py-0.5'}">
    <div class="w-full space-y-0.5 flex flex-col {collapsed ? 'items-center' : ''}">
      {#each pinnedStore.navigableItems as pin, index (pin.id)}
        <PinnedNavItem
          {pin}
          {collapsed}
          {index}
          isDragged={draggedIndex === index}
          isDragOver={dragOverIndex === index && draggedIndex !== null && draggedIndex !== index}
          dropIndicatorPosition={draggedIndex !== null && draggedIndex < index ? "bottom" : "top"}
          onclick={() => handleRowClick(pin)}
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
      onPlay={() => queueAlbumAsPlaylist(item.album)}
      onGoToArtist={item.album.artist ? () => navigationStore.viewArtist(item.album.artist!) : undefined}
      onClose={() => { contextMenuState = null; }}
    />
  {:else if item.type === "song"}
    <SongContextMenu
      x={contextMenuState.x}
      y={contextMenuState.y}
      song={item.song}
      onPlay={() => playerStore.playSong(item.song.id)}
      onGoToArtist={item.song.artist ? () => navigationStore.viewArtist(item.song.album_artist?.trim() || item.song.artist || "") : undefined}
      onGoToAlbum={item.song.album ? () => navigationStore.viewAlbum(item.song.album || "") : undefined}
      onClose={() => { contextMenuState = null; }}
    />
  {:else if item.type === "artist"}
    <ArtistContextMenu
      x={contextMenuState.x}
      y={contextMenuState.y}
      artistName={item.artist.name || ""}
      onPlay={() => queueArtistAsPlaylist(item.artist.name || "")}
      onClose={() => { contextMenuState = null; }}
    />
  {:else if item.type === "playlist"}
    <PlaylistCardContextMenu
      x={contextMenuState.x}
      y={contextMenuState.y}
      playlist={item.playlist}
      onPlay={() => queuePlaylistAsPlaylist(item.playlist)}
      onClose={() => { contextMenuState = null; }}
    />
  {:else if item.type === "auto_playlist"}
    <AutoPlaylistContextMenu
      x={contextMenuState.x}
      y={contextMenuState.y}
      autoPlaylist={item.autoPlaylist}
      label={autoPlaylistLabel(item.autoPlaylist)}
      onPlay={() => queueAutoPlaylistAsPlaylist(item.autoPlaylist, autoPlaylistLabel(item.autoPlaylist))}
      onClose={() => { contextMenuState = null; }}
    />
  {/if}
{/if}
