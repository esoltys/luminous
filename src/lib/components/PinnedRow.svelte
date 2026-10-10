<script lang="ts">
  import type { AutoPlaylistItem, PinnedItem, PinnedItemType } from "../types";
  import { pinnedRefKeyFor } from "../types";
  import { pinnedStore } from "../stores/pinned.svelte";
  import { playerStore } from "../stores/player.svelte";
  import { playlistsStore } from "../stores/playlists.svelte";
  import { navigationStore } from "../stores/navigation.svelte";
  import { i18n } from "../stores/i18n.svelte";
  import { getArtistAlbums, getArtistSongs } from "../utils/artist";
  import {
    getPlaylistDisplayName,
    queueAlbumAsPlaylist,
    queueArtistAsPlaylist,
    queuePlaylistAsPlaylist,
    queueAutoPlaylistAsPlaylist
  } from "../utils/playlist";
  import { collectionStore } from "../stores/collection.svelte";
  import CardSection from "./CardSection.svelte";
  import { prefs } from "../stores/prefs.svelte";
  import AlbumCard from "./AlbumCard.svelte";
  import AlbumRowCard from "./AlbumRowCard.svelte";
  import ArtistCard from "./ArtistCard.svelte";
  import ArtistRowCard from "./ArtistRowCard.svelte";
  import PlaylistCard from "./PlaylistCard.svelte";
  import PlaylistRowCard from "./PlaylistRowCard.svelte";
  import AutoPlaylistCard from "./AutoPlaylistCard.svelte";
  import AutoPlaylistRowCard from "./AutoPlaylistRowCard.svelte";
  import PinnedSongCard from "./PinnedSongCard.svelte";
  import PinnedSongRowCard from "./PinnedSongRowCard.svelte";
  import AlbumContextMenu from "./AlbumContextMenu.svelte";
  import SongContextMenu from "./SongContextMenu.svelte";
  import ArtistContextMenu from "./ArtistContextMenu.svelte";
  import PlaylistCardContextMenu from "./PlaylistCardContextMenu.svelte";
  import AutoPlaylistContextMenu from "./AutoPlaylistContextMenu.svelte";

  function keyFor(item: PinnedItem): string {
    return `${item.type}:${pinnedRefKeyFor(item)}`;
  }

  // Empty playlists stay pinned but aren't shown, so the default pins (Moment
  // Mix, Favourites) only appear on Home once they have songs to play.
  function hasContent(item: PinnedItem): boolean {
    if (item.type === "playlist") return item.playlist.track_count > 0;
    if (item.type === "auto_playlist") return item.autoPlaylist.trackCount > 0;
    return true;
  }

  const visibleItems = $derived(pinnedStore.items.filter(hasContent));

  // Mirrors PlaylistsCollectionView's autoDefs label derivation — Favourites/
  // Recently Added/Most Played/History use fixed i18n labels, while genre/
  // decade/bpm/artist_tag defer to the materialized playlist's own display
  // name (falling back to the raw selector if the row hasn't resolved yet).
  function autoPlaylistLabel(ap: AutoPlaylistItem): string {
    switch (ap.kind) {
      case "favourites":
        return i18n.t("playlists.autoFavourites");
      case "recently_added":
        return i18n.t("playlists.autoRecentlyAdded");
      case "most_played":
        return i18n.t("playlists.autoMostPlayed");
      case "history":
        return i18n.t("playlists.autoHistory");
      case "daypart": {
        const pl = playlistsStore.playlists.find((p) => p.id === ap.playlistId);
        return pl ? getPlaylistDisplayName(pl) : i18n.t("playlists.daypartAutoPlaylist");
      }
      default: {
        const pl = playlistsStore.playlists.find((p) => p.id === ap.playlistId);
        return pl ? getPlaylistDisplayName(pl) : (ap.genre ?? ap.decade ?? ap.bpm ?? ap.artistTag ?? ap.kind);
      }
    }
  }

  function openItem(item: PinnedItem) {
    switch (item.type) {
      case "song":
        playerStore.playSong(item.song.id);
        break;
      case "album":
        navigationStore.viewAlbum(item.album.album || "");
        break;
      case "artist":
        navigationStore.viewArtist(item.artist.name || "");
        break;
      case "playlist":
        navigationStore.viewPlaylist(item.playlist.id);
        break;
      case "auto_playlist":
        navigationStore.viewAutoPlaylist({
          kind: item.autoPlaylist.kind,
          genre: item.autoPlaylist.genre,
          artistTag: item.autoPlaylist.artistTag,
          decade: item.autoPlaylist.decade,
          bpm: item.autoPlaylist.bpm,
          playlistId: item.autoPlaylist.playlistId,
          updated: item.autoPlaylist.updated,
        });
        break;
    }
  }

  // Pointer-based drag reorder — native HTML5 drag events are swallowed by
  // Tauri's dragDropEnabled window option (needed for OS file-drop import
  // into the queue), so reordering uses pointer events instead, mirroring
  // SongTable.svelte's pointer-drag reorder. Unlike SongTable's row (whose
  // own onclick lives on the exact element that takes pointer capture),
  // each pinned card's click handler lives on a nested child element
  // (AlbumCard/ArtistCard/etc.'s own root), so capture is deferred until a
  // drag is actually armed — capturing eagerly on every pointerdown, as
  // SongTable does, retargets the *click* event to this wrapper too and
  // silently swallows every nested card's click.
  const POINTER_DRAG_THRESHOLD_PX = 4;

  let draggedIndex = $state<number | null>(null);
  let dragOverIndex = $state<number | null>(null);
  let pointerDragArmed = false;
  let pointerDragStartX = 0;
  let pointerDragStartY = 0;
  let pointerDragPointerId: number | null = null;
  let pointerDragEl: HTMLElement | null = null;

  // Drag indices are positions in visibleItems; hidden (empty) pins keep
  // their place in the full order around the moved card.
  function commitReorder(targetIndex: number) {
    if (draggedIndex === null || targetIndex === draggedIndex) return;
    const moved = visibleItems[draggedIndex];
    const target = visibleItems[targetIndex];
    const items = pinnedStore.items.filter((item) => item !== moved);
    const targetPos = items.indexOf(target);
    items.splice(targetIndex > draggedIndex ? targetPos + 1 : targetPos, 0, moved);
    const order: Array<[PinnedItemType, string]> = items.map((item) => [item.type, pinnedRefKeyFor(item)]);
    pinnedStore.reorder(order);
  }

  // Eats the single click that follows a committed drag, so releasing the
  // pointer over a different card doesn't also open that card.
  function suppressOneClick(e: MouseEvent) {
    e.preventDefault();
    e.stopPropagation();
  }

  function handleCardPointerDown(e: PointerEvent, index: number) {
    if (e.button !== 0) return;
    const target = e.target as HTMLElement;
    if (target.closest("button, a, input, select, textarea, [data-interactive]")) return;
    pointerDragArmed = false;
    pointerDragStartX = e.clientX;
    pointerDragStartY = e.clientY;
    pointerDragPointerId = e.pointerId;
    pointerDragEl = e.currentTarget as HTMLElement;
    draggedIndex = index;
    window.addEventListener("pointermove", handlePointerDragMove);
    window.addEventListener("pointerup", handlePointerDragUp);
  }

  function handlePointerDragMove(e: PointerEvent) {
    if (draggedIndex === null) return;
    if (!pointerDragArmed) {
      const dx = e.clientX - pointerDragStartX;
      const dy = e.clientY - pointerDragStartY;
      if (Math.hypot(dx, dy) < POINTER_DRAG_THRESHOLD_PX) return;
      pointerDragArmed = true;
      // Only once an actual drag is confirmed: pin subsequent pointer events
      // to this element/JS loop so WebView2 can't hijack the in-progress
      // gesture into a native OS drag (which — with dragDropEnabled on —
      // would otherwise surface the "drop files to queue" overlay).
      if (pointerDragEl && pointerDragPointerId !== null) {
        pointerDragEl.setPointerCapture?.(pointerDragPointerId);
      }
    }
    const el = document.elementFromPoint(e.clientX, e.clientY)?.closest("[data-pinned-index]") as HTMLElement | null;
    const idx = el?.dataset.pinnedIndex;
    dragOverIndex = idx !== undefined ? Number(idx) : null;
  }

  function handlePointerDragUp() {
    window.removeEventListener("pointermove", handlePointerDragMove);
    window.removeEventListener("pointerup", handlePointerDragUp);
    if (pointerDragArmed && dragOverIndex !== null) {
      commitReorder(dragOverIndex);
      window.addEventListener("click", suppressOneClick, { capture: true, once: true });
    }
    draggedIndex = null;
    dragOverIndex = null;
    pointerDragArmed = false;
    pointerDragPointerId = null;
    pointerDragEl = null;
  }

  let contextMenuState = $state<{ x: number; y: number; item: PinnedItem } | null>(null);

  function handleContextMenu(e: MouseEvent, item: PinnedItem) {
    e.preventDefault();
    e.stopPropagation();
    contextMenuState = { x: e.clientX, y: e.clientY, item };
  }
</script>

{#if visibleItems.length > 0}
  <CardSection
    title={i18n.t('home.pinned')}
    viewMode={prefs.pinnedViewMode}
    onViewModeChange={(m) => prefs.setPinnedViewMode(m)}
  >
    {#snippet children(mode)}
      {@const rows = mode === "rows"}
      {#each visibleItems as item, index (keyFor(item))}
        <!-- svelte-ignore a11y_no_static_element_interactions -->
        <div
          data-pinned-index={index}
          onpointerdown={(e) => handleCardPointerDown(e, index)}
          oncontextmenu={(e) => handleContextMenu(e, item)}
          ondragstart={(e) => e.preventDefault()}
          class="relative transition-opacity cursor-grab active:cursor-grabbing {rows ? 'rounded-lg' : 'rounded-xl'} {draggedIndex === index ? 'opacity-40' : ''}"
        >
          {#if item.type === "album"}
            {#if rows}
              <AlbumRowCard
                album={item.album}
                onclick={() => openItem(item)}
                oncontextmenu={(e) => handleContextMenu(e, item)}
              />
            {:else}
              <AlbumCard
                album={item.album}
                widthClass="w-full"
                onclick={() => openItem(item)}
                oncontextmenu={(e) => handleContextMenu(e, item)}
              />
            {/if}
          {:else if item.type === "artist"}
            {#if rows}
              <ArtistRowCard
                artist={item.artist}
                artistAlbums={getArtistAlbums(collectionStore.albums, item.artist.name)}
                artistSongs={getArtistSongs(collectionStore.songs, item.artist.name)}
                onclick={() => openItem(item)}
                oncontextmenu={(e) => handleContextMenu(e, item)}
              />
            {:else}
              <ArtistCard
                artist={item.artist}
                artistAlbums={getArtistAlbums(collectionStore.albums, item.artist.name)}
                artistSongs={getArtistSongs(collectionStore.songs, item.artist.name)}
                onclick={() => openItem(item)}
                oncontextmenu={(e) => handleContextMenu(e, item)}
              />
            {/if}
          {:else if item.type === "playlist"}
            {#if rows}
              <PlaylistRowCard
                playlist={item.playlist}
                onClick={() => openItem(item)}
                oncontextmenu={(e) => handleContextMenu(e, item)}
              />
            {:else}
              <PlaylistCard
                playlist={item.playlist}
                widthClass="w-full"
                onClick={() => openItem(item)}
                oncontextmenu={(e) => handleContextMenu(e, item)}
              />
            {/if}
          {:else if item.type === "auto_playlist"}
            {#if rows}
              <AutoPlaylistRowCard
                label={autoPlaylistLabel(item.autoPlaylist)}
                kind={item.autoPlaylist.kind}
                genre={item.autoPlaylist.genre}
                artistTag={item.autoPlaylist.artistTag}
                decade={item.autoPlaylist.decade}
                bpm={item.autoPlaylist.bpm}
                playlistId={item.autoPlaylist.playlistId}
                updated={item.autoPlaylist.updated}
                trackCount={item.autoPlaylist.trackCount}
                onClick={() => openItem(item)}
                oncontextmenu={(e) => handleContextMenu(e, item)}
              />
            {:else}
              <AutoPlaylistCard
                label={autoPlaylistLabel(item.autoPlaylist)}
                kind={item.autoPlaylist.kind}
                genre={item.autoPlaylist.genre}
                artistTag={item.autoPlaylist.artistTag}
                decade={item.autoPlaylist.decade}
                bpm={item.autoPlaylist.bpm}
                playlistId={item.autoPlaylist.playlistId}
                updated={item.autoPlaylist.updated}
                trackCount={item.autoPlaylist.trackCount}
                onClick={() => openItem(item)}
                oncontextmenu={(e) => handleContextMenu(e, item)}
              />
            {/if}
          {:else if rows}
            <PinnedSongRowCard
              song={item.song}
              onclick={() => openItem(item)}
              oncontextmenu={(e) => handleContextMenu(e, item)}
            />
          {:else}
            <PinnedSongCard
              song={item.song}
              onclick={() => openItem(item)}
              oncontextmenu={(e) => handleContextMenu(e, item)}
            />
          {/if}
          {#if dragOverIndex === index && draggedIndex !== null && draggedIndex !== index}
            <div class="absolute inset-0 bg-brand-accent/30 pointer-events-none {rows ? 'rounded-lg' : 'rounded-xl'}"></div>
          {/if}
        </div>
      {/each}
    {/snippet}
  </CardSection>
{/if}

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
