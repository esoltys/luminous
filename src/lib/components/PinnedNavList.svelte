<script lang="ts">
  import { pinnedStore } from "../stores/pinned.svelte";
  import { playerStore } from "../stores/player.svelte";
  import { navigationStore } from "../stores/navigation.svelte";
  import { i18n } from "../stores/i18n.svelte";
  import type { PinnedItem } from "../types";
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
  let pointerDragPointerId: number | null = null;
  let pointerDragEl: HTMLElement | null = null;

  let contextMenuState = $state<{ x: number; y: number; item: PinnedItem } | null>(null);

  function suppressOneClick(e: MouseEvent) {
    e.preventDefault();
    e.stopPropagation();
  }

  function handlePointerDown(e: PointerEvent, index: number) {
    if (e.button !== 0) return;
    const target = e.target as HTMLElement;
    if (target.closest("button:not([data-pinned-nav-item]), input, select, textarea")) return;

    pointerDragArmed = false;
    pointerDragStartX = e.clientX;
    pointerDragStartY = e.clientY;
    pointerDragPointerId = e.pointerId;
    pointerDragEl = e.currentTarget as HTMLElement;
    draggedIndex = index;

    window.addEventListener("pointermove", handlePointerMove);
    window.addEventListener("pointerup", handlePointerUp);
  }

  function handlePointerMove(e: PointerEvent) {
    if (draggedIndex === null) return;

    if (!pointerDragArmed) {
      const dx = e.clientX - pointerDragStartX;
      const dy = e.clientY - pointerDragStartY;
      if (Math.hypot(dx, dy) < POINTER_DRAG_THRESHOLD_PX) return;

      pointerDragArmed = true;
      if (pointerDragEl && pointerDragPointerId !== null) {
        pointerDragEl.setPointerCapture?.(pointerDragPointerId);
      }
    }

    const el = document
      .elementFromPoint(e.clientX, e.clientY)
      ?.closest("[data-pinned-nav-index]") as HTMLElement | null;
    const idx = el?.dataset.pinnedNavIndex;
    dragOverIndex = idx !== undefined ? Number(idx) : null;
  }

  async function handlePointerUp() {
    window.removeEventListener("pointermove", handlePointerMove);
    window.removeEventListener("pointerup", handlePointerUp);

    if (pointerDragArmed && dragOverIndex !== null && draggedIndex !== null) {
      const fromIdx = draggedIndex;
      const toIdx = dragOverIndex;
      window.addEventListener("click", suppressOneClick, { capture: true, once: true });
      if (fromIdx !== toIdx) {
        await pinnedStore.reorderVisible(fromIdx, toIdx);
      }
    }

    draggedIndex = null;
    dragOverIndex = null;
    pointerDragArmed = false;
    pointerDragPointerId = null;
    pointerDragEl = null;
  }

  function handleContextMenu(e: MouseEvent, item: PinnedItem) {
    e.preventDefault();
    e.stopPropagation();
    contextMenuState = { x: e.clientX, y: e.clientY, item };
  }

  function handleKeyDown(e: KeyboardEvent, index: number) {
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
    }
  }
</script>

{#if pinnedStore.navigableItems.length > 0}
  <div class="w-full flex flex-col {collapsed ? 'items-center py-1' : 'px-1 pt-1.5 pb-1'}">
    {#if !collapsed}
      <div class="px-2 pt-1 pb-1 text-[10px] font-semibold tracking-wider uppercase text-brand-text-secondary/60 select-none">
        {i18n.t('sidebar.pinned')}
      </div>
    {/if}

    <div class="w-full space-y-0.5 flex flex-col {collapsed ? 'items-center' : ''}">
      {#each pinnedStore.navigableItems as pin, index (pin.id)}
        <!-- svelte-ignore a11y_no_static_element_interactions -->
        <div
          data-pinned-nav-index={index}
          onpointerdown={(e) => handlePointerDown(e, index)}
          onkeydown={(e) => handleKeyDown(e, index)}
          class="relative w-full transition-opacity rounded-md {draggedIndex === index ? 'opacity-40 cursor-grabbing' : 'cursor-grab'}"
        >
          <!-- Drop target indicator line / ring -->
          {#if dragOverIndex === index && draggedIndex !== null && draggedIndex !== index}
            {#if collapsed}
              <div class="absolute inset-0 rounded-xl ring-2 ring-brand-accent pointer-events-none z-10"></div>
            {:else}
              <div
                class="absolute left-1 right-1 h-0.5 bg-brand-accent rounded-full pointer-events-none z-10 {draggedIndex < index ? '-bottom-0.5' : '-top-0.5'}"
              ></div>
            {/if}
          {/if}

          <PinnedNavItem
            {pin}
            {collapsed}
            onclick={() => pin.open()}
            oncontextmenu={(e) => handleContextMenu(e, pin.rawItem)}
          />
        </div>
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
