<script lang="ts">
  import type { AutoPlaylistItem, PinnedItem, PinnedItemType } from "../types";
  import { pinnedRefKeyFor } from "../types";
  import { pinnedStore } from "../stores/pinned.svelte";
  import { startPointerDrag, type PointerDragHandle } from "../utils/pointerDrag";
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
    queueAutoPlaylistAsPlaylist,
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
  import DragGhost from "./DragGhost.svelte";
  import {
    DiscIcon as Disc,
    MicrophoneStageIcon as MicrophoneStage,
    MusicNotesIcon as Music,
    PlaylistIcon as ListMusic,
    SparkleIcon as Sparkles,
  } from "phosphor-svelte";

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
        return pl
          ? getPlaylistDisplayName(pl)
          : (ap.genre ?? ap.decade ?? ap.bpm ?? ap.artistTag ?? ap.kind);
      }
    }
  }

  function itemLabel(item: PinnedItem): string {
    switch (item.type) {
      case "song":
        return item.song.title ?? "";
      case "album":
        return item.album.album ?? "";
      case "artist":
        return item.artist.name ?? "";
      case "playlist":
        return getPlaylistDisplayName(item.playlist);
      case "auto_playlist":
        return autoPlaylistLabel(item.autoPlaylist);
    }
  }

  function openItem(item: PinnedItem) {
    switch (item.type) {
      case "song":
        playerStore.playSong(item.song.id);
        break;
      case "album":
        navigationStore.viewAlbum(
          item.album.album || "",
          undefined,
          item.album.album_key ?? undefined,
        );
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
  // silently swallows every nested card's click. Hence `captureOn: "arm"` below.

  let draggedIndex = $state<number | null>(null);
  let dragOverIndex = $state<number | null>(null);
  /** Cursor position for the drag ghost; set only once a drag is armed, so a click never shows it. */
  let ghostPos = $state<{ x: number; y: number } | null>(null);
  let cardDrag: PointerDragHandle | null = null;

  // Drag indices are positions in visibleItems; hidden (empty) pins keep
  // their place in the full order around the moved card.
  function commitReorder(targetIndex: number) {
    if (draggedIndex === null || targetIndex === draggedIndex) return;
    const moved = visibleItems[draggedIndex];
    const target = visibleItems[targetIndex];
    const items = pinnedStore.items.filter((item) => item !== moved);
    const targetPos = items.indexOf(target);
    items.splice(targetIndex > draggedIndex ? targetPos + 1 : targetPos, 0, moved);
    const order: Array<[PinnedItemType, string]> = items.map((item) => [
      item.type,
      pinnedRefKeyFor(item),
    ]);
    pinnedStore.reorder(order);
  }

  function resetDrag() {
    draggedIndex = null;
    dragOverIndex = null;
    ghostPos = null;
    cardDrag = null;
  }

  function handleCardPointerDown(e: PointerEvent, index: number) {
    cardDrag = startPointerDrag(e, {
      captureOn: "arm",
      onMove: (pos) => {
        ghostPos = pos;
        const el = document
          .elementFromPoint(pos.x, pos.y)
          ?.closest("[data-pinned-index]") as HTMLElement | null;
        const idx = el?.dataset.pinnedIndex;
        dragOverIndex = idx !== undefined ? Number(idx) : null;
      },
      onDrop: () => {
        if (dragOverIndex !== null) commitReorder(dragOverIndex);
        resetDrag();
      },
      onTap: resetDrag,
      onCancel: resetDrag,
    });
    if (cardDrag) draggedIndex = index;
  }

  $effect(() => () => cardDrag?.cancel());

  let contextMenuState = $state<{ x: number; y: number; item: PinnedItem } | null>(null);

  function handleContextMenu(e: MouseEvent, item: PinnedItem) {
    e.preventDefault();
    e.stopPropagation();
    contextMenuState = { x: e.clientX, y: e.clientY, item };
  }
</script>

{#if visibleItems.length > 0}
  <CardSection
    title={i18n.t("home.pinned")}
    viewMode={prefs.pinnedViewMode}
    onViewModeChange={(m) => prefs.setPinnedViewMode(m)}
  >
    {#snippet children(mode)}
      {@const rows = mode === "rows"}
      {#each visibleItems as item, index (keyFor(item))}
        <!-- [role=button] cards (ArtistCard) would otherwise show app.css's default arrow instead of
             this wrapper's grab cursor. -->
        <!-- svelte-ignore a11y_no_static_element_interactions -->
        <div
          data-pinned-index={index}
          onpointerdown={(e) => handleCardPointerDown(e, index)}
          oncontextmenu={(e) => handleContextMenu(e, item)}
          ondragstart={(e) => e.preventDefault()}
          class="relative h-full [&>*:first-child]:h-full transition-opacity cursor-grab active:cursor-grabbing [&_[role=button]]:cursor-[inherit] {rows
            ? 'rounded-lg'
            : 'rounded-xl'} {draggedIndex === index ? 'opacity-40' : ''}"
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
            <div
              class="absolute inset-0 bg-brand-accent/30 pointer-events-none {rows
                ? 'rounded-lg'
                : 'rounded-xl'}"
            ></div>
          {/if}
        </div>
      {/each}
    {/snippet}
  </CardSection>
{/if}

{#if ghostPos && draggedIndex !== null && visibleItems[draggedIndex]}
  {@const dragged = visibleItems[draggedIndex]}
  <DragGhost
    x={ghostPos.x}
    y={ghostPos.y}
    overTarget={dragOverIndex !== null && dragOverIndex !== draggedIndex}
    testId="pinned-drag-ghost"
  >
    {#if dragged.type === "album"}
      <Disc class="w-3.5 h-3.5 text-brand-accent shrink-0" />
    {:else if dragged.type === "artist"}
      <MicrophoneStage class="w-3.5 h-3.5 text-brand-accent shrink-0" />
    {:else if dragged.type === "playlist"}
      <ListMusic class="w-3.5 h-3.5 text-brand-accent shrink-0" />
    {:else if dragged.type === "auto_playlist"}
      <Sparkles class="w-3.5 h-3.5 text-brand-accent shrink-0" />
    {:else}
      <Music class="w-3.5 h-3.5 text-brand-accent shrink-0" />
    {/if}
    <span class="max-w-[180px] truncate">{itemLabel(dragged)}</span>
  </DragGhost>
{/if}

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
