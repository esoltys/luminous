<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { listen } from "@tauri-apps/api/event";
  import { applySongStats, type SongStatsPayload, applyAlbumStats, type AlbumStatsPayload } from "../utils/stats";
  import { collectionStore } from "../stores/collection.svelte";
  import { playerStore } from "../stores/player.svelte";
  import { playlistsStore } from "../stores/playlists.svelte";
  import { shuffleArray } from "../utils/shuffle";
  import CoverArt from "./CoverArt.svelte";
  import SongRating from "./SongRating.svelte";
  import FavouriteCornerFlag from "./FavouriteCornerFlag.svelte";
  import BoxSetDiscIcons from "./BoxSetDiscIcons.svelte";
  import TagEditor from "./TagEditor.svelte";
  import AlbumTagEditor from "./AlbumTagEditor.svelte";
  import SongContextMenu from "./SongContextMenu.svelte";
  import SortableHeader from "./SortableHeader.svelte";
  import SongSelectionToolbar from "./SongSelectionToolbar.svelte";
  import EmptyState from "./EmptyState.svelte";
  import PlayShuffleButtons from "./PlayShuffleButtons.svelte";
  import NowPlayingBars from "./NowPlayingBars.svelte";
  import IconActionButton from "./IconActionButton.svelte";
  import LinkButton from "./LinkButton.svelte";
  import ColumnSelector from "./ColumnSelector.svelte";
  import { Play, Plus, Edit3, RefreshCw, Clock, Music } from "lucide-svelte";
  import type { Song, AlbumItem, PlayContext } from "../types";
  import { getCoverArtUrl } from "../types";
  import { i18n } from "../stores/i18n.svelte";
  import { toastStore } from "../stores/toast.svelte";
  import { formatTrackNumber } from "../utils/artist";
  import { formatDate, formatFileSize, formatSampleRate, formatBitDepth, formatChannels } from "../utils/formatters";
  import { formatDateAdded } from "../utils/date";
  import { rememberScroll } from "../utils/scrollMemory";
  import { SONG_TABLE_COLUMNS } from "../utils/songColumns";
  import { columnResize } from "../utils/columnResize";

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

  let selectedSongIds = $state<Set<number>>(new Set());
  let lastSelectedSongId = $state<number | null>(null);

  function handleContextMenu(event: MouseEvent, song: Song) {
    event.preventDefault();
    if (!selectedSongIds.has(song.id)) {
      selectedSongIds = new Set([song.id]);
      lastSelectedSongId = song.id;
    }
    contextMenuState = { x: event.clientX, y: event.clientY, song };
  }

  function handleSongClick(e: MouseEvent, song: Song) {
    if (e.shiftKey && lastSelectedSongId !== null) {
      const idx1 = songs.findIndex((s) => s.id === lastSelectedSongId);
      const idx2 = songs.findIndex((s) => s.id === song.id);
      if (idx1 !== -1 && idx2 !== -1) {
        const start = Math.min(idx1, idx2);
        const end = Math.max(idx1, idx2);
        const newSet = new Set(e.ctrlKey || e.metaKey ? selectedSongIds : []);
        for (let i = start; i <= end; i++) {
          newSet.add(songs[i].id);
        }
        selectedSongIds = newSet;
      }
    } else if (e.ctrlKey || e.metaKey) {
      const newSet = new Set(selectedSongIds);
      if (newSet.has(song.id)) {
        newSet.delete(song.id);
      } else {
        newSet.add(song.id);
      }
      selectedSongIds = newSet;
      lastSelectedSongId = song.id;
    } else if (selectedSongIds.size === 1 && selectedSongIds.has(song.id)) {
      selectedSongIds = new Set();
      lastSelectedSongId = null;
    } else {
      selectedSongIds = new Set([song.id]);
      lastSelectedSongId = song.id;
    }
  }

  function handleKeydown(e: KeyboardEvent) {
    if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "a") {
      const target = e.target as HTMLElement;
      if (target && (target.tagName === "INPUT" || target.tagName === "TEXTAREA")) return;
      e.preventDefault();
      selectedSongIds = new Set(songs.map((s) => s.id));
    } else if (e.key === "Escape") {
      selectedSongIds = new Set();
    }
  }

  function handleWindowMouseDown(e: MouseEvent) {
    if (selectedSongIds.size === 0) return;
    const target = e.target as HTMLElement;
    if (!target) return;
    if (
      target.closest("[data-song-row]") ||
      target.closest("[role='menu']") ||
      target.closest("[data-floating-toolbar]") ||
      target.closest("button") ||
      target.closest("input")
    ) {
      return;
    }
    selectedSongIds = new Set();
  }

  async function handleBulkAddToPlaylist() {
    if (selectedSongIds.size === 0) return;
    const songIds = Array.from(selectedSongIds);
    const targetPlaylist = playlistsStore.activeCustomPlaylist;
    const isQueue = !targetPlaylist || targetPlaylist.is_queue;

    if (isQueue) {
      {
        await playlistsStore.addSongsToQueue(songIds);
        const label = songIds.length === 1 ? "1 song" : `${songIds.length} songs`;
        toastStore.show(i18n.t("playlists.addedToQueueSuccess", { name: label }, `Added ${label} to Queue`));
      }
    } else {
      await playlistsStore.addSongsToPlaylist(targetPlaylist.id, songIds);
      toastStore.show(i18n.t("playlists.addedToPlaylistSuccess", { name: targetPlaylist.name }, `Added to ${targetPlaylist.name}`));
    }
  }

  function albumPlayContext(): PlayContext {
    return { type: "album", album: albumName, albumArtist: artistName || undefined };
  }

  function handlePlaySelected() {
    if (selectedSongIds.size === 0) return;
    const selectedList = songs.filter((s) => selectedSongIds.has(s.id));
    if (selectedList.length > 0) {
      playerStore.playSongs(selectedList.map((s) => s.id), 0, undefined, albumPlayContext());
    }
  }

  let albumItem = $derived(
    collectionStore.albums.find((a) => a.album === albumName) || null
  );

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
        url =
          item.art_manual.startsWith("http://") || item.art_manual.startsWith("https://") || item.art_manual.startsWith("/")
            ? item.art_manual
            : getCoverArtUrl(`luminous-art://${item.art_manual}`);
      } else if (item?.art_automatic) {
        if (item.art_automatic.startsWith("http://") || item.art_automatic.startsWith("https://") || item.art_automatic.startsWith("/")) {
          url = item.art_automatic;
        } else if (item.art_automatic.startsWith("album-")) {
          url = getCoverArtUrl(`luminous-art://${item.art_automatic}`);
        } else {
          url = getCoverArtUrl(`luminous-art://local/${item.art_automatic}`);
        }
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

  // The materialized genre auto-playlist row backing this genre, if it's been
  // generated yet (see PlaylistsCollectionView's identical genre/decade split).
  let genrePlaylist = $derived.by(() => {
    if (!rawGenre) return null;
    return (
      playlistsStore.playlists.find(
        (p) => p.dynamic_enabled && !p.dynamic_spec?.startsWith("decade:") && p.dynamic_spec === rawGenre
      ) ?? null
    );
  });

  function openGenrePlaylist() {
    if (!rawGenre) return;
    collectionStore.viewAutoPlaylist({
      kind: "genre",
      genre: rawGenre,
      playlistId: genrePlaylist?.id,
      updated: genrePlaylist?.updated,
    });
  }

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
        // Sort by disc, then by track
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

  function toggleSort(field: AlbumSortField) {
    if (sortField === field) {
      sortAsc = !sortAsc;
    } else {
      sortField = field;
      sortAsc = true;
    }
  }

  let sortedSongs = $derived.by(() => {
    if (sortField === "track") {
      if (sortAsc) return songs;
      return [...songs].reverse();
    }

    const field = sortField as keyof Song;
    return [...songs].sort((a, b) => {
      const valA = a[field];
      const valB = b[field];

      if (valA === undefined || valA === null) return sortAsc ? 1 : -1;
      if (valB === undefined || valB === null) return sortAsc ? -1 : 1;

      if (typeof valA === "string" && typeof valB === "string") {
        const cmp = valA.localeCompare(valB);
        return sortAsc ? cmp : -cmp;
      }
      if (typeof valA === "number" && typeof valB === "number") {
        return sortAsc ? valA - valB : valB - valA;
      }
      return 0;
    });
  });

  // Default column widths (px or fr) — used when no saved width exists for a column.
  const ALBUM_COL_DEFAULTS: Partial<Record<keyof typeof collectionStore.visibleColumns, string>> = {
    track: "48px", title: "2fr", artist: "1.5fr", album: "1.5fr",
    composer: "1.5fr", album_artist: "1.5fr", format: "64px", year: "60px",
    genre: "1.2fr", grouping: "1.2fr", bpm: "60px", initial_key: "60px",
    bitrate: "70px", samplerate: "75px", bitdepth: "65px", channels: "70px",
    filesize: "75px", rating: "96px", playcount: "70px", skipcount: "70px",
    lastplayed: "90px", added: "90px", duration: "80px", path: "2fr", actions: "80px",
  };

  // Mirrors CollectionView/PlaylistView/AutoPlaylistDetailView's identical
  // formula so all four song-table views share one column configuration.
  let gridColsStyle = $derived.by(() => {
    const vc = collectionStore.visibleColumns;
    const cw = collectionStore.columnWidths;
    const cols: string[] = ["36px"]; // play indicator always present
    for (const { key } of SONG_TABLE_COLUMNS) {
      if (!vc[key]) continue;
      const saved = cw[key];
      cols.push(saved !== undefined ? `${saved}px` : (ALBUM_COL_DEFAULTS[key] ?? "80px"));
    }
    return `grid-template-columns: ${cols.join(" ")}`;
  });

  function goBack() {
    collectionStore.selectedAlbumName = null;
    collectionStore.activeSubTab = "albums";
  }

  async function handlePlaySong(song: Song) {
    const list = sortedSongs;
    const index = list.findIndex((s) => s.id === song.id);
    const songIds = list.map((s) => s.id);
    await playerStore.playSongs(songIds, index >= 0 ? index : 0, undefined, albumPlayContext());
  }

  async function handlePlayAll() {
    if (sortedSongs.length === 0) return;
    await playerStore.setShuffleMode("off");
    await playerStore.playSongs(sortedSongs.map((s) => s.id), 0, undefined, albumPlayContext());
  }

  async function handleShufflePlay() {
    if (sortedSongs.length === 0) return;
    const shuffledIds = shuffleArray(sortedSongs.map((s) => s.id));
    await playerStore.setShuffleMode("all");
    await playerStore.playSongs(shuffledIds, 0, undefined, albumPlayContext());
  }

  async function handleAddSongToPlaylist(songId: number) {
    const targetPlaylist = playlistsStore.activeCustomPlaylist;
    const isQueue = !targetPlaylist || targetPlaylist.is_queue;
    const songIds = [songId];

    if (isQueue) {
      {
        await playlistsStore.addSongsToQueue(songIds);
        const songObj = songs.find((s) => s.id === songId);
        const name = songObj?.title || "Song";
        toastStore.show(i18n.t("playlists.addedToQueueSuccess", { name }, `Added ${name} to Queue`));
      }
    } else {
      await playlistsStore.addSongsToPlaylist(targetPlaylist.id, songIds);
      toastStore.show(i18n.t("playlists.addedToPlaylistSuccess", { name: targetPlaylist.name }, `Added to ${targetPlaylist.name}`));
    }
  }

  async function handleAddAlbumToPlaylist() {
    if (songs.length === 0) return;
    const targetPlaylist = playlistsStore.activeCustomPlaylist;
    const isQueue = !targetPlaylist || targetPlaylist.is_queue;
    const songIds = songs.map((s) => s.id);

    if (isQueue) {
      {
        await playlistsStore.addSongsToQueue(songIds);
        const name = albumName || "Album";
        toastStore.show(i18n.t("playlists.addedToQueueSuccess", { name }, `Added ${name} to Queue`));
      }
    } else {
      await playlistsStore.addSongsToPlaylist(targetPlaylist.id, songIds);
      toastStore.show(i18n.t("playlists.addedToPlaylistSuccess", { name: targetPlaylist.name }, `Added to ${targetPlaylist.name}`));
    }
  }

  function openTagEditor(songId: number) {
    editingSongId = songId;
  }

  function openAlbumTagEditor() {
    if (songs.length === 0) return;
    showAlbumTagEditor = true;
  }

  function handleTagEditorSaved() {
    collectionStore.refreshLibrary();
    // Reload songs
    loading = true;
    invoke<Song[]>("get_songs_by_album", { album: albumName })
      .then((fetchedSongs) => {
        let filtered = [...fetchedSongs];
        filtered.sort((a, b) => {
          if (a.disc !== b.disc) {
            return (a.disc ?? 1) - (b.disc ?? 1);
          }
          return (a.track ?? 0) - (b.track ?? 0);
        });
        songs = filtered;
      })
      .catch((err) => console.error(err))
      .finally(() => loading = false);
  }

  function formatDuration(ns: number | undefined): string {
    if (!ns) return "0:00";
    const sec = Math.floor(ns / 1_000_000_000);
    const m = Math.floor(sec / 60);
    const s = sec % 60;
    return `${m}:${s < 10 ? "0" : ""}${s}`;
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

  <!-- Album Hero & Summary Banner Header -->
  <div class="relative z-30 w-full border-b border-brand-border/60 bg-brand-main/60 backdrop-blur-md px-6 pt-6 pb-6">
    <div class="flex items-start justify-between gap-6 relative z-10">
      <!-- Left Title & Summary Metadata -->
      <div class="flex flex-col justify-end gap-1.5 min-w-0 max-w-xl">
        <h1 class="text-3xl sm:text-4xl font-heading font-bold text-brand-text-primary leading-snug truncate py-0.5" title={albumName}>
          {albumName}
        </h1>

        <div class="flex items-center gap-2 text-base font-semibold text-brand-accent-text">
          {#if artistName}
            <LinkButton
              onclick={() => collectionStore.viewArtist(artistName)}
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
            <button
              onclick={openGenrePlaylist}
              class="hover:underline hover:text-brand-accent-text transition-colors"
              title={i18n.t('albumDetail.goToGenrePlaylistTooltip', { genre: rawGenre })}
            >
              {genreLabel}
            </button>
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

        <!-- Control Buttons -->
        <div class="flex items-center gap-3 mt-3 select-none">
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
          <ColumnSelector align="left" iconOnly />
        </div>
      </div>

      <!-- Right: Full Album Cover Art -->
      <div class="relative w-40 h-40 hidden sm:block shrink-0">
        <div class="absolute inset-0 overflow-hidden border border-brand-border/60 shadow-2xl">
          <CoverArt
            songId={undefined}
            artEmbedded={albumItem?.art_embedded}
            artAutomatic={albumItem?.art_automatic}
            artManual={albumItem?.art_manual}
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
    </div>
  </div>

  <!-- Songs Table Section -->
  <div class="relative z-10 px-6 py-6" class:pb-28={!!playerStore.currentSong}>
    <div class="border border-brand-border rounded-lg bg-brand-sidebar/50 backdrop-blur-xl shadow-2xl overflow-hidden">
      <!-- Table Header -->
      <div class="sticky top-0 z-10 flex flex-col rounded-t-lg bg-brand-sidebar/80 backdrop-blur-md border-b border-brand-border text-[10px] text-brand-text-primary uppercase tracking-wider font-semibold select-none">
        <div class="grid items-center py-2.5 px-4" style={gridColsStyle}>
          <div class="text-center w-9"></div>
          {#if collectionStore.visibleColumns.track}
            <div use:columnResize={{ column: "track", onResize: collectionStore.setColumnWidth.bind(collectionStore), onReset: collectionStore.resetColumnWidth.bind(collectionStore) }} class="relative overflow-hidden">
            <SortableHeader
              active={sortField === "track"}
              {sortAsc}
              onclick={() => toggleSort("track")}
              class="text-left hover:text-brand-text-primary transition-colors flex items-center gap-1 font-semibold uppercase tracking-wider min-w-0 w-full"
            >
              {#snippet label(arrow)}<span class="truncate max-w-[calc(100%-1rem)]">{i18n.t('collection.tableHeaderTrack')} {arrow}</span>{/snippet}
            </SortableHeader>
            </div>
          {/if}
          {#if collectionStore.visibleColumns.title}
            <div use:columnResize={{ column: "title", onResize: collectionStore.setColumnWidth.bind(collectionStore), onReset: collectionStore.resetColumnWidth.bind(collectionStore) }} class="relative overflow-hidden">
            <SortableHeader
              active={sortField === "title"}
              {sortAsc}
              onclick={() => toggleSort("title")}
              class="text-left hover:text-brand-text-primary transition-colors flex items-center gap-1 font-semibold uppercase tracking-wider min-w-0 w-full"
            >
              {#snippet label(arrow)}<span class="truncate max-w-[calc(100%-1rem)]">{i18n.t('collection.tableHeaderTitle')} {arrow}</span>{/snippet}
            </SortableHeader>
            </div>
          {/if}
          {#if collectionStore.visibleColumns.artist}
            <div use:columnResize={{ column: "artist", onResize: collectionStore.setColumnWidth.bind(collectionStore), onReset: collectionStore.resetColumnWidth.bind(collectionStore) }} class="relative overflow-hidden">
            <SortableHeader
              active={sortField === "artist"}
              {sortAsc}
              onclick={() => toggleSort("artist")}
              class="text-left hover:text-brand-text-primary transition-colors flex items-center gap-1 font-semibold uppercase tracking-wider min-w-0 w-full"
            >
              {#snippet label(arrow)}<span class="truncate max-w-[calc(100%-1rem)]">{i18n.t('collection.tableHeaderArtist')} {arrow}</span>{/snippet}
            </SortableHeader>
            </div>
          {/if}
          {#if collectionStore.visibleColumns.album}
            <div use:columnResize={{ column: "album", onResize: collectionStore.setColumnWidth.bind(collectionStore), onReset: collectionStore.resetColumnWidth.bind(collectionStore) }} class="relative overflow-hidden">
            <SortableHeader
              active={sortField === "album"}
              {sortAsc}
              onclick={() => toggleSort("album")}
              class="text-left hover:text-brand-text-primary transition-colors flex items-center gap-1 font-semibold uppercase tracking-wider min-w-0 w-full"
            >
              {#snippet label(arrow)}<span class="truncate max-w-[calc(100%-1rem)]">{i18n.t('collection.tableHeaderAlbum')} {arrow}</span>{/snippet}
            </SortableHeader>
            </div>
          {/if}
          {#if collectionStore.visibleColumns.composer}
            <div use:columnResize={{ column: "composer", onResize: collectionStore.setColumnWidth.bind(collectionStore), onReset: collectionStore.resetColumnWidth.bind(collectionStore) }} class="relative overflow-hidden">
            <SortableHeader
              active={sortField === "composer"}
              {sortAsc}
              onclick={() => toggleSort("composer")}
              class="text-left hover:text-brand-text-primary transition-colors flex items-center gap-1 font-semibold uppercase tracking-wider min-w-0 w-full"
            >
              {#snippet label(arrow)}<span class="truncate max-w-[calc(100%-0.5rem)]">{i18n.t('collection.tableHeaderComposer')} {arrow}</span>{/snippet}
            </SortableHeader>
            </div>
          {/if}
          {#if collectionStore.visibleColumns.album_artist}
            <div use:columnResize={{ column: "album_artist", onResize: collectionStore.setColumnWidth.bind(collectionStore), onReset: collectionStore.resetColumnWidth.bind(collectionStore) }} class="relative overflow-hidden">
            <SortableHeader
              active={sortField === "album_artist"}
              {sortAsc}
              onclick={() => toggleSort("album_artist")}
              class="text-left hover:text-brand-text-primary transition-colors flex items-center gap-1 font-semibold uppercase tracking-wider min-w-0 w-full"
            >
              {#snippet label(arrow)}<span class="truncate max-w-[calc(100%-0.5rem)]">{i18n.t('collection.tableHeaderAlbumArtist')} {arrow}</span>{/snippet}
            </SortableHeader>
            </div>
          {/if}
          {#if collectionStore.visibleColumns.format}
            <div use:columnResize={{ column: "format", onResize: collectionStore.setColumnWidth.bind(collectionStore), onReset: collectionStore.resetColumnWidth.bind(collectionStore) }} class="relative overflow-hidden">
            <SortableHeader
              active={sortField === "filetype"}
              {sortAsc}
              onclick={() => toggleSort("filetype")}
              class="text-left hover:text-brand-text-primary transition-colors flex items-center gap-1 font-semibold uppercase tracking-wider min-w-0 w-full"
            >
              {#snippet label(arrow)}<span class="truncate max-w-[calc(100%-0.5rem)]">{i18n.t('collection.tableHeaderFormat')} {arrow}</span>{/snippet}
            </SortableHeader>
            </div>
          {/if}
          {#if collectionStore.visibleColumns.year}
            <div use:columnResize={{ column: "year", onResize: collectionStore.setColumnWidth.bind(collectionStore), onReset: collectionStore.resetColumnWidth.bind(collectionStore) }} class="relative overflow-hidden">
            <SortableHeader
              active={sortField === "year"}
              {sortAsc}
              onclick={() => toggleSort("year")}
              class="text-left hover:text-brand-text-primary transition-colors flex items-center gap-1 font-semibold uppercase tracking-wider min-w-0 w-full"
            >
              {#snippet label(arrow)}<span class="truncate max-w-[calc(100%-0.5rem)]">{i18n.t('collection.tableHeaderYear')} {arrow}</span>{/snippet}
            </SortableHeader>
            </div>
          {/if}
          {#if collectionStore.visibleColumns.genre}
            <div use:columnResize={{ column: "genre", onResize: collectionStore.setColumnWidth.bind(collectionStore), onReset: collectionStore.resetColumnWidth.bind(collectionStore) }} class="relative overflow-hidden">
            <SortableHeader
              active={sortField === "genre"}
              {sortAsc}
              onclick={() => toggleSort("genre")}
              class="text-left hover:text-brand-text-primary transition-colors flex items-center gap-1 font-semibold uppercase tracking-wider min-w-0 w-full"
            >
              {#snippet label(arrow)}<span class="truncate max-w-[calc(100%-0.5rem)]">{i18n.t('collection.tableHeaderGenre')} {arrow}</span>{/snippet}
            </SortableHeader>
            </div>
          {/if}
          {#if collectionStore.visibleColumns.grouping}
            <div use:columnResize={{ column: "grouping", onResize: collectionStore.setColumnWidth.bind(collectionStore), onReset: collectionStore.resetColumnWidth.bind(collectionStore) }} class="relative overflow-hidden">
            <SortableHeader
              active={sortField === "grouping"}
              {sortAsc}
              onclick={() => toggleSort("grouping")}
              class="text-left hover:text-brand-text-primary transition-colors flex items-center gap-1 font-semibold uppercase tracking-wider min-w-0 w-full"
            >
              {#snippet label(arrow)}<span class="truncate max-w-[calc(100%-0.5rem)]">{i18n.t('collection.tableHeaderGrouping')} {arrow}</span>{/snippet}
            </SortableHeader>
            </div>
          {/if}
          {#if collectionStore.visibleColumns.bpm}
            <div use:columnResize={{ column: "bpm", onResize: collectionStore.setColumnWidth.bind(collectionStore), onReset: collectionStore.resetColumnWidth.bind(collectionStore) }} class="relative overflow-hidden">
            <SortableHeader
              active={sortField === "bpm"}
              {sortAsc}
              onclick={() => toggleSort("bpm")}
              class="text-left hover:text-brand-text-primary transition-colors flex items-center gap-1 font-semibold uppercase tracking-wider min-w-0 w-full"
            >
              {#snippet label(arrow)}<span class="truncate max-w-[calc(100%-0.5rem)]">{i18n.t('collection.tableHeaderBpm')} {arrow}</span>{/snippet}
            </SortableHeader>
            </div>
          {/if}
          {#if collectionStore.visibleColumns.initial_key}
            <div use:columnResize={{ column: "initial_key", onResize: collectionStore.setColumnWidth.bind(collectionStore), onReset: collectionStore.resetColumnWidth.bind(collectionStore) }} class="relative overflow-hidden">
            <SortableHeader
              active={sortField === "initial_key"}
              {sortAsc}
              onclick={() => toggleSort("initial_key")}
              class="text-left hover:text-brand-text-primary transition-colors flex items-center gap-1 font-semibold uppercase tracking-wider min-w-0 w-full"
            >
              {#snippet label(arrow)}<span class="truncate max-w-[calc(100%-0.5rem)]">{i18n.t('collection.tableHeaderInitialKey')} {arrow}</span>{/snippet}
            </SortableHeader>
            </div>
          {/if}
          {#if collectionStore.visibleColumns.bitrate}
            <div use:columnResize={{ column: "bitrate", onResize: collectionStore.setColumnWidth.bind(collectionStore), onReset: collectionStore.resetColumnWidth.bind(collectionStore) }} class="relative overflow-hidden">
            <SortableHeader
              active={sortField === "bitrate"}
              {sortAsc}
              onclick={() => toggleSort("bitrate")}
              class="text-left hover:text-brand-text-primary transition-colors flex items-center gap-1 font-semibold uppercase tracking-wider min-w-0 w-full"
            >
              {#snippet label(arrow)}<span class="truncate max-w-[calc(100%-0.5rem)]">{i18n.t('collection.tableHeaderBitrate')} {arrow}</span>{/snippet}
            </SortableHeader>
            </div>
          {/if}
          {#if collectionStore.visibleColumns.samplerate}
            <div use:columnResize={{ column: "samplerate", onResize: collectionStore.setColumnWidth.bind(collectionStore), onReset: collectionStore.resetColumnWidth.bind(collectionStore) }} class="relative overflow-hidden">
            <SortableHeader
              active={sortField === "samplerate"}
              {sortAsc}
              onclick={() => toggleSort("samplerate")}
              class="text-left hover:text-brand-text-primary transition-colors flex items-center gap-1 font-semibold uppercase tracking-wider min-w-0 w-full"
            >
              {#snippet label(arrow)}<span class="truncate max-w-[calc(100%-0.5rem)]">{i18n.t('collection.tableHeaderSampleRate')} {arrow}</span>{/snippet}
            </SortableHeader>
            </div>
          {/if}
          {#if collectionStore.visibleColumns.bitdepth}
            <div use:columnResize={{ column: "bitdepth", onResize: collectionStore.setColumnWidth.bind(collectionStore), onReset: collectionStore.resetColumnWidth.bind(collectionStore) }} class="relative overflow-hidden">
            <SortableHeader
              active={sortField === "bitdepth"}
              {sortAsc}
              onclick={() => toggleSort("bitdepth")}
              class="text-left hover:text-brand-text-primary transition-colors flex items-center gap-1 font-semibold uppercase tracking-wider min-w-0 w-full"
            >
              {#snippet label(arrow)}<span class="truncate max-w-[calc(100%-0.5rem)]">{i18n.t('collection.tableHeaderBitDepth')} {arrow}</span>{/snippet}
            </SortableHeader>
            </div>
          {/if}
          {#if collectionStore.visibleColumns.channels}
            <div use:columnResize={{ column: "channels", onResize: collectionStore.setColumnWidth.bind(collectionStore), onReset: collectionStore.resetColumnWidth.bind(collectionStore) }} class="relative overflow-hidden">
            <SortableHeader
              active={sortField === "channels"}
              {sortAsc}
              onclick={() => toggleSort("channels")}
              class="text-left hover:text-brand-text-primary transition-colors flex items-center gap-1 font-semibold uppercase tracking-wider min-w-0 w-full"
            >
              {#snippet label(arrow)}<span class="truncate max-w-[calc(100%-0.5rem)]">{i18n.t('collection.tableHeaderChannels')} {arrow}</span>{/snippet}
            </SortableHeader>
            </div>
          {/if}
          {#if collectionStore.visibleColumns.filesize}
            <div use:columnResize={{ column: "filesize", onResize: collectionStore.setColumnWidth.bind(collectionStore), onReset: collectionStore.resetColumnWidth.bind(collectionStore) }} class="relative overflow-hidden">
            <SortableHeader
              active={sortField === "filesize"}
              {sortAsc}
              onclick={() => toggleSort("filesize")}
              class="text-left hover:text-brand-text-primary transition-colors flex items-center gap-1 font-semibold uppercase tracking-wider min-w-0 w-full"
            >
              {#snippet label(arrow)}<span class="truncate max-w-[calc(100%-0.5rem)]">{i18n.t('collection.tableHeaderFileSize')} {arrow}</span>{/snippet}
            </SortableHeader>
            </div>
          {/if}
          {#if collectionStore.visibleColumns.rating}
            <div use:columnResize={{ column: "rating", onResize: collectionStore.setColumnWidth.bind(collectionStore), onReset: collectionStore.resetColumnWidth.bind(collectionStore) }} class="relative overflow-hidden">
            <SortableHeader
              active={sortField === "rating"}
              {sortAsc}
              onclick={() => toggleSort("rating")}
              class="flex items-center justify-center hover:text-brand-text-primary transition-colors font-semibold uppercase tracking-wider min-w-0 w-full"
            >
              {#snippet label(arrow)}<span class="truncate">{i18n.t('collection.tableHeaderRating')} {arrow}</span>{/snippet}
            </SortableHeader>
            </div>
          {/if}
          {#if collectionStore.visibleColumns.playcount}
            <div use:columnResize={{ column: "playcount", onResize: collectionStore.setColumnWidth.bind(collectionStore), onReset: collectionStore.resetColumnWidth.bind(collectionStore) }} class="relative overflow-hidden">
            <SortableHeader
              active={sortField === "playcount"}
              {sortAsc}
              onclick={() => toggleSort("playcount")}
              class="text-center hover:text-brand-text-primary transition-colors flex items-center justify-center gap-1 font-semibold uppercase tracking-wider min-w-0 w-full"
            >
              {#snippet label(arrow)}<span class="truncate">{i18n.t('collection.tableHeaderPlays')} {arrow}</span>{/snippet}
            </SortableHeader>
            </div>
          {/if}
          {#if collectionStore.visibleColumns.skipcount}
            <div use:columnResize={{ column: "skipcount", onResize: collectionStore.setColumnWidth.bind(collectionStore), onReset: collectionStore.resetColumnWidth.bind(collectionStore) }} class="relative overflow-hidden">
            <SortableHeader
              active={sortField === "skipcount"}
              {sortAsc}
              onclick={() => toggleSort("skipcount")}
              class="text-center hover:text-brand-text-primary transition-colors flex items-center justify-center gap-1 font-semibold uppercase tracking-wider min-w-0 w-full"
            >
              {#snippet label(arrow)}<span class="truncate">{i18n.t('collection.tableHeaderSkips')} {arrow}</span>{/snippet}
            </SortableHeader>
            </div>
          {/if}
          {#if collectionStore.visibleColumns.lastplayed}
            <div use:columnResize={{ column: "lastplayed", onResize: collectionStore.setColumnWidth.bind(collectionStore), onReset: collectionStore.resetColumnWidth.bind(collectionStore) }} class="relative overflow-hidden">
            <SortableHeader
              active={sortField === "lastplayed"}
              {sortAsc}
              onclick={() => toggleSort("lastplayed")}
              class="text-center hover:text-brand-text-primary transition-colors flex items-center justify-center gap-1 font-semibold uppercase tracking-wider min-w-0 w-full"
            >
              {#snippet label(arrow)}<span class="truncate">{i18n.t('collection.tableHeaderLastPlayed')} {arrow}</span>{/snippet}
            </SortableHeader>
            </div>
          {/if}
          {#if collectionStore.visibleColumns.added}
            <div use:columnResize={{ column: "added", onResize: collectionStore.setColumnWidth.bind(collectionStore), onReset: collectionStore.resetColumnWidth.bind(collectionStore) }} class="relative overflow-hidden">
            <SortableHeader
              active={sortField === "added"}
              {sortAsc}
              onclick={() => toggleSort("added")}
              class="text-center hover:text-brand-text-primary transition-colors flex items-center justify-center gap-1 font-semibold uppercase tracking-wider min-w-0 w-full"
            >
              {#snippet label(arrow)}<span class="truncate">{i18n.t('collection.tableHeaderAdded')} {arrow}</span>{/snippet}
            </SortableHeader>
            </div>
          {/if}
          {#if collectionStore.visibleColumns.duration}
            <div use:columnResize={{ column: "duration", onResize: collectionStore.setColumnWidth.bind(collectionStore), onReset: collectionStore.resetColumnWidth.bind(collectionStore) }} class="relative overflow-hidden">
            <SortableHeader
              active={sortField === "length_nanosec"}
              {sortAsc}
              onclick={() => toggleSort("length_nanosec")}
              class="flex items-center justify-center hover:text-brand-text-primary transition-colors font-semibold uppercase tracking-wider min-w-0 w-full"
            >
              {#snippet label(arrow)}<Clock class="w-3.5 h-3.5 shrink-0" /> {arrow}{/snippet}
            </SortableHeader>
            </div>
          {/if}
          {#if collectionStore.visibleColumns.path}
            <div use:columnResize={{ column: "path", onResize: collectionStore.setColumnWidth.bind(collectionStore), onReset: collectionStore.resetColumnWidth.bind(collectionStore) }} class="relative overflow-hidden">
            <SortableHeader
              active={sortField === "path"}
              {sortAsc}
              onclick={() => toggleSort("path")}
              class="text-left hover:text-brand-text-primary transition-colors flex items-center gap-1 font-semibold uppercase tracking-wider min-w-0 w-full"
            >
              {#snippet label(arrow)}<span class="truncate max-w-[calc(100%-0.5rem)]">{i18n.t('collection.tableHeaderPath')} {arrow}</span>{/snippet}
            </SortableHeader>
            </div>
          {/if}
          {#if collectionStore.visibleColumns.actions}
            <div use:columnResize={{ column: "actions", onResize: collectionStore.setColumnWidth.bind(collectionStore), onReset: collectionStore.resetColumnWidth.bind(collectionStore) }} class="relative overflow-hidden text-center">{i18n.t('collection.tableHeaderActions')}</div>
          {/if}
        </div>
      </div>

      <!-- Table Body -->
      <div class="divide-y divide-brand-border/40 rounded-b-lg overflow-hidden">
        {#if loading}
          <div class="flex items-center justify-center py-16">
            <div class="text-brand-text-primary text-sm">{i18n.t('home.loading')}</div>
          </div>
        {:else if sortedSongs.length === 0}
          <div class="py-16 text-center select-none">
            <EmptyState icon={Music} title={i18n.t('collection.noSongsTitle')} />
          </div>
        {:else}
          {#each sortedSongs as song, index (song.id)}
            {@const disconnected = !song.unavailable && collectionStore.isPathOnDisconnectedDrive(song.path)}
            {@const disabled = song.unavailable || disconnected}
            <!-- svelte-ignore a11y_no_static_element_interactions -->
            <!-- svelte-ignore a11y_click_events_have_key_events -->
            <div
              data-song-row="true"
              onclick={(e) => !disabled && handleSongClick(e, song)}
              ondblclick={() => !disabled && handlePlaySong(song)}
              oncontextmenu={(e) => !disabled && handleContextMenu(e, song)}
              style={gridColsStyle}
              title={disconnected ? i18n.t('collection.driveDisconnectedTooltip') : undefined}
              class="grid items-center hover:bg-brand-sidebar/40 group transition-colors py-2 px-4 text-sm
                {disabled ? 'opacity-50 cursor-not-allowed' : ''}
                {selectedSongIds.has(song.id) ? 'bg-brand-accent/20 border-l-2 border-brand-accent text-brand-accent-text-hover' : (playerStore.currentSong && playerStore.currentSong.id === song.id ? 'bg-brand-accent/10 text-brand-accent-text-hover' : '')}"
            >
              <div class="text-center flex justify-center relative w-9 h-6 items-center">
                {#if playerStore.currentSong && playerStore.currentSong.id === song.id && playerStore.state === 'playing'}
                  <div class="flex items-center justify-center gap-0.5 h-3.5 w-3.5 absolute group-hover:opacity-0 transition-opacity">
                    <NowPlayingBars />
                  </div>
                {/if}
                <button
                  onclick={(e) => { e.stopPropagation(); if (!disabled) handlePlaySong(song); }}
                  class="absolute flex items-center justify-center opacity-0 group-hover:opacity-100 text-brand-accent-text hover:text-brand-accent-text-hover transition-all duration-150 disabled:opacity-0 disabled:cursor-not-allowed"
                  disabled={disabled}
                  title={disconnected ? i18n.t('collection.driveDisconnectedTooltip') : i18n.t('collection.playSong')}
                >
                  <Play class="w-3.5 h-3.5 fill-current" />
                </button>
              </div>

              {#if collectionStore.visibleColumns.track}
                <div class="text-brand-text-primary truncate pr-2 min-w-0 text-xs font-medium">
                  {formatTrackNumber(song.track, song.disc, discCount, index)}
                </div>
              {/if}

              {#if collectionStore.visibleColumns.title}
                <div class="font-medium truncate pr-4 flex items-center gap-2 min-w-0">
                  <span class="truncate {playerStore.currentSong && playerStore.currentSong.id === song.id ? 'text-brand-accent-text-hover' : 'text-brand-text-primary'}">
                    {song.title || i18n.t('collection.unknownSong')}
                  </span>
                </div>
              {/if}

              {#if collectionStore.visibleColumns.artist}
                <div class="text-brand-text-primary truncate pr-4 flex items-center min-w-0">
                  {#if song.artist}
                    <LinkButton
                      onclick={(e) => { e.stopPropagation(); collectionStore.viewArtist(song.album_artist?.trim() || song.artist || ""); }}
                      class="text-brand-text-primary truncate min-w-0"
                      title={i18n.t('collection.filterByArtist', { artist: song.artist })}
                    >
                      {song.artist}
                    </LinkButton>
                  {:else}
                    <span class="text-brand-text-primary truncate min-w-0">{i18n.t('collection.unknownArtist')}</span>
                  {/if}
                </div>
              {/if}

              {#if collectionStore.visibleColumns.album}
                <div class="text-brand-text-primary truncate pr-4 min-w-0 text-xs font-medium">
                  {song.album || i18n.t('collection.unknownAlbum')}
                </div>
              {/if}

              {#if collectionStore.visibleColumns.composer}
                <div class="text-brand-text-primary truncate pr-2 min-w-0 text-xs font-medium" title={song.composer}>
                  {song.composer || "—"}
                </div>
              {/if}
              {#if collectionStore.visibleColumns.album_artist}
                <div class="text-brand-text-primary truncate pr-2 min-w-0 text-xs font-medium" title={song.album_artist}>
                  {song.album_artist || "—"}
                </div>
              {/if}
              {#if collectionStore.visibleColumns.format}
                <div class="text-brand-text-primary truncate pr-2 min-w-0 text-xs font-semibold uppercase">
                  {song.filetype ? song.filetype.toUpperCase() : "—"}
                </div>
              {/if}
              {#if collectionStore.visibleColumns.year}
                <div class="text-brand-text-primary truncate pr-2 min-w-0 text-xs font-medium">
                  {song.year || "—"}
                </div>
              {/if}
              {#if collectionStore.visibleColumns.genre}
                <div class="text-brand-text-primary truncate pr-2 min-w-0 text-xs font-medium" title={song.genre}>
                  {song.genre || "—"}
                </div>
              {/if}
              {#if collectionStore.visibleColumns.grouping}
                <div class="text-brand-text-primary truncate pr-2 min-w-0 text-xs font-medium" title={song.grouping}>
                  {song.grouping || "—"}
                </div>
              {/if}
              {#if collectionStore.visibleColumns.bpm}
                <div class="text-brand-text-primary truncate pr-2 min-w-0 text-xs font-mono">
                  {song.bpm || "—"}
                </div>
              {/if}
              {#if collectionStore.visibleColumns.initial_key}
                <div class="text-brand-text-primary truncate pr-2 min-w-0 text-xs font-mono">
                  {song.initial_key || "—"}
                </div>
              {/if}
              {#if collectionStore.visibleColumns.bitrate}
                <div class="text-brand-text-primary truncate pr-2 min-w-0 text-xs font-mono">
                  {song.bitrate ? `${song.bitrate}k` : "—"}
                </div>
              {/if}
              {#if collectionStore.visibleColumns.samplerate}
                <div class="text-brand-text-primary truncate pr-2 min-w-0 text-xs font-mono">
                  {formatSampleRate(song.samplerate)}
                </div>
              {/if}
              {#if collectionStore.visibleColumns.bitdepth}
                <div class="text-brand-text-primary truncate pr-2 min-w-0 text-xs font-mono">
                  {formatBitDepth(song.bitdepth)}
                </div>
              {/if}
              {#if collectionStore.visibleColumns.channels}
                <div class="text-brand-text-primary truncate pr-2 min-w-0 text-xs font-medium">
                  {formatChannels(song.channels)}
                </div>
              {/if}
              {#if collectionStore.visibleColumns.filesize}
                <div class="text-brand-text-primary truncate pr-2 min-w-0 text-xs font-mono">
                  {formatFileSize(song.filesize)}
                </div>
              {/if}

              {#if collectionStore.visibleColumns.rating}
                <div class="flex justify-center">
                  <SongRating rating={song.rating} onRate={(r) => rateSong(song, r)} />
                </div>
              {/if}

              {#if collectionStore.visibleColumns.playcount}
                <div class="text-center text-brand-text-primary font-medium">
                  {song.playcount ?? 0}
                </div>
              {/if}
              {#if collectionStore.visibleColumns.skipcount}
                <div class="text-center text-brand-text-primary font-mono text-xs">
                  {song.skipcount ?? 0}
                </div>
              {/if}
              {#if collectionStore.visibleColumns.lastplayed}
                <div class="text-center text-brand-text-primary text-xs whitespace-nowrap">
                  {formatDate(song.lastplayed)}
                </div>
              {/if}
              {#if collectionStore.visibleColumns.added}
                <div class="text-center text-brand-text-primary text-xs whitespace-nowrap">
                  {formatDateAdded(song.added)}
                </div>
              {/if}

              {#if collectionStore.visibleColumns.duration}
                <div class="text-center text-brand-text-primary text-xs font-medium">
                  {formatDuration(song.length_nanosec)}
                </div>
              {/if}

              {#if collectionStore.visibleColumns.path}
                <div class="text-brand-text-primary truncate pr-4 min-w-0 text-xs font-mono" title={song.path}>
                  {song.path || "—"}
                </div>
              {/if}

              {#if collectionStore.visibleColumns.actions}
                <div class="flex items-center justify-center gap-2.5">
                  <button
                    onclick={() => handleAddSongToPlaylist(song.id)}
                    class="text-brand-text-primary hover:text-brand-accent-text transition-colors"
                    title={playlistsStore.activeCustomPlaylist
                      ? i18n.t('collection.addPlaylistTooltip', { name: playlistsStore.activeCustomPlaylist.name })
                      : i18n.t('collection.addPlaylistTooltipDefault')}
                  >
                    <Plus class="w-4 h-4" />
                  </button>
                  <button
                    onclick={() => openTagEditor(song.id)}
                    class="text-brand-text-primary hover:text-brand-accent-text transition-colors"
                    title={i18n.t('collection.editTagsTooltip')}
                  >
                    <Edit3 class="w-4 h-4" />
                  </button>
                </div>
              {/if}
            </div>
          {/each}
        {/if}
      </div>
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
    initialAlbumArtist={songs[0].album_artist || songs[0].artist}
    initialGenre={songs[0].genre}
    initialYear={songs[0].year}
    initialDisc={songs[0].disc}
    onClose={() => { showAlbumTagEditor = false; }}
    onSave={handleTagEditorSaved}
  />
{/if}

<svelte:window onkeydown={handleKeydown} onmousedown={handleWindowMouseDown} />

{#if contextMenuState}
  {@const song = contextMenuState.song}
  <SongContextMenu
    x={contextMenuState.x}
    y={contextMenuState.y}
    {song}
    selectedCount={selectedSongIds.size}
    selectedSongIds={Array.from(selectedSongIds)}
    onPlay={() => {
      if (selectedSongIds.size > 1) {
        handlePlaySelected();
      } else {
        handlePlaySong(song);
      }
    }}
    onAddToPlaylist={() => {
      if (selectedSongIds.size > 1) {
        handleBulkAddToPlaylist();
      } else {
        handleAddSongToPlaylist(song.id);
      }
    }}
    onGoToArtist={() => collectionStore.viewArtist(song.album_artist?.trim() || song.artist || "")}
    onGoToAlbum={() => collectionStore.viewAlbum(song.album || "")}
    onEditTags={() => openTagEditor(song.id)}
    onClose={() => { contextMenuState = null; }}
  />
{/if}

{#if selectedSongIds.size > 0}
  <SongSelectionToolbar
    count={selectedSongIds.size}
    onPlaySelected={handlePlaySelected}
    onAddToPlaylist={handleBulkAddToPlaylist}
    onClear={() => { selectedSongIds = new Set(); }}
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

