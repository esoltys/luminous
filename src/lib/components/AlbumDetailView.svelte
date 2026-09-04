<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { listen } from "@tauri-apps/api/event";
  import { applySongStats, type SongStatsPayload, applyAlbumStats, type AlbumStatsPayload } from "../utils/stats";
  import { collectionStore } from "../stores/collection.svelte";
  import { navigationStore } from "../stores/navigation.svelte";
  import { windowLayoutStore } from "../stores/windowLayout.svelte";
  import { playerStore } from "../stores/player.svelte";
  import { playlistsStore } from "../stores/playlists.svelte";
  import { pinnedStore } from "../stores/pinned.svelte";
  import { shuffleArray } from "../utils/shuffle";
  import CoverArt from "./CoverArt.svelte";
  import CoverStack from "./CoverStack.svelte";
  import SongRating from "./SongRating.svelte";
  import FavouriteCornerFlag from "./FavouriteCornerFlag.svelte";
  import BoxSetDiscIcons from "./BoxSetDiscIcons.svelte";
  import TagEditor from "./TagEditor.svelte";
  import AlbumTagEditor from "./AlbumTagEditor.svelte";
  import SongContextMenu from "./SongContextMenu.svelte";
  import GenreChips from "./GenreChips.svelte";
  import { tagsStore } from "../stores/tags.svelte";
  import SongSelectionToolbar from "./SongSelectionToolbar.svelte";
  import PlayShuffleButtons from "./PlayShuffleButtons.svelte";
  import IconActionButton from "./IconActionButton.svelte";
  import LinkButton from "./LinkButton.svelte";
  import ColumnSelector from "./ColumnSelector.svelte";
  import SongTable, { type SongTableRow } from "./SongTable.svelte";
  import {
    PlusIcon as Plus,
    PencilSimpleIcon as Edit3,
    ArrowsClockwiseIcon as RefreshCw,
    PushPinIcon as Pin,
    PushPinSlashIcon as PinOff
  } from "phosphor-svelte";
  import type { Song, AlbumItem, PlayContext } from "../types";
  import { getCoverArtUrl, resolveArtUrl } from "../types";
  import { i18n } from "../stores/i18n.svelte";
  import { toastStore } from "../stores/toast.svelte";
  import { compareSongs } from "../utils/songSort";
  import { rememberScroll } from "../utils/scrollMemory";
  import { openInPicard } from "../utils/picard";

  let { albumName }: { albumName: string } = $props();

  let songs = $state<Song[]>([]);
  let loading = $state(true);
  let refreshing = $state(false);
  let editingSongId = $state<number | null>(null);
  let showAlbumTagEditor = $state(false);
  let contextMenuState = $state<{ x: number; y: number; song: Song } | null>(null);

  async function handleRefreshAlbum() {
    if (refreshing || collectionStore.isScanning) return;
    refreshing = true;
    try {
      await collectionStore.startScan(true);
      await collectionStore.refreshLibrary();
      const fetchedSongs = await invoke<Song[]>("get_songs_by_album", { album: albumName });
      let filtered = [...fetchedSongs];
      filtered.sort((a, b) => {
        if (a.disc !== b.disc) {
          return (a.disc ?? 1) - (b.disc ?? 1);
        }
        return (a.track ?? 0) - (b.track ?? 0);
      });
      songs = filtered;
      toastStore.show(i18n.t("albumDetail.refreshSuccess", {}, "Album metadata and artwork refreshed"));
    } catch (err) {
      console.error("Failed to refresh album:", err);
      toastStore.show(i18n.t("albumDetail.refreshError", {}, "Failed to refresh album metadata"));
    } finally {
      refreshing = false;
    }
  }

  let selectedKeys = $state<Set<string>>(new Set());

  function handleRowContextMenu(event: MouseEvent, row: SongTableRow) {
    if (row.song) contextMenuState = { x: event.clientX, y: event.clientY, song: row.song };
  }

  async function handleBulkAddToPlaylist() {
    if (selectedKeys.size === 0) return;
    const songIds = Array.from(selectedKeys, Number);
    const label = songIds.length === 1 ? "1 song" : `${songIds.length} songs`;
    await playlistsStore.addSongsToActiveTarget(songIds, label);
  }

  function albumPlayContext(): PlayContext {
    return { type: "album", album: albumName, albumArtist: artistName || undefined };
  }

  function handlePlaySelected() {
    if (selectedKeys.size === 0) return;
    const selectedList = songs.filter((s) => selectedKeys.has(String(s.id)));
    if (selectedList.length > 0) {
      playerStore.playSongs(selectedList.map((s) => s.id), 0, undefined, albumPlayContext());
    }
  }

  let albumItem = $derived(
    collectionStore.albums.find((a) => a.album === albumName) || null
  );

  /** Any one track's id, used to look up extended local artwork (#98/#760)
   * for this album's directory — there's no dedicated album id in the
   * schema (see #758), so a representative song stands in for one. */
  let representativeSongId = $derived(songs[0]?.id);

  let artistName = $derived.by(() => {
    if (albumItem?.artist) return albumItem.artist;
    if (songs.length > 0) return songs[0].album_artist || songs[0].artist || "";
    return "";
  });

  // Resolves the same art_manual/art_automatic/art_embedded precedence as
  // CoverArt.svelte and themeStore.updateArtworkColors, but at album level —
  // embedded art has no album-wide URI, so it falls back to a representative song.
  let backdropUrl = $state<string | null>(null);

  $effect(() => {
    const item = albumItem;
    const fallbackSongId = item?.sample_song_id ?? songs[0]?.id;
    let cancelled = false;

    async function resolve() {
      let url: string | null = null;
      if (item?.art_manual) {
        url = resolveArtUrl(item.art_manual);
      } else if (item?.art_automatic) {
        url = resolveArtUrl(item.art_automatic);
      } else if (item?.art_embedded && fallbackSongId !== undefined) {
        try {
          const uri = await invoke<string | null>("get_cover_art_uri", { songId: fallbackSongId });
          if (uri) url = getCoverArtUrl(uri);
        } catch (e) {
          console.error("Failed to load album backdrop art:", e);
        }
      }
      if (!cancelled) backdropUrl = url;
    }

    resolve();
    return () => {
      cancelled = true;
    };
  });

  // Whether *any* track carries a disc > 1 decides whether every row
  // (including disc-1 tracks) gets the "{disc}-{track}" prefix — a plain
  // song.disc_count read wouldn't reflect songs still loading/missing tags.
  let discCount = $derived(songs.reduce((max, s) => Math.max(max, s.disc ?? 1), 1));

  let rawGenre = $derived.by(() => (songs.length > 0 ? songs[0].genre : undefined));

  let genreLabel = $derived(rawGenre || i18n.t('albumDetail.unknownGenre'));

  let yearLabel = $derived.by(() => {
    if (albumItem?.year) return albumItem.year;
    if (songs.length > 0 && songs[0].year) return songs[0].year;
    return null;
  });

  let totalDurationLabel = $derived.by(() => {
    const totalNs = songs.reduce((sum, s) => sum + (s.length_nanosec ?? 0), 0);
    const totalMinutes = Math.round(totalNs / 1_000_000_000 / 60);
    const h = Math.floor(totalMinutes / 60);
    const m = totalMinutes % 60;
    return h > 0 ? `${h}h ${m}m` : `${m}m`;
  });

  $effect(() => {
    const requested = albumName;
    loading = true;
    invoke<Song[]>("get_songs_by_album", { album: requested })
      .then((fetchedSongs) => {
        if (requested !== albumName) return;
        let filtered = [...fetchedSongs];
        filtered.sort((a, b) => {
          if (a.disc !== b.disc) {
            return (a.disc ?? 1) - (b.disc ?? 1);
          }
          return (a.track ?? 0) - (b.track ?? 0);
        });
        songs = filtered;
      })
      .catch((err) => {
        console.error("Failed to load album detail:", err);
      })
      .finally(() => {
        if (requested === albumName) loading = false;
      });
  });

  type AlbumSortField = keyof Song | "track";
  let sortField = $state<AlbumSortField>("track");
  let sortAsc = $state(true);

  function toggleSort(field: string) {
    const f = field as AlbumSortField;
    if (sortField === f) {
      sortAsc = !sortAsc;
    } else {
      sortField = f;
      sortAsc = true;
    }
  }

  let sortedSongs = $derived.by(() => {
    if (sortField === "track") {
      if (sortAsc) return songs;
      return [...songs].reverse();
    }
    const field = sortField as keyof Song;
    return [...songs].sort((a, b) => compareSongs(a, b, field, sortAsc));
  });

  // "Not included" tracks stay visible and individually playable (clicking
  // a row plays the full `sortedSongs` list, that track included), but bulk
  // "play the whole album" actions — Play/Shuffle Play buttons, Add Album
  // to Playlist — build their queue from this filtered list instead (#104).
  let playableSongs = $derived(sortedSongs.filter((s) => !s.not_included));

  // Default column widths (px or fr) — used when no saved width exists for a column.
  const ALBUM_COL_DEFAULTS: Partial<Record<keyof typeof collectionStore.visibleColumns, string>> = {
    track: "48px", title: "2fr", artist: "1.5fr", album: "1.5fr",
    composer: "1.5fr", album_artist: "1.5fr", format: "64px", year: "60px", originalyear: "60px",
    genre: "1.2fr", grouping: "1.2fr", bpm: "60px", initial_key: "60px",
    bitrate: "70px", samplerate: "75px", bitdepth: "65px", channels: "70px",
    filesize: "75px", rating: "96px", playcount: "70px", skipcount: "70px",
    lastplayed: "90px", added: "90px", duration: "80px", path: "2fr", actions: "80px",
  };

  function songToRow(song: Song): SongTableRow {
    const disconnected = !song.unavailable && collectionStore.isPathOnDisconnectedDrive(song.path);
    return {
      key: String(song.id),
      song,
      disabled: song.unavailable || disconnected,
      disabledTooltip: disconnected ? i18n.t("collection.driveDisconnectedTooltip") : undefined,
    };
  }

  let tableRows = $derived(sortedSongs.map(songToRow));
  // Range (shift-click) selection matches natural track order, not the
  // currently displayed sort — preserving the existing behavior.
  let rangeSelectionRows = $derived(songs.map(songToRow));

  function goBack() {
    navigationStore.selectedAlbumName = null;
    navigationStore.activeSubTab = "albums";
  }

  async function handlePlaySong(song: Song) {
    const list = sortedSongs;
    const index = list.findIndex((s) => s.id === song.id);
    const songIds = list.map((s) => s.id);
    await playerStore.playSongs(songIds, index >= 0 ? index : 0, undefined, albumPlayContext());
  }

  async function handlePlayAll() {
    if (playableSongs.length === 0) return;
    await playerStore.setShuffleMode("off");
    await playerStore.playSongs(playableSongs.map((s) => s.id), 0, undefined, albumPlayContext());
  }

  async function handleShufflePlay() {
    if (playableSongs.length === 0) return;
    const shuffledIds = shuffleArray(playableSongs.map((s) => s.id));
    await playerStore.setShuffleMode("off");
    await playerStore.playSongs(shuffledIds, 0, undefined, albumPlayContext());
  }

  async function handleAddSongToPlaylist(songId: number) {
    const songObj = songs.find((s) => s.id === songId);
    await playlistsStore.addSongsToActiveTarget([songId], songObj?.title || "Song");
  }

  async function handleAddAlbumToPlaylist() {
    if (playableSongs.length === 0) return;
    await playlistsStore.addSongsToActiveTarget(
      playableSongs.map((s) => s.id),
      albumName || "Album"
    );
  }

  function openTagEditor(songId: number) {
    editingSongId = songId;
  }

  function openAlbumTagEditor() {
    if (songs.length === 0) return;
    showAlbumTagEditor = true;
  }

  async function handleTagEditorSaved() {
    collectionStore.refreshLibrary();
    tagsStore.load();
    loading = true;

    // The album-level editor renames every song currently loaded here at once,
    // so if it changed the album name, this view's `albumName` prop is now
    // stale and re-querying by it would come up empty. Resolve the current
    // name from one of the album's own songs and hand off to navigationStore
    // so the album-name effect refetches under the new name. A single-song
    // edit only ever touches one track, so it's left to the normal refetch
    // below, which naturally drops that song if it moved to a different album.
    if (showAlbumTagEditor && songs[0]?.id !== undefined) {
      try {
        const details = await invoke<{ album: string }>("get_song_details", { songId: songs[0].id });
        if (details.album && details.album !== albumName) {
          navigationStore.selectedAlbumName = details.album;
          return;
        }
      } catch (err) {
        console.error("Failed to resolve current album name after tag edit:", err);
      }
    }

    try {
      const fetchedSongs = await invoke<Song[]>("get_songs_by_album", { album: albumName });
      let filtered = [...fetchedSongs];
      filtered.sort((a, b) => {
        if (a.disc !== b.disc) {
          return (a.disc ?? 1) - (b.disc ?? 1);
        }
        return (a.track ?? 0) - (b.track ?? 0);
      });
      songs = filtered;
    } catch (err) {
      console.error(err);
    } finally {
      loading = false;
    }
  }

  async function rateSong(song: Song, rating: number) {
    song.rating = await invoke<number>("set_song_rating", { songId: song.id, rating });
  }

  async function rateAlbum(rating: number) {
    if (!albumItem?.album) return;
    const normalized = await invoke<number>("set_album_rating", { album: albumItem.album, rating });
    albumItem.rating = normalized;
  }

  // Sync rating/playcount changes from other views and scrobble bumps into
  // this view's locally fetched song list.
  $effect(() => {
    let unlisten: (() => void) | undefined;
    let disposed = false;
    listen<SongStatsPayload>("song-stats-changed", (event) => {
      const song = songs.find((s) => s.id === event.payload.song_id);
      if (song) applySongStats(song, event.payload);
    }).then((fn) => {
      if (disposed) fn();
      else unlisten = fn;
    });
    return () => {
      disposed = true;
      unlisten?.();
    };
  });

  // Sync album rating changes made from other views (e.g. the Collection grid).
  $effect(() => {
    let unlisten: (() => void) | undefined;
    let disposed = false;
    listen<AlbumStatsPayload>("album-stats-changed", (event) => {
      if (albumItem && albumItem.album === event.payload.album) applyAlbumStats(albumItem, event.payload);
    }).then((fn) => {
      if (disposed) fn();
      else unlisten = fn;
    });
    return () => {
      disposed = true;
      unlisten?.();
    };
  });
</script>

<div
  class="relative flex-1 flex flex-col overflow-y-auto text-brand-text-secondary h-full carousel-scroll {backdropUrl ? '' : 'bg-brand-main'}"
  use:rememberScroll={`album-detail:${albumName}`}
>
  {#if backdropUrl}
    <div class="absolute inset-0 z-0 overflow-hidden pointer-events-none" aria-hidden="true">
      <img
        src={backdropUrl}
        alt=""
        class="w-full h-full object-cover blur-2xl"
        style="will-change: filter; transform: translateZ(0) scale(1.5);"
      />
      <div class="absolute inset-0 bg-gradient-to-b from-transparent via-transparent to-brand-main"></div>
    </div>
  {/if}

  <div class="relative z-30 w-full border-b border-brand-border/60 bg-brand-main/60 backdrop-blur-md px-6 {windowLayoutStore.isDetailHeaderCollapsed ? 'py-3' : 'pt-6 pb-6'}">
    <div class="flex items-start justify-between gap-6 relative z-10">
      <div class="flex flex-col justify-end gap-1.5 min-w-0 max-w-xl">
        {#if !windowLayoutStore.isDetailHeaderCollapsed}
        <h1 class="text-3xl sm:text-4xl font-heading font-bold text-brand-text-primary leading-snug truncate py-0.5" title={albumName}>
          {albumName}
        </h1>

        <div class="flex items-center gap-2 text-base font-semibold text-brand-accent-text">
          {#if artistName}
            <LinkButton
              onclick={() => navigationStore.viewArtist(artistName)}
              class="font-bold"
            >
              {artistName}
            </LinkButton>
          {:else}
            <span class="text-brand-text-secondary">{i18n.t('collection.unknownArtist')}</span>
          {/if}
        </div>

        <div class="flex flex-wrap items-center gap-x-2 gap-y-1 text-xs text-brand-text-secondary font-medium">
          {#if rawGenre}
            <GenreChips genre={rawGenre} variant="full" />
          {:else}
            <span>{genreLabel}</span>
          {/if}
          <span>•</span>
          {#if yearLabel}
            <span>{yearLabel}</span>
            <span>•</span>
          {/if}
          <span>{songs.length === 1 ? i18n.t('playlists.oneSong') : i18n.t('playlists.songsCount', { count: songs.length })}</span>
          <span>•</span>
          <span>{totalDurationLabel}</span>
          {#if albumItem}
            <span>•</span>
            <SongRating rating={albumItem.rating} onRate={rateAlbum} size="sm" />
          {/if}
        </div>
        {/if}

        <div class="flex flex-wrap items-center gap-3 mt-3 select-none">
          <PlayShuffleButtons
            onPlayAll={handlePlayAll}
            onShufflePlay={handleShufflePlay}
            disabled={loading || songs.length === 0}
          />
          <IconActionButton
            onclick={handleAddAlbumToPlaylist}
            disabled={loading || songs.length === 0}
            title={playlistsStore.activeCustomPlaylist
              ? i18n.t('albumDetail.addAllToPlaylistTooltip', { name: playlistsStore.activeCustomPlaylist.name })
              : i18n.t('albumDetail.addAllToPlaylistTooltipDefault')}
          >
            {#snippet icon()}<Plus class="w-4 h-4" />{/snippet}
          </IconActionButton>
          <IconActionButton
            onclick={openAlbumTagEditor}
            disabled={loading || songs.length === 0}
            title={i18n.t('albumDetail.editInfoTooltip')}
          >
            {#snippet icon()}<Edit3 class="w-4 h-4" />{/snippet}
          </IconActionButton>
          <IconActionButton
            onclick={handleRefreshAlbum}
            disabled={loading || collectionStore.isScanning || refreshing}
            title={i18n.t('albumDetail.refreshTooltip')}
          >
            {#snippet icon()}<RefreshCw class="w-4 h-4 {refreshing || collectionStore.isScanning ? 'animate-spin' : ''}" />{/snippet}
          </IconActionButton>
          <IconActionButton
            onclick={() => pinnedStore.toggle("album", albumName)}
            title={pinnedStore.isPinned("album", albumName)
              ? i18n.t("playlists.contextMenuUnpinHome")
              : i18n.t("playlists.contextMenuPinHome")}
          >
            {#snippet icon()}
              {#if pinnedStore.isPinned("album", albumName)}
                <PinOff class="w-4 h-4" />
              {:else}
                <Pin class="w-4 h-4" />
              {/if}
            {/snippet}
          </IconActionButton>
          <ColumnSelector align="left" iconOnly />
        </div>
      </div>

      {#if !windowLayoutStore.isDetailHeaderCollapsed}
      <div class="relative w-40 h-40 hidden sm:block shrink-0">
        <div class="absolute inset-0 overflow-hidden border border-brand-border/60 shadow-2xl">
          <CoverStack
            covers={[{
              artEmbedded: albumItem?.art_embedded,
              artAutomatic: albumItem?.art_automatic,
              artManual: albumItem?.art_manual,
            }]}
            extendedArtworkSongId={representativeSongId}
            sizeClass="w-full h-full object-cover"
          />
          {#if albumItem && albumItem.rating === 5}
            <FavouriteCornerFlag size="lg" />
          {/if}
          {#if albumItem && albumItem.disc_count > 1}
            <BoxSetDiscIcons discCount={albumItem.disc_count} size="md" />
          {/if}
        </div>
      </div>
      {/if}
    </div>
  </div>

  <div class="relative z-10 px-6 py-6" class:pb-28={!!playerStore.currentSong}>
    <div class="border border-brand-border rounded-lg bg-brand-sidebar/50 backdrop-blur-xl shadow-2xl overflow-hidden table-surface-blur">
      <SongTable
        rows={tableRows}
        rangeSelectionOrder={rangeSelectionRows}
        mode="track"
        {discCount}
        leadingColumnWidth="36px"
        colDefaults={ALBUM_COL_DEFAULTS}
        {sortField}
        {sortAsc}
        onToggleSort={toggleSort}
        bind:selectedKeys
        {loading}
        onRowDoubleClick={(row) => row.song && handlePlaySong(row.song)}
        onRowContextMenu={handleRowContextMenu}
        onRate={rateSong}
        onAddToPlaylist={(song) => handleAddSongToPlaylist(song.id)}
        onEditTags={(song) => openTagEditor(song.id)}
        onOpenInPicard={(song) => openInPicard([song.id])}
      />
    </div>
  </div>
</div>

{#if editingSongId !== null}
  <TagEditor
    songId={editingSongId}
    onClose={() => { editingSongId = null; }}
    onSave={handleTagEditorSaved}
  />
{/if}

{#if showAlbumTagEditor && songs.length > 0}
  <AlbumTagEditor
    songIds={songs.map((s) => s.id)}
    initialAlbum={songs[0].album}
    initialAlbumSort={songs[0].albumsort}
    initialAlbumArtist={songs[0].album_artist || songs[0].artist}
    initialAlbumArtistSort={songs[0].album_artist_sort || songs[0].artistsort}
    initialGenre={songs[0].genre}
    initialGenreSort={songs[0].genresort}
    initialYear={songs[0].year}
    initialDisc={songs[0].disc}
    initialCompilation={songs[0].compilation}
    hasEmbeddedArt={songs.some((s) => s.art_embedded)}
    initialArtAutomatic={albumItem?.art_automatic}
    initialArtManual={albumItem?.art_manual}
    onClose={() => { showAlbumTagEditor = false; }}
    onSave={handleTagEditorSaved}
  />
{/if}

{#if contextMenuState}
  {@const song = contextMenuState.song}
  <SongContextMenu
    x={contextMenuState.x}
    y={contextMenuState.y}
    {song}
    selectedCount={selectedKeys.size}
    selectedSongIds={Array.from(selectedKeys, Number)}
    onPlay={() => {
      if (selectedKeys.size > 1) {
        handlePlaySelected();
      } else {
        handlePlaySong(song);
      }
    }}
    onAddToPlaylist={() => {
      if (selectedKeys.size > 1) {
        handleBulkAddToPlaylist();
      } else {
        handleAddSongToPlaylist(song.id);
      }
    }}
    onGoToArtist={() => navigationStore.viewArtist(song.album_artist?.trim() || song.artist || "")}
    onGoToAlbum={() => navigationStore.viewAlbum(song.album || "")}
    onEditTags={() => openTagEditor(song.id)}
    onOpenInPicard={() => openInPicard(selectedKeys.size > 1 ? Array.from(selectedKeys, Number) : [song.id])}
    onClose={() => { contextMenuState = null; }}
  />
{/if}

{#if selectedKeys.size > 0}
  <SongSelectionToolbar
    count={selectedKeys.size}
    onPlaySelected={handlePlaySelected}
    onAddToPlaylist={handleBulkAddToPlaylist}
    onClear={() => { selectedKeys = new Set(); }}
  />
{/if}

<style>
  :global(.carousel-scroll) {
    scrollbar-width: none;
    -ms-overflow-style: none;
  }
  :global(.carousel-scroll::-webkit-scrollbar) {
    display: none;
  }
</style>
