<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { collectionStore } from "../stores/collection.svelte";
  import { playerStore } from "../stores/player.svelte";
  import { playlistsStore } from "../stores/playlists.svelte";
  import CoverArt from "./CoverArt.svelte";
  import SongRating from "./SongRating.svelte";
  import TagEditor from "./TagEditor.svelte";
  import { Play, Plus, Clock, FileText, Music, DiscAlbum, Mic2, Edit3, Columns, LayoutGrid, Rows3 } from "lucide-svelte";
  import type { Song, AlbumItem, ArtistItem } from "../types";
  import { i18n } from "../stores/i18n.svelte";
  import { toastStore } from "../stores/toast.svelte";
  import { prefs, type CollectionViewMode } from "../stores/prefs.svelte";
  import { VirtualList } from "svelte-virtual-list-ts";
  import { getArtistAlbums, getArtistSongs, getArtistGradient } from "../utils/artist";
  import ArtistDetailView from "./ArtistDetailView.svelte";
  import AlbumDetailView from "./AlbumDetailView.svelte";
  import SongContextMenu from "./SongContextMenu.svelte";
  import AlbumContextMenu from "./AlbumContextMenu.svelte";
  import AlbumCard from "./AlbumCard.svelte";
  import ArtistCard from "./ArtistCard.svelte";
  import AlbumRowCard from "./AlbumRowCard.svelte";
  import ArtistRowCard from "./ArtistRowCard.svelte";
  import SortableHeader from "./SortableHeader.svelte";
  import Select from "./Select.svelte";
  import NowPlayingBars from "./NowPlayingBars.svelte";
  import LinkButton from "./LinkButton.svelte";
  import ColumnSelector from "./ColumnSelector.svelte";
  import LibraryWelcome from "./LibraryWelcome.svelte";
  import SearchEmptyState from "./SearchEmptyState.svelte";
  import { formatDate, formatFileSize, formatSampleRate, formatBitDepth, formatChannels } from "../utils/formatters";
  import { formatDateAdded } from "../utils/date";
  import { SONG_TABLE_COLUMNS } from "../utils/songColumns";
  import { rememberScroll, watchScrollMemory } from "../utils/scrollMemory";
  import { columnResize } from "../utils/columnResize";

  // activeSubTab and activeTab are managed globally via collectionStore

  let editingSongId = $state<number | null>(null);
  let showColumnsMenu = $state(false);
  let contextMenuState = $state<{ x: number; y: number; song: Song } | null>(null);
  let albumContextMenuState = $state<{ x: number; y: number; album: AlbumItem } | null>(null);

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

  function handleAlbumContextMenu(event: MouseEvent, album: AlbumItem) {
    event.preventDefault();
    albumContextMenuState = { x: event.clientX, y: event.clientY, album };
  }

  function handleSongClick(e: MouseEvent, song: Song) {
    if (e.shiftKey && lastSelectedSongId !== null) {
      const idx1 = filteredSongs.findIndex((s) => s.id === lastSelectedSongId);
      const idx2 = filteredSongs.findIndex((s) => s.id === song.id);
      if (idx1 !== -1 && idx2 !== -1) {
        const start = Math.min(idx1, idx2);
        const end = Math.max(idx1, idx2);
        const newSet = new Set(e.ctrlKey || e.metaKey ? selectedSongIds : []);
        for (let i = start; i <= end; i++) {
          newSet.add(filteredSongs[i].id);
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
    if (collectionStore.activeTab !== "collection" || collectionStore.activeSubTab !== "songs") return;

    if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "a") {
      const target = e.target as HTMLElement;
      if (target && (target.tagName === "INPUT" || target.tagName === "TEXTAREA")) return;
      e.preventDefault();
      selectedSongIds = new Set(filteredSongs.map((s) => s.id));
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
    if (playlistsStore.activeCustomPlaylist) {
      await playlistsStore.addSongsToPlaylist(playlistsStore.activeCustomPlaylist.id, Array.from(selectedSongIds));
    } else {
      toastStore.show(i18n.t("collection.selectPlaylistFirstAlert"));
    }
  }

  function handlePlaySelected() {
    if (selectedSongIds.size === 0) return;
    const selectedList = filteredSongs.filter((s) => selectedSongIds.has(s.id));
    if (selectedList.length > 0) {
      playerStore.playSongs(selectedList.map((s) => s.id), 0);
    }
  }

  function openTagEditor(songId: number) {
    editingSongId = songId;
  }

  function handleTagEditorSaved() {
    collectionStore.refreshLibrary();
  }

  function getArtistAlbumsFor(name: string | null): AlbumItem[] {
    return getArtistAlbums(collectionStore.albums, name);
  }

  function getArtistSongsFor(name: string | null): Song[] {
    return getArtistSongs(collectionStore.songs, name);
  }

  // Albums and Artists each remember their own Cards/Rows view mode.
  let activeViewMode = $derived(
    collectionStore.activeSubTab === "albums" ? prefs.albumsViewMode : prefs.artistsViewMode
  );

  function setActiveViewMode(mode: CollectionViewMode) {
    if (collectionStore.activeSubTab === "albums") {
      prefs.setAlbumsViewMode(mode);
    } else {
      prefs.setArtistsViewMode(mode);
    }
  }

  let sortField = $state<keyof Song>(
    (typeof window !== "undefined" && localStorage.getItem("sort_song_field") as keyof Song) || "title"
  );
  let sortAsc = $state(
    typeof window !== "undefined" ? localStorage.getItem("sort_song_asc") !== "false" : true
  );

  // Sort dropdown options: only sortable columns currently visible via ColumnSelector.
  let sortableColumns = $derived(
    SONG_TABLE_COLUMNS.filter((col) => col.field && collectionStore.visibleColumns[col.key])
  );

  $effect(() => {
    collectionStore.search(collectionStore.searchQuery);
  });

  let filteredSongs = $derived.by(() => {
    let result = collectionStore.filteredSongs;

    return [...result].sort((a, b) => {
      let valA = a[sortField];
      let valB = b[sortField];

      // Missing tags come back as `null` — but some tags (composer especially)
      // are commonly present-but-blank in files rather than absent entirely, which
      // reads as an empty string, not null. Treat both as "no value" so they always
      // sort to the end, regardless of direction, instead of an empty string sorting
      // first (it's lexicographically smaller than everything) or a real `null`
      // flipping to the top when descending.
      const isBlank = (v: unknown) => v == null || (typeof v === "string" && v.trim() === "");
      const blankA = isBlank(valA);
      const blankB = isBlank(valB);
      if (blankA && blankB) return 0;
      if (blankA) return 1;
      if (blankB) return -1;

      if (typeof valA === "string" && typeof valB === "string") {
        return sortAsc
          ? valA.localeCompare(valB)
          : valB.localeCompare(valA);
      }

      if (typeof valA === "number" && typeof valB === "number") {
        return sortAsc ? valA - valB : valB - valA;
      }

      return 0;
    });
  });

  let albumSortField = $state<"album" | "artist" | "year" | "rating" | "added">(
    (() => {
      if (typeof window === "undefined") return "album";
      const saved = localStorage.getItem("sort_album_field");
      if (saved === "track_count") return "album";
      return (saved as "album" | "artist" | "year" | "rating" | "added") || "album";
    })()
  );
  let albumSortAsc = $state(
    typeof window !== "undefined" ? localStorage.getItem("sort_album_asc") !== "false" : true
  );

  let artistSortField = $state<"name" | "genre" | "song_count">(
    (() => {
      if (typeof window === "undefined") return "name";
      const saved = localStorage.getItem("sort_artist_field");
      if (saved === "album_count") return "genre";
      return (saved as "name" | "genre" | "song_count") || "name";
    })()
  );
  let artistSortAsc = $state(
    typeof window !== "undefined" ? localStorage.getItem("sort_artist_asc") !== "false" : true
  );

  $effect(() => {
    if (typeof window !== "undefined") {
      localStorage.setItem("sort_song_field", sortField);
      localStorage.setItem("sort_song_asc", sortAsc.toString());
      localStorage.setItem("sort_album_field", albumSortField);
      localStorage.setItem("sort_album_asc", albumSortAsc.toString());
      localStorage.setItem("sort_artist_field", artistSortField);
      localStorage.setItem("sort_artist_asc", artistSortAsc.toString());
    }
  });

  let sortedAlbums = $derived.by(() => {
    const list = [...collectionStore.filteredAlbums];
    const field = albumSortField;
    const asc = albumSortAsc;

    return list.sort((a, b) => {
      let valA = a[field];
      let valB = b[field];

      if (valA === null || valA === undefined) return asc ? 1 : -1;
      if (valB === null || valB === undefined) return asc ? -1 : 1;

      if (typeof valA === "string" && typeof valB === "string") {
        return asc ? valA.localeCompare(valB) : valB.localeCompare(valA);
      }
      if (typeof valA === "number" && typeof valB === "number") {
        return asc ? valA - valB : valB - valA;
      }
      return 0;
    });
  });

  let sortedArtists = $derived.by(() => {
    const list = [...collectionStore.filteredArtists];
    const field = artistSortField;
    const asc = artistSortAsc;

    return list.sort((a, b) => {
      let valA = field === "genre" ? (a.genre?.trim() || i18n.t('artistDetail.unknownGenre')) : a[field];
      let valB = field === "genre" ? (b.genre?.trim() || i18n.t('artistDetail.unknownGenre')) : b[field];

      if (valA === null || valA === undefined) return asc ? 1 : -1;
      if (valB === null || valB === undefined) return asc ? -1 : 1;

      if (typeof valA === "string" && typeof valB === "string") {
        return asc ? valA.localeCompare(valB) : valB.localeCompare(valA);
      }
      if (typeof valA === "number" && typeof valB === "number") {
        return asc ? valA - valB : valB - valA;
      }
      return 0;
    });
  });

  // Default column widths (px or fr) — used when no saved width exists for a column.
  const COLLECTION_COL_DEFAULTS: Partial<Record<keyof typeof collectionStore.visibleColumns, string>> = {
    track: "48px", title: "2fr", artist: "1.5fr", album: "1.5fr",
    composer: "1.5fr", album_artist: "1.5fr", format: "64px", year: "60px",
    genre: "1.2fr", grouping: "1.2fr", bpm: "60px", initial_key: "60px",
    bitrate: "70px", samplerate: "75px", bitdepth: "65px", channels: "70px",
    filesize: "75px", rating: "96px", playcount: "70px", skipcount: "70px",
    lastplayed: "90px", added: "90px", duration: "80px", path: "2fr", actions: "80px",
  };

  let gridColsStyle = $derived.by(() => {
    const vc = collectionStore.visibleColumns;
    const cw = collectionStore.columnWidths;
    const cols: string[] = ["36px"]; // play indicator always present
    for (const { key } of SONG_TABLE_COLUMNS) {
      if (!vc[key]) continue;
      const saved = cw[key];
      cols.push(saved !== undefined ? `${saved}px` : (COLLECTION_COL_DEFAULTS[key] ?? "80px"));
    }
    return `grid-template-columns: ${cols.join(" ")}`;
  });

  // The song rows live inside <VirtualList>'s scrolling viewport, which
  // reserves layout width for its own vertical scrollbar; the sticky header
  // sits outside that viewport and doesn't. On platforms with a non-overlay
  // scrollbar (e.g. Windows), that gutter makes the header's grid track a
  // few pixels wider than the rows' grid track, misaligning every column
  // after the ones that can absorb it. Measure the actual scrollbar width
  // and pad the header by the same amount so both grids compute identical
  // track widths.
  let songsTableContainer = $state<HTMLDivElement | undefined>(undefined);
  let scrollbarWidth = $state(0);

  $effect(() => {
    if (!songsTableContainer) return;
    const viewport = songsTableContainer.querySelector<HTMLElement>("svelte-virtual-list-viewport");
    if (!viewport) return;
    const update = () => {
      scrollbarWidth = viewport.offsetWidth - viewport.clientWidth;
    };
    update();
    const observer = new ResizeObserver(update);
    observer.observe(viewport);
    return () => observer.disconnect();
  });

  // VirtualList renders its own <svelte-virtual-list-viewport> scrolling element
  // inside songsTableContainer, so it can't be reached with a template `use:` ref.
  $effect(() => {
    if (!songsTableContainer) return;
    const viewport = songsTableContainer.querySelector<HTMLElement>("svelte-virtual-list-viewport");
    if (!viewport) return;
    return watchScrollMemory(viewport, "collection:songs");
  });

  function toggleSort(field: keyof Song) {
    if (sortField === field) {
      sortAsc = !sortAsc;
    } else {
      sortField = field;
      sortAsc = true;
    }
  }

  function handlePlaySong(song: Song) {
    playerStore.playSong(song.id);
  }

  async function handlePlayAlbum(albumName: string) {
    let songs = await invoke<Song[]>("get_songs_by_album", {
      album: albumName,
    });
    if (songs.length > 0) {
      const songIds = songs.map((s) => s.id);
      playerStore.playSongs(songIds, 0);
    }
  }

  function formatDuration(ns: number | undefined): string {
    if (!ns) return "0:00";
    const sec = Math.floor(ns / 1_000_000_000);
    const m = Math.floor(sec / 60);
    const s = sec % 60;
    return `${m}:${s < 10 ? "0" : ""}${s}`;
  }

  async function handleAddSongToPlaylist(songId: number) {
    const targetPlaylist = playlistsStore.activeCustomPlaylist;
    const isQueue = !targetPlaylist || targetPlaylist.is_queue;
    const songIds = [songId];

    if (isQueue) {
      {
        await playlistsStore.addSongsToQueue(songIds);
        const songObj = collectionStore.songs.find((s) => s.id === songId);
        const name = songObj?.title || "Song";
        toastStore.show(i18n.t("playlists.addedToQueueSuccess", { name }, `Added ${name} to Queue`));
      }
    } else {
      await playlistsStore.addSongsToPlaylist(targetPlaylist.id, songIds);
      toastStore.show(i18n.t("playlists.addedToPlaylistSuccess", { name: targetPlaylist.name }, `Added to ${targetPlaylist.name}`));
    }
  }

  async function rateSong(song: Song, rating: number) {
    song.rating = await invoke<number>("set_song_rating", { songId: song.id, rating });
  }
</script>

<svelte:window onkeydown={handleKeydown} onmousedown={handleWindowMouseDown} />

{#if collectionStore.selectedAlbumName !== null}
  <AlbumDetailView albumName={collectionStore.selectedAlbumName} />
{:else if collectionStore.selectedArtistName !== null}
  <ArtistDetailView artistName={collectionStore.selectedArtistName} />
{:else}
<div class="flex-1 flex flex-col overflow-hidden bg-brand-main text-brand-text-secondary h-full">
  {#if collectionStore.activeSubTab === "songs"}
    <div class="px-6 pt-4 pb-2 flex-shrink-0">
      <div class="h-9 flex items-center justify-between">
        <div class="text-xs text-brand-text-secondary font-medium">
          {filteredSongs.length === 1 ? i18n.t('collection.showingOneSong') : i18n.t('collection.showingSongs', { count: filteredSongs.length })}
        </div>

        <div class="flex items-center gap-2">
          <ColumnSelector align="right" iconOnly />
          <div class="relative">
            <Select
              value={`${sortField}-${sortAsc}`}
              onchange={(e) => {
                const [field, asc] = e.currentTarget.value.split("-");
                sortField = field as keyof Song;
                sortAsc = asc === "true";
              }}
              class="bg-brand-sidebar border border-brand-border hover:border-brand-accent/60 text-brand-text-secondary text-xs rounded-full pl-2.5 pr-8 py-1.5 focus:outline-none focus:border-brand-accent transition-all font-medium"
            >
              {#each sortableColumns as col (col.key)}
                <option value="{col.field}-true">▲ {i18n.t(col.label)}</option>
                <option value="{col.field}-false">▼ {i18n.t(col.label)}</option>
              {/each}
            </Select>
          </div>
        </div>
      </div>
    </div>

    <div class="flex-1 px-6 pt-2 overflow-hidden flex flex-col {playerStore.currentSong ? 'pb-28' : 'pb-6'}">
      <div bind:this={songsTableContainer} class="flex-1 overflow-hidden border border-brand-border rounded-lg bg-brand-sidebar/40 flex flex-col min-h-0">
        <div class="sticky top-0 z-20 flex flex-col bg-brand-sidebar border-b border-brand-border text-xs text-brand-text-secondary uppercase tracking-wider font-semibold select-none">
          <div class="grid items-center py-3 px-4" style="{gridColsStyle}; padding-right: calc(1rem + {scrollbarWidth}px)">
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
                {#snippet label(arrow)}<Clock class="w-4 h-4 shrink-0" /> {arrow}{/snippet}
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

        <div class="flex-1 min-h-0 relative">
          {#if filteredSongs.length === 0 && collectionStore.searchQuery}
            <div class="py-16 text-center">
              <SearchEmptyState
                icon={Music}
                title={i18n.t('collection.noSongsTitle')}
                matchQueryText={i18n.t('collection.noTracksMatchQuery')}
                query={collectionStore.searchQuery}
                onReset={() => { collectionStore.searchQuery = ""; collectionStore.search(""); }}
              />
            </div>
          {:else if filteredSongs.length === 0}
            <!-- Library has no songs at all yet (no watched folders) — a distinct
                 welcome moment, not a "your search/filters found nothing" state. -->
            <div class="py-16 text-center">
              <LibraryWelcome />
            </div>
          {:else}
            {#key gridColsStyle}
            <VirtualList items={filteredSongs} itemHeight={41}>
              {#snippet children({ item: song }: { item: Song })}
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
                class="grid items-center border-b border-brand-border/40 hover:bg-brand-sidebar/40 group transition-colors py-2.5 px-4 text-sm
                  {disabled ? 'opacity-50 cursor-not-allowed' : ''}
                  {selectedSongIds.has(song.id) ? 'bg-brand-accent/20 border-l-2 border-brand-accent text-brand-accent-text-hover' : (playerStore.currentSong && playerStore.currentSong.id === song.id ? 'bg-brand-accent/10 text-brand-accent-text-hover' : '')}"
              >
                <div class="text-center flex justify-center relative w-9 h-6 items-center">
                  {#if playerStore.currentSong && playerStore.currentSong.id === song.id && playerStore.state === 'playing'}
                    <div class="flex items-center justify-center gap-0.5 h-4 w-4 absolute group-hover:opacity-0 transition-opacity">
                      <NowPlayingBars />
                    </div>
                  {/if}
                  <button
                    onclick={() => !disabled && handlePlaySong(song)}
                    class="absolute flex items-center justify-center opacity-0 group-hover:opacity-100 text-brand-accent-text hover:text-brand-accent-text-hover transition-all duration-150 disabled:opacity-0 disabled:cursor-not-allowed"
                    disabled={disabled}
                    title={disconnected ? i18n.t('collection.driveDisconnectedTooltip') : i18n.t('collection.playSong')}
                  >
                    <Play class="w-4 h-4 fill-current" />
                  </button>
                </div>
                {#if collectionStore.visibleColumns.track}
                  <div class="text-brand-text-secondary truncate pr-2 min-w-0 text-xs font-medium">
                    {song.track !== undefined && song.track !== null ? song.track : "—"}
                  </div>
                {/if}
                {#if collectionStore.visibleColumns.title}
                  <div class="font-medium truncate pr-4 flex items-center gap-2 min-w-0">
                    <CoverArt
                      songId={song.id}
                      artEmbedded={song.art_embedded}
                      artAutomatic={song.art_automatic}
                      artManual={song.art_manual}
                      sizeClass="w-7 h-7 rounded shrink-0"
                    />
                    <span
                      class="truncate min-w-0 font-medium {playerStore.currentSong && playerStore.currentSong.id === song.id ? 'text-brand-accent-text-hover' : 'text-brand-text-primary'}"
                      title={song.title || i18n.t('collection.unknownSong')}
                    >
                      {song.title || i18n.t('collection.unknownSong')}
                    </span>
                  </div>
                {/if}
                {#if collectionStore.visibleColumns.artist}
                  <div class="text-brand-text-secondary truncate pr-4 flex items-center min-w-0">
                    {#if song.artist}
                      <LinkButton
                        onclick={(e) => { e.stopPropagation(); collectionStore.viewArtist(song.album_artist?.trim() || song.artist || ""); }}
                        class="text-brand-text-secondary truncate min-w-0"
                        title={i18n.t('collection.filterByArtist', { artist: song.artist })}
                      >
                        {song.artist}
                      </LinkButton>
                    {:else}
                      <span class="text-brand-text-secondary truncate min-w-0">{i18n.t('collection.unknownArtist')}</span>
                    {/if}
                  </div>
                {/if}
                {#if collectionStore.visibleColumns.album}
                  <div class="text-brand-text-secondary truncate pr-4 flex items-center min-w-0">
                    {#if song.album}
                      <LinkButton
                        onclick={(e) => { e.stopPropagation(); collectionStore.viewAlbum(song.album || ""); }}
                        class="text-brand-text-secondary truncate min-w-0"
                        title={i18n.t('collection.filterByAlbum', { album: song.album })}
                      >
                        {song.album}
                      </LinkButton>
                    {:else}
                      <span class="text-brand-text-secondary truncate min-w-0">{i18n.t('collection.unknownAlbum')}</span>
                    {/if}
                  </div>
                {/if}
                {#if collectionStore.visibleColumns.composer}
                  <div class="text-brand-text-secondary truncate pr-4 min-w-0 text-xs font-medium" title={song.composer}>
                    {song.composer || "—"}
                  </div>
                {/if}
                {#if collectionStore.visibleColumns.album_artist}
                  <div class="text-brand-text-secondary truncate pr-4 min-w-0 text-xs font-medium" title={song.album_artist}>
                    {song.album_artist || "—"}
                  </div>
                {/if}
                {#if collectionStore.visibleColumns.format}
                  <div class="text-brand-text-secondary truncate pr-2 min-w-0 text-xs font-semibold uppercase">
                    {song.filetype ? song.filetype.toUpperCase() : "—"}
                  </div>
                {/if}
                {#if collectionStore.visibleColumns.year}
                  <div class="text-brand-text-secondary truncate pr-2 min-w-0 text-xs font-medium">
                    {song.year || "—"}
                  </div>
                {/if}
                {#if collectionStore.visibleColumns.genre}
                  <div class="text-brand-text-secondary truncate pr-2 min-w-0 text-xs font-medium" title={song.genre}>
                    {song.genre || "—"}
                  </div>
                {/if}
                {#if collectionStore.visibleColumns.grouping}
                  <div class="text-brand-text-secondary truncate pr-2 min-w-0 text-xs font-medium" title={song.grouping}>
                    {song.grouping || "—"}
                  </div>
                {/if}
                {#if collectionStore.visibleColumns.bpm}
                  <div class="text-brand-text-secondary truncate pr-2 min-w-0 text-xs font-mono">
                    {song.bpm || "—"}
                  </div>
                {/if}
                {#if collectionStore.visibleColumns.initial_key}
                  <div class="text-brand-text-secondary truncate pr-2 min-w-0 text-xs font-mono">
                    {song.initial_key || "—"}
                  </div>
                {/if}
                {#if collectionStore.visibleColumns.bitrate}
                  <div class="text-brand-text-secondary truncate pr-2 min-w-0 text-xs font-mono">
                    {song.bitrate ? `${song.bitrate}k` : "—"}
                  </div>
                {/if}
                {#if collectionStore.visibleColumns.samplerate}
                  <div class="text-brand-text-secondary truncate pr-2 min-w-0 text-xs font-mono">
                    {formatSampleRate(song.samplerate)}
                  </div>
                {/if}
                {#if collectionStore.visibleColumns.bitdepth}
                  <div class="text-brand-text-secondary truncate pr-2 min-w-0 text-xs font-mono">
                    {formatBitDepth(song.bitdepth)}
                  </div>
                {/if}
                {#if collectionStore.visibleColumns.channels}
                  <div class="text-brand-text-secondary truncate pr-2 min-w-0 text-xs font-medium">
                    {formatChannels(song.channels)}
                  </div>
                {/if}
                {#if collectionStore.visibleColumns.filesize}
                  <div class="text-brand-text-secondary truncate pr-2 min-w-0 text-xs font-mono">
                    {formatFileSize(song.filesize)}
                  </div>
                {/if}
                {#if collectionStore.visibleColumns.rating}
                  <div class="flex justify-center">
                    <SongRating rating={song.rating} onRate={(r) => rateSong(song, r)} />
                  </div>
                {/if}
                {#if collectionStore.visibleColumns.playcount}
                  <div class="text-center text-brand-text-secondary font-mono text-xs">
                    {song.playcount ?? 0}
                  </div>
                {/if}
                {#if collectionStore.visibleColumns.skipcount}
                  <div class="text-center text-brand-text-secondary font-mono text-xs">
                    {song.skipcount ?? 0}
                  </div>
                {/if}
                {#if collectionStore.visibleColumns.lastplayed}
                  <div class="text-center text-brand-text-secondary text-xs whitespace-nowrap">
                    {formatDate(song.lastplayed)}
                  </div>
                {/if}
                {#if collectionStore.visibleColumns.added}
                  <div class="text-center text-brand-text-secondary text-xs whitespace-nowrap">
                    {formatDateAdded(song.added)}
                  </div>
                {/if}
                {#if collectionStore.visibleColumns.duration}
                  <div class="text-center text-brand-text-secondary font-mono text-xs">{formatDuration(song.length_nanosec)}</div>
                {/if}
                {#if collectionStore.visibleColumns.path}
                  <div class="text-brand-text-secondary truncate pr-4 min-w-0 text-xs font-mono" title={song.path}>
                    {song.path || "—"}
                  </div>
                {/if}
                {#if collectionStore.visibleColumns.actions}
                  <div class="flex items-center justify-center gap-2.5">
                    <button
                      onclick={() => handleAddSongToPlaylist(song.id)}
                      class="text-brand-text-secondary hover:text-brand-accent-text transition-colors"
                      title={playlistsStore.activeCustomPlaylist
                        ? i18n.t('collection.addPlaylistTooltip', { name: playlistsStore.activeCustomPlaylist.name })
                        : i18n.t('collection.addPlaylistTooltipDefault')}
                    >
                      <Plus class="w-4 h-4" />
                    </button>
                    <button
                      onclick={() => openTagEditor(song.id)}
                      class="text-brand-text-secondary hover:text-brand-accent-text transition-colors"
                      title={i18n.t('collection.editTagsTooltip')}
                    >
                      <Edit3 class="w-4 h-4" />
                    </button>
                  </div>
                {/if}
              </div>
              {/snippet}
            </VirtualList>
            {/key}
          {/if}
        </div>
      </div>
    </div>

  {:else}
    <div class="flex-1 px-6 overflow-y-auto {playerStore.currentSong ? 'pb-28' : 'pb-6'}" use:rememberScroll={`collection:${collectionStore.activeSubTab}`}>
      <div class="sticky top-0 z-20 bg-brand-main pt-3">
        <div class="h-12 flex items-center justify-between">
          <div class="text-xs text-brand-text-secondary font-medium">
            {#if collectionStore.activeSubTab === "albums"}
              {sortedAlbums.length === 1 ? i18n.t('collection.showingOneAlbum') : i18n.t('collection.showingAlbums', { count: sortedAlbums.length })}
            {:else}
              {sortedArtists.length === 1 ? i18n.t('collection.showingOneArtist') : i18n.t('collection.showingArtists', { count: sortedArtists.length })}
            {/if}
          </div>

          <div class="flex items-center gap-2">
            <div class="inline-flex items-center gap-0.5 bg-brand-sidebar border border-brand-border rounded-full p-1">
              <button
                onclick={() => setActiveViewMode("cards")}
                class="flex items-center justify-center w-7 h-7 rounded-full transition-colors {activeViewMode === 'cards' ? 'bg-brand-accent text-white' : 'text-brand-text-secondary hover:text-brand-text-primary'}"
                title={i18n.t('collection.viewCards')}
                aria-label={i18n.t('collection.viewCards')}
                aria-pressed={activeViewMode === "cards"}
              >
                <LayoutGrid class="w-4 h-4" />
              </button>
              <button
                onclick={() => setActiveViewMode("rows")}
                class="flex items-center justify-center w-7 h-7 rounded-full transition-colors {activeViewMode === 'rows' ? 'bg-brand-accent text-white' : 'text-brand-text-secondary hover:text-brand-text-primary'}"
                title={i18n.t('collection.viewRows')}
                aria-label={i18n.t('collection.viewRows')}
                aria-pressed={activeViewMode === "rows"}
              >
                <Rows3 class="w-4 h-4" />
              </button>
            </div>
            {#if collectionStore.activeSubTab === "albums"}
              <div class="relative">
                <Select
                  value={`${albumSortField}-${albumSortAsc}`}
                  onchange={(e) => {
                    const [field, asc] = e.currentTarget.value.split("-");
                    albumSortField = field as "album" | "artist" | "year" | "rating" | "added";
                    albumSortAsc = asc === "true";
                  }}
                  class="bg-brand-sidebar border border-brand-border hover:border-brand-accent/60 text-brand-text-secondary text-xs rounded-full pl-2.5 pr-8 py-1.5 focus:outline-none focus:border-brand-accent transition-all font-medium"
                >
                  <option value="album-true">▲ {i18n.t('collection.tableHeaderAlbum')}</option>
                  <option value="album-false">▼ {i18n.t('collection.tableHeaderAlbum')}</option>
                  <option value="artist-true">▲ {i18n.t('collection.tableHeaderArtist')}</option>
                  <option value="artist-false">▼ {i18n.t('collection.tableHeaderArtist')}</option>
                  <option value="year-true">▲ {i18n.t('collection.tableHeaderYear')}</option>
                  <option value="year-false">▼ {i18n.t('collection.tableHeaderYear')}</option>
                  <option value="rating-true">▲ {i18n.t('collection.tableHeaderRating')}</option>
                  <option value="rating-false">▼ {i18n.t('collection.tableHeaderRating')}</option>
                  <option value="added-true">▲ {i18n.t('collection.sortDateAddedLabel')}</option>
                  <option value="added-false">▼ {i18n.t('collection.sortDateAddedLabel')}</option>
                </Select>
              </div>
            {:else if collectionStore.activeSubTab === "artists"}
              <div class="relative">
                <Select
                  value={`${artistSortField}-${artistSortAsc}`}
                  onchange={(e) => {
                    const [field, asc] = e.currentTarget.value.split("-");
                    artistSortField = field as "name" | "genre" | "song_count";
                    artistSortAsc = asc === "true";
                  }}
                  class="bg-brand-sidebar border border-brand-border hover:border-brand-accent/60 text-brand-text-secondary text-xs rounded-full pl-2.5 pr-8 py-1.5 focus:outline-none focus:border-brand-accent transition-all font-medium"
                >
                  <option value="name-true">▲ {i18n.t('collection.tableHeaderArtist')}</option>
                  <option value="name-false">▼ {i18n.t('collection.tableHeaderArtist')}</option>
                  <option value="genre-true">▲ {i18n.t('collection.tableHeaderGenre')}</option>
                  <option value="genre-false">▼ {i18n.t('collection.tableHeaderGenre')}</option>
                  <option value="song_count-true">▲ {i18n.t('collection.sortLabelSongs')}</option>
                  <option value="song_count-false">▼ {i18n.t('collection.sortLabelSongs')}</option>
                </Select>
              </div>
            {/if}
          </div>
        </div>
      </div>

      {#snippet albumEmptyState()}
        {#if sortedAlbums.length === 0 && collectionStore.searchQuery}
          <div class="col-span-full py-16 text-center">
            <SearchEmptyState
              icon={DiscAlbum}
              title={i18n.t('collection.noAlbumsTitle')}
              matchQueryText={i18n.t('collection.noAlbumsMatchQuery')}
              query={collectionStore.searchQuery}
              onReset={() => { collectionStore.searchQuery = ""; collectionStore.search(""); }}
            />
          </div>
        {:else if sortedAlbums.length === 0}
          <div class="col-span-full py-16 text-center">
            <LibraryWelcome />
          </div>
        {/if}
      {/snippet}

      {#snippet artistEmptyState()}
        {#if sortedArtists.length === 0 && collectionStore.searchQuery}
          <div class="col-span-full py-16 text-center">
            <SearchEmptyState
              icon={Mic2}
              title={i18n.t('collection.noArtistsTitle')}
              matchQueryText={i18n.t('collection.noArtistsMatchQuery')}
              query={collectionStore.searchQuery}
              onReset={() => { collectionStore.searchQuery = ""; collectionStore.search(""); }}
            />
          </div>
        {:else if sortedArtists.length === 0}
          <div class="col-span-full py-16 text-center">
            <LibraryWelcome />
          </div>
        {/if}
      {/snippet}

      <div class="pt-2">
        {#if collectionStore.activeSubTab === "albums"}
        {#if activeViewMode === "rows"}
          <div class="grid grid-cols-[repeat(auto-fill,minmax(280px,1fr))] gap-2">
            {#each sortedAlbums as album}
              <AlbumRowCard
                {album}
                oncontextmenu={(e) => handleAlbumContextMenu(e, album)}
              />
            {/each}
            {@render albumEmptyState()}
          </div>
        {:else}
          <div class="grid grid-cols-[repeat(auto-fill,minmax(180px,1fr))] gap-6">
            {#each sortedAlbums as album}
              <AlbumCard
                {album}
                widthClass="w-full"
                oncontextmenu={(e) => handleAlbumContextMenu(e, album)}
              />
            {/each}
            {@render albumEmptyState()}
          </div>
        {/if}
        {:else if collectionStore.activeSubTab === "artists"}
        {#if activeViewMode === "rows"}
          <div class="grid grid-cols-[repeat(auto-fill,minmax(280px,1fr))] gap-2">
            {#each sortedArtists as artist}
              {@const artistAlbums = getArtistAlbumsFor(artist.name)}
              {@const artistSongs = getArtistSongsFor(artist.name)}
              <ArtistRowCard
                {artist}
                {artistAlbums}
                {artistSongs}
                onclick={() => collectionStore.viewArtist(artist.name || "")}
              />
            {/each}
            {@render artistEmptyState()}
          </div>
        {:else}
          <div class="grid grid-cols-[repeat(auto-fill,minmax(180px,1fr))] gap-6">
            {#each sortedArtists as artist}
              {@const artistAlbums = getArtistAlbumsFor(artist.name)}
              {@const artistSongs = getArtistSongsFor(artist.name)}
              <ArtistCard
                {artist}
                {artistAlbums}
                {artistSongs}
                onclick={() => collectionStore.viewArtist(artist.name || "")}
              />
            {/each}
            {@render artistEmptyState()}
          </div>
        {/if}
        {/if}
      </div>
    </div>
  {/if}
</div>
{/if}

{#if editingSongId !== null}
  <TagEditor
    songId={editingSongId}
    onClose={() => { editingSongId = null; }}
    onSave={handleTagEditorSaved}
  />
{/if}

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

{#if albumContextMenuState}
  {@const album = albumContextMenuState.album}
  <AlbumContextMenu
    x={albumContextMenuState.x}
    y={albumContextMenuState.y}
    albumName={album.album || i18n.t("collection.unknownAlbum")}
    artistName={album.artist || undefined}
    onPlay={() => handlePlayAlbum(album.album || "")}
    onAddToPlaylist={async () => {
      let songs = await invoke<Song[]>("get_songs_by_album", { album: album.album || "" });
      if (songs.length > 0) {
        const targetPlaylist = playlistsStore.activeCustomPlaylist;
        const isQueue = !targetPlaylist || targetPlaylist.is_queue;
        const songIds = songs.map(s => s.id);

        if (isQueue) {
          {
            await playlistsStore.addSongsToQueue(songIds);
            const name = album.album || i18n.t("collection.unknownAlbum");
            toastStore.show(i18n.t("playlists.addedToQueueSuccess", { name }, `Added ${name} to Queue`));
          }
        } else {
          await playlistsStore.addSongsToPlaylist(targetPlaylist.id, songIds);
          toastStore.show(i18n.t("playlists.addedToPlaylistSuccess", { name: targetPlaylist.name }, `Added to ${targetPlaylist.name}`));
        }
      }
    }}
    onGoToArtist={album.artist ? () => collectionStore.viewArtist(album.artist || "") : undefined}
    onClose={() => { albumContextMenuState = null; }}
  />
{/if}

{#if selectedSongIds.size > 0 && collectionStore.activeSubTab === 'songs'}
  <div data-floating-toolbar="true" class="absolute left-1/2 -translate-x-1/2 z-40 bg-brand-sidebar/95 border border-brand-border/80 shadow-2xl rounded-full px-5 py-2.5 flex items-center gap-4 text-xs font-semibold backdrop-blur-xl animate-in fade-in slide-in-from-bottom-4 duration-200" class:bottom-6={!playerStore.currentSong} class:bottom-28={!!playerStore.currentSong}>
    <span class="text-brand-accent-text font-bold">
      {i18n.t('playlists.selectedCount', { count: selectedSongIds.size })}
    </span>
    <div class="h-4 w-px bg-brand-border/60"></div>
    <button
      onclick={handlePlaySelected}
      class="flex items-center gap-1.5 hover:text-brand-accent-text transition-colors"
    >
      <Play class="w-3.5 h-3.5 fill-current text-brand-accent-text" />
      <span>{i18n.t('playlists.playSelected')}</span>
    </button>
    <button
      onclick={handleBulkAddToPlaylist}
      class="flex items-center gap-1.5 hover:text-brand-accent-text transition-colors"
    >
      <Plus class="w-3.5 h-3.5 text-brand-accent-text" />
      <span>
        {playlistsStore.activeCustomPlaylist
          ? i18n.t('playlists.contextMenuAddToPlaylist', { name: playlistsStore.activeCustomPlaylist.name })
          : i18n.t('playlists.contextMenuAddToPlaylistDefault')}
      </span>
    </button>
    <div class="h-4 w-px bg-brand-border/60"></div>
    <button
      onclick={() => { selectedSongIds = new Set(); }}
      class="text-brand-text-secondary hover:text-brand-text-primary transition-colors"
    >
      {i18n.t('playlists.clearSelection')}
    </button>
  </div>
{/if}

<style>
  :global(svelte-virtual-list-viewport) {
    scrollbar-width: thin;
    scrollbar-color: var(--color-border) transparent;
  }
  :global(svelte-virtual-list-viewport::-webkit-scrollbar) {
    width: 6px;
  }
  :global(svelte-virtual-list-viewport::-webkit-scrollbar-track) {
    background: transparent;
  }
  :global(svelte-virtual-list-viewport::-webkit-scrollbar-thumb) {
    background: var(--color-border);
    border-radius: 3px;
  }
</style>

