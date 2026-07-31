<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { collectionStore } from "../stores/collection.svelte";
  import { playerStore } from "../stores/player.svelte";
  import { playlistsStore } from "../stores/playlists.svelte";
  import CoverArt from "./CoverArt.svelte";
  import SongRating from "./SongRating.svelte";
  import TagEditor from "./TagEditor.svelte";
  import { Play, Plus, Clock, FileText, Music, DiscAlbum, Mic2, Edit3, Columns } from "lucide-svelte";
  import type { Song, AlbumItem, ArtistItem } from "../types";
  import { i18n } from "../stores/i18n.svelte";
  import { toastStore } from "../stores/toast.svelte";
  import { VirtualList } from "svelte-virtual-list-ts";
  import { getArtistAlbums, getArtistSongs, getArtistGradient } from "../utils/artist";
  import ArtistDetailView from "./ArtistDetailView.svelte";
  import AlbumDetailView from "./AlbumDetailView.svelte";
  import SongContextMenu from "./SongContextMenu.svelte";
  import AlbumContextMenu from "./AlbumContextMenu.svelte";
  import AlbumCard from "./AlbumCard.svelte";
  import ArtistCard from "./ArtistCard.svelte";
  import SortableHeader from "./SortableHeader.svelte";
  import Select from "./Select.svelte";
  import NowPlayingBars from "./NowPlayingBars.svelte";
  import LinkButton from "./LinkButton.svelte";
  import ColumnSelector from "./ColumnSelector.svelte";
  import LibraryWelcome from "./LibraryWelcome.svelte";
  import SearchEmptyState from "./SearchEmptyState.svelte";
  import { formatDate, formatFileSize, formatSampleRate, formatBitDepth, formatChannels } from "../utils/formatters";

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

  let sortField = $state<keyof Song>(
    (typeof window !== "undefined" && localStorage.getItem("sort_song_field") as keyof Song) || "title"
  );
  let sortAsc = $state(
    typeof window !== "undefined" ? localStorage.getItem("sort_song_asc") !== "false" : true
  );

  // Trigger search on collectionStore when query changes
  $effect(() => {
    collectionStore.search(collectionStore.searchQuery);
  });

  // Computed songs list with filtering and sorting
  let filteredSongs = $derived.by(() => {
    let result = collectionStore.filteredSongs;

    // Apply sort
    return [...result].sort((a, b) => {
      let valA = a[sortField];
      let valB = b[sortField];

      if (valA === undefined) return sortAsc ? 1 : -1;
      if (valB === undefined) return sortAsc ? -1 : 1;

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

  let albumSortField = $state<"album" | "artist" | "year" | "track_count">(
    (typeof window !== "undefined" &&
      (localStorage.getItem("sort_album_field") as "album" | "artist" | "year" | "track_count")) ||
      "album"
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

  // Save sorting states to localStorage when they change
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

  // Computed sorted albums list
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

  // Computed sorted artists list
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

  let gridColsStyle = $derived.by(() => {
    const vc = collectionStore.visibleColumns;
    const cols: string[] = ["36px"]; // play indicator always present
    if (vc.track) cols.push("40px");
    if (vc.title) cols.push("2fr");
    if (vc.artist) cols.push("1.5fr");
    if (vc.album) cols.push("1.5fr");

    if (vc.composer) cols.push("1.5fr");
    if (vc.album_artist) cols.push("1.5fr");
    if (vc.format) cols.push("64px");
    if (vc.year) cols.push("60px");
    if (vc.genre) cols.push("1.2fr");
    if (vc.grouping) cols.push("1.2fr");
    if (vc.bpm) cols.push("60px");
    if (vc.initial_key) cols.push("60px");
    if (vc.bitrate) cols.push("70px");
    if (vc.samplerate) cols.push("75px");
    if (vc.bitdepth) cols.push("65px");
    if (vc.channels) cols.push("70px");
    if (vc.filesize) cols.push("75px");
    if (vc.rating) cols.push("96px");
    if (vc.playcount) cols.push("70px");
    if (vc.skipcount) cols.push("70px");
    if (vc.lastplayed) cols.push("90px");
    if (vc.added) cols.push("90px");
    if (vc.duration) cols.push("80px");
    if (vc.path) cols.push("2fr");
    if (vc.actions) cols.push("80px");

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

  function toggleSort(field: keyof Song) {
    if (sortField === field) {
      sortAsc = !sortAsc;
    } else {
      sortField = field;
      sortAsc = true;
    }
  }

  function handlePlaySong(song: Song) {
    const index = filteredSongs.findIndex((s) => s.id === song.id);
    const songIds = filteredSongs.map((s) => s.id);
    playerStore.playSongs(songIds, index >= 0 ? index : 0);
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
    if (playlistsStore.activeCustomPlaylist) {
      await playlistsStore.addSongsToPlaylist(playlistsStore.activeCustomPlaylist.id, [songId]);
    } else {
      toastStore.show(i18n.t('collection.selectPlaylistFirstAlert'));
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
    <!-- Top bar for Songs View -->
    <div class="h-12 px-6 flex items-center justify-between flex-shrink-0">
      <!-- Showing Count (Left) -->
      <div class="text-xs text-brand-text-secondary font-medium">
        {filteredSongs.length === 1 ? i18n.t('collection.showingOneSong') : i18n.t('collection.showingSongs', { count: filteredSongs.length })}
      </div>

      <!-- Sort Dropdown & Column Controls (Right) -->
      <div class="flex items-center gap-3">
        <ColumnSelector />

        <div class="relative">
          <Select
            value={`${sortField}-${sortAsc}`}
            onchange={(e) => {
              const [field, asc] = e.currentTarget.value.split("-");
              sortField = field as keyof Song;
              sortAsc = asc === "true";
            }}
            class="bg-brand-sidebar hover:bg-brand-main border border-brand-border text-brand-text-secondary hover:text-brand-text-primary text-xs rounded-lg pl-2.5 pr-8 py-1.5 focus:outline-none focus:border-brand-accent transition-all font-medium"
          >
            <option value="title-true">{i18n.t('collection.sortTitleAsc')}</option>
            <option value="title-false">{i18n.t('collection.sortTitleDesc')}</option>
            <option value="artist-true">{i18n.t('collection.sortArtistAsc')}</option>
            <option value="artist-false">{i18n.t('collection.sortArtistDesc')}</option>
            <option value="album-true">{i18n.t('collection.sortAlbumAsc')}</option>
            <option value="album-false">{i18n.t('collection.sortAlbumDesc')}</option>
            <option value="track-true">{i18n.t('collection.sortTrackAsc')}</option>
            <option value="track-false">{i18n.t('collection.sortTrackDesc')}</option>
            <option value="length_nanosec-true">{i18n.t('collection.sortDurationAsc')}</option>
            <option value="length_nanosec-false">{i18n.t('collection.sortDurationDesc')}</option>
          </Select>
        </div>
      </div>
    </div>

    <!-- Main View Songs Container -->
    <div class="flex-1 px-6 pt-2 overflow-hidden flex flex-col {playerStore.currentSong ? 'pb-28' : 'pb-6'}">
      <!-- Songs Table View -->
      <div bind:this={songsTableContainer} class="flex-1 overflow-hidden border border-brand-border rounded-lg bg-brand-sidebar/40 flex flex-col min-h-0">
        <div class="sticky top-0 z-20 flex flex-col bg-brand-sidebar border-b border-brand-border text-xs text-brand-text-secondary uppercase tracking-wider font-semibold select-none">
          <div class="grid items-center py-3 px-4" style="{gridColsStyle}; padding-right: calc(1rem + {scrollbarWidth}px)">
            <div class="text-center w-9"></div>
            {#if collectionStore.visibleColumns.track}
              <SortableHeader
                active={sortField === "track"}
                {sortAsc}
                onclick={() => toggleSort("track")}
                class="text-left hover:text-brand-text-primary transition-colors flex items-center gap-1 cursor-pointer font-semibold uppercase tracking-wider min-w-0"
              >
                {#snippet label(arrow)}<span class="truncate max-w-[calc(100%-1rem)]">{i18n.t('collection.tableHeaderTrack')} {arrow}</span>{/snippet}
              </SortableHeader>
            {/if}
            {#if collectionStore.visibleColumns.title}
              <SortableHeader
                active={sortField === "title"}
                {sortAsc}
                onclick={() => toggleSort("title")}
                class="text-left hover:text-brand-text-primary transition-colors flex items-center gap-1 cursor-pointer font-semibold uppercase tracking-wider min-w-0"
              >
                {#snippet label(arrow)}<span class="truncate max-w-[calc(100%-1rem)]">{i18n.t('collection.tableHeaderTitle')} {arrow}</span>{/snippet}
              </SortableHeader>
            {/if}
            {#if collectionStore.visibleColumns.artist}
              <SortableHeader
                active={sortField === "artist"}
                {sortAsc}
                onclick={() => toggleSort("artist")}
                class="text-left hover:text-brand-text-primary transition-colors flex items-center gap-1 cursor-pointer font-semibold uppercase tracking-wider min-w-0"
              >
                {#snippet label(arrow)}<span class="truncate max-w-[calc(100%-1rem)]">{i18n.t('collection.tableHeaderArtist')} {arrow}</span>{/snippet}
              </SortableHeader>
            {/if}
            {#if collectionStore.visibleColumns.album}
              <SortableHeader
                active={sortField === "album"}
                {sortAsc}
                onclick={() => toggleSort("album")}
                class="text-left hover:text-brand-text-primary transition-colors flex items-center gap-1 cursor-pointer font-semibold uppercase tracking-wider min-w-0"
              >
                {#snippet label(arrow)}<span class="truncate max-w-[calc(100%-1rem)]">{i18n.t('collection.tableHeaderAlbum')} {arrow}</span>{/snippet}
              </SortableHeader>
            {/if}
            {#if collectionStore.visibleColumns.composer}
              <SortableHeader
                active={sortField === "composer"}
                {sortAsc}
                onclick={() => toggleSort("composer")}
                class="text-left hover:text-brand-text-primary transition-colors flex items-center gap-1 cursor-pointer font-semibold uppercase tracking-wider min-w-0"
              >
                {#snippet label(arrow)}<span class="truncate max-w-[calc(100%-0.5rem)]">{i18n.t('collection.tableHeaderComposer')} {arrow}</span>{/snippet}
              </SortableHeader>
            {/if}
            {#if collectionStore.visibleColumns.album_artist}
              <SortableHeader
                active={sortField === "album_artist"}
                {sortAsc}
                onclick={() => toggleSort("album_artist")}
                class="text-left hover:text-brand-text-primary transition-colors flex items-center gap-1 cursor-pointer font-semibold uppercase tracking-wider min-w-0"
              >
                {#snippet label(arrow)}<span class="truncate max-w-[calc(100%-0.5rem)]">{i18n.t('collection.tableHeaderAlbumArtist')} {arrow}</span>{/snippet}
              </SortableHeader>
            {/if}
            {#if collectionStore.visibleColumns.format}
              <SortableHeader
                active={sortField === "filetype"}
                {sortAsc}
                onclick={() => toggleSort("filetype")}
                class="text-left hover:text-brand-text-primary transition-colors flex items-center gap-1 cursor-pointer font-semibold uppercase tracking-wider min-w-0"
              >
                {#snippet label(arrow)}<span class="truncate max-w-[calc(100%-0.5rem)]">{i18n.t('collection.tableHeaderFormat')} {arrow}</span>{/snippet}
              </SortableHeader>
            {/if}
            {#if collectionStore.visibleColumns.year}
              <SortableHeader
                active={sortField === "year"}
                {sortAsc}
                onclick={() => toggleSort("year")}
                class="text-left hover:text-brand-text-primary transition-colors flex items-center gap-1 cursor-pointer font-semibold uppercase tracking-wider min-w-0"
              >
                {#snippet label(arrow)}<span class="truncate max-w-[calc(100%-0.5rem)]">{i18n.t('collection.tableHeaderYear')} {arrow}</span>{/snippet}
              </SortableHeader>
            {/if}
            {#if collectionStore.visibleColumns.genre}
              <SortableHeader
                active={sortField === "genre"}
                {sortAsc}
                onclick={() => toggleSort("genre")}
                class="text-left hover:text-brand-text-primary transition-colors flex items-center gap-1 cursor-pointer font-semibold uppercase tracking-wider min-w-0"
              >
                {#snippet label(arrow)}<span class="truncate max-w-[calc(100%-0.5rem)]">{i18n.t('collection.tableHeaderGenre')} {arrow}</span>{/snippet}
              </SortableHeader>
            {/if}
            {#if collectionStore.visibleColumns.grouping}
              <SortableHeader
                active={sortField === "grouping"}
                {sortAsc}
                onclick={() => toggleSort("grouping")}
                class="text-left hover:text-brand-text-primary transition-colors flex items-center gap-1 cursor-pointer font-semibold uppercase tracking-wider min-w-0"
              >
                {#snippet label(arrow)}<span class="truncate max-w-[calc(100%-0.5rem)]">{i18n.t('collection.tableHeaderGrouping')} {arrow}</span>{/snippet}
              </SortableHeader>
            {/if}
            {#if collectionStore.visibleColumns.bpm}
              <SortableHeader
                active={sortField === "bpm"}
                {sortAsc}
                onclick={() => toggleSort("bpm")}
                class="text-left hover:text-brand-text-primary transition-colors flex items-center gap-1 cursor-pointer font-semibold uppercase tracking-wider min-w-0"
              >
                {#snippet label(arrow)}<span class="truncate max-w-[calc(100%-0.5rem)]">{i18n.t('collection.tableHeaderBpm')} {arrow}</span>{/snippet}
              </SortableHeader>
            {/if}
            {#if collectionStore.visibleColumns.initial_key}
              <SortableHeader
                active={sortField === "initial_key"}
                {sortAsc}
                onclick={() => toggleSort("initial_key")}
                class="text-left hover:text-brand-text-primary transition-colors flex items-center gap-1 cursor-pointer font-semibold uppercase tracking-wider min-w-0"
              >
                {#snippet label(arrow)}<span class="truncate max-w-[calc(100%-0.5rem)]">{i18n.t('collection.tableHeaderInitialKey')} {arrow}</span>{/snippet}
              </SortableHeader>
            {/if}
            {#if collectionStore.visibleColumns.bitrate}
              <SortableHeader
                active={sortField === "bitrate"}
                {sortAsc}
                onclick={() => toggleSort("bitrate")}
                class="text-left hover:text-brand-text-primary transition-colors flex items-center gap-1 cursor-pointer font-semibold uppercase tracking-wider min-w-0"
              >
                {#snippet label(arrow)}<span class="truncate max-w-[calc(100%-0.5rem)]">{i18n.t('collection.tableHeaderBitrate')} {arrow}</span>{/snippet}
              </SortableHeader>
            {/if}
            {#if collectionStore.visibleColumns.samplerate}
              <SortableHeader
                active={sortField === "samplerate"}
                {sortAsc}
                onclick={() => toggleSort("samplerate")}
                class="text-left hover:text-brand-text-primary transition-colors flex items-center gap-1 cursor-pointer font-semibold uppercase tracking-wider min-w-0"
              >
                {#snippet label(arrow)}<span class="truncate max-w-[calc(100%-0.5rem)]">{i18n.t('collection.tableHeaderSampleRate')} {arrow}</span>{/snippet}
              </SortableHeader>
            {/if}
            {#if collectionStore.visibleColumns.bitdepth}
              <SortableHeader
                active={sortField === "bitdepth"}
                {sortAsc}
                onclick={() => toggleSort("bitdepth")}
                class="text-left hover:text-brand-text-primary transition-colors flex items-center gap-1 cursor-pointer font-semibold uppercase tracking-wider min-w-0"
              >
                {#snippet label(arrow)}<span class="truncate max-w-[calc(100%-0.5rem)]">{i18n.t('collection.tableHeaderBitDepth')} {arrow}</span>{/snippet}
              </SortableHeader>
            {/if}
            {#if collectionStore.visibleColumns.channels}
              <SortableHeader
                active={sortField === "channels"}
                {sortAsc}
                onclick={() => toggleSort("channels")}
                class="text-left hover:text-brand-text-primary transition-colors flex items-center gap-1 cursor-pointer font-semibold uppercase tracking-wider min-w-0"
              >
                {#snippet label(arrow)}<span class="truncate max-w-[calc(100%-0.5rem)]">{i18n.t('collection.tableHeaderChannels')} {arrow}</span>{/snippet}
              </SortableHeader>
            {/if}
            {#if collectionStore.visibleColumns.filesize}
              <SortableHeader
                active={sortField === "filesize"}
                {sortAsc}
                onclick={() => toggleSort("filesize")}
                class="text-left hover:text-brand-text-primary transition-colors flex items-center gap-1 cursor-pointer font-semibold uppercase tracking-wider min-w-0"
              >
                {#snippet label(arrow)}<span class="truncate max-w-[calc(100%-0.5rem)]">{i18n.t('collection.tableHeaderFileSize')} {arrow}</span>{/snippet}
              </SortableHeader>
            {/if}
            {#if collectionStore.visibleColumns.rating}
              <SortableHeader
                active={sortField === "rating"}
                {sortAsc}
                onclick={() => toggleSort("rating")}
                class="flex items-center justify-center hover:text-brand-text-primary transition-colors cursor-pointer font-semibold uppercase tracking-wider min-w-0"
              >
                {#snippet label(arrow)}<span class="truncate">{i18n.t('collection.tableHeaderRating')} {arrow}</span>{/snippet}
              </SortableHeader>
            {/if}
            {#if collectionStore.visibleColumns.playcount}
              <SortableHeader
                active={sortField === "playcount"}
                {sortAsc}
                onclick={() => toggleSort("playcount")}
                class="text-center hover:text-brand-text-primary transition-colors flex items-center justify-center gap-1 cursor-pointer font-semibold uppercase tracking-wider min-w-0"
              >
                {#snippet label(arrow)}<span class="truncate">{i18n.t('collection.tableHeaderPlays')} {arrow}</span>{/snippet}
              </SortableHeader>
            {/if}
            {#if collectionStore.visibleColumns.skipcount}
              <SortableHeader
                active={sortField === "skipcount"}
                {sortAsc}
                onclick={() => toggleSort("skipcount")}
                class="text-center hover:text-brand-text-primary transition-colors flex items-center justify-center gap-1 cursor-pointer font-semibold uppercase tracking-wider min-w-0"
              >
                {#snippet label(arrow)}<span class="truncate">{i18n.t('collection.tableHeaderSkips')} {arrow}</span>{/snippet}
              </SortableHeader>
            {/if}
            {#if collectionStore.visibleColumns.lastplayed}
              <SortableHeader
                active={sortField === "lastplayed"}
                {sortAsc}
                onclick={() => toggleSort("lastplayed")}
                class="text-center hover:text-brand-text-primary transition-colors flex items-center justify-center gap-1 cursor-pointer font-semibold uppercase tracking-wider min-w-0"
              >
                {#snippet label(arrow)}<span class="truncate">{i18n.t('collection.tableHeaderLastPlayed')} {arrow}</span>{/snippet}
              </SortableHeader>
            {/if}
            {#if collectionStore.visibleColumns.added}
              <SortableHeader
                active={sortField === "added"}
                {sortAsc}
                onclick={() => toggleSort("added")}
                class="text-center hover:text-brand-text-primary transition-colors flex items-center justify-center gap-1 cursor-pointer font-semibold uppercase tracking-wider min-w-0"
              >
                {#snippet label(arrow)}<span class="truncate">{i18n.t('collection.tableHeaderAdded')} {arrow}</span>{/snippet}
              </SortableHeader>
            {/if}
            {#if collectionStore.visibleColumns.duration}
              <SortableHeader
                active={sortField === "length_nanosec"}
                {sortAsc}
                onclick={() => toggleSort("length_nanosec")}
                class="flex items-center justify-center hover:text-brand-text-primary transition-colors cursor-pointer font-semibold uppercase tracking-wider min-w-0"
              >
                {#snippet label(arrow)}<Clock class="w-4 h-4 shrink-0" /> {arrow}{/snippet}
              </SortableHeader>
            {/if}
            {#if collectionStore.visibleColumns.path}
              <SortableHeader
                active={sortField === "path"}
                {sortAsc}
                onclick={() => toggleSort("path")}
                class="text-left hover:text-brand-text-primary transition-colors flex items-center gap-1 cursor-pointer font-semibold uppercase tracking-wider min-w-0"
              >
                {#snippet label(arrow)}<span class="truncate max-w-[calc(100%-0.5rem)]">{i18n.t('collection.tableHeaderPath')} {arrow}</span>{/snippet}
              </SortableHeader>
            {/if}
            {#if collectionStore.visibleColumns.actions}
              <div class="text-center">{i18n.t('collection.tableHeaderActions')}</div>
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
            <VirtualList items={filteredSongs} let:item={song}>
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
                  {disabled ? 'opacity-50 cursor-not-allowed' : 'cursor-pointer'}
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
                    class="absolute flex items-center justify-center opacity-0 group-hover:opacity-100 text-brand-accent-text hover:text-brand-accent-text-hover transition-all duration-150 cursor-pointer disabled:opacity-0 disabled:cursor-not-allowed"
                    disabled={disabled}
                    title={disconnected ? i18n.t('collection.driveDisconnectedTooltip') : i18n.t('collection.playSong')}
                  >
                    <Play class="w-4 h-4 fill-current" />
                  </button>
                </div>
                {#if collectionStore.visibleColumns.track}
                  <div class="text-brand-text-secondary truncate pr-4 min-w-0 font-medium">
                    {song.track !== undefined && song.track !== null ? song.track : "—"}
                  </div>
                {/if}
                {#if collectionStore.visibleColumns.title}
                  <div class="font-medium truncate pr-4 flex items-center gap-2 min-w-0">
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
                  <div class="text-center text-brand-text-secondary font-mono text-xs whitespace-nowrap">
                    {formatDate(song.lastplayed)}
                  </div>
                {/if}
                {#if collectionStore.visibleColumns.added}
                  <div class="text-center text-brand-text-secondary font-mono text-xs whitespace-nowrap">
                    {formatDate(song.added)}
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
                      class="text-brand-text-secondary hover:text-brand-accent-text transition-colors cursor-pointer"
                      title={playlistsStore.activeCustomPlaylist
                        ? i18n.t('collection.addPlaylistTooltip', { name: playlistsStore.activeCustomPlaylist.name })
                        : i18n.t('collection.addPlaylistTooltipDefault')}
                    >
                      <Plus class="w-4 h-4" />
                    </button>
                    <button
                      onclick={() => openTagEditor(song.id)}
                      class="text-brand-text-secondary hover:text-brand-accent-text transition-colors cursor-pointer"
                      title={i18n.t('collection.editTagsTooltip')}
                    >
                      <Edit3 class="w-4 h-4" />
                    </button>
                  </div>
                {/if}
              </div>
            </VirtualList>
            {/key}
          {/if}
        </div>
      </div>
    </div>

  {:else}
    <!-- Scrollable Container for Albums / Artists Views -->
    <div class="flex-1 px-6 overflow-y-auto {playerStore.currentSong ? 'pb-28' : 'pb-6'}">
      <!-- Top bar with Filter Info / Sort controls (sticky) -->
      <div class="h-12 flex items-center justify-between sticky top-0 z-20 bg-brand-main">
        <!-- Showing Count (Left) -->
        <div class="text-xs text-brand-text-secondary font-medium">
          {#if collectionStore.activeSubTab === "albums"}
            {sortedAlbums.length === 1 ? i18n.t('collection.showingOneAlbum') : i18n.t('collection.showingAlbums', { count: sortedAlbums.length })}
          {:else}
            {sortedArtists.length === 1 ? i18n.t('collection.showingOneArtist') : i18n.t('collection.showingArtists', { count: sortedArtists.length })}
          {/if}
        </div>

        <!-- Sort Dropdown (Right) -->
        <div class="flex items-center gap-4">
          {#if collectionStore.activeSubTab === "albums"}
            <div class="relative">
              <Select
                value={`${albumSortField}-${albumSortAsc}`}
                onchange={(e) => {
                  const [field, asc] = e.currentTarget.value.split("-");
                  albumSortField = field as "album" | "artist" | "year" | "track_count";
                  albumSortAsc = asc === "true";
                }}
                class="bg-brand-sidebar hover:bg-brand-main border border-brand-border text-brand-text-secondary hover:text-brand-text-primary text-xs rounded-lg pl-2.5 pr-8 py-1.5 focus:outline-none focus:border-brand-accent transition-all font-medium"
              >
                <option value="album-true">{i18n.t('collection.sortAlbumNameAsc')}</option>
                <option value="album-false">{i18n.t('collection.sortAlbumNameDesc')}</option>
                <option value="artist-true">{i18n.t('collection.sortArtistNameAsc')}</option>
                <option value="artist-false">{i18n.t('collection.sortArtistNameDesc')}</option>
                <option value="year-false">{i18n.t('collection.sortYearDesc')}</option>
                <option value="year-true">{i18n.t('collection.sortYearAsc')}</option>
                <option value="track_count-false">{i18n.t('collection.sortTracksDesc')}</option>
                <option value="track_count-true">{i18n.t('collection.sortTracksAsc')}</option>
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
                class="bg-brand-sidebar hover:bg-brand-main border border-brand-border text-brand-text-secondary hover:text-brand-text-primary text-xs rounded-lg pl-2.5 pr-8 py-1.5 focus:outline-none focus:border-brand-accent transition-all font-medium"
              >
                <option value="name-true">{i18n.t('collection.sortArtistNameAsc')}</option>
                <option value="name-false">{i18n.t('collection.sortArtistNameDesc')}</option>
                <option value="genre-true">{i18n.t('collection.sortGenreAsc')}</option>
                <option value="genre-false">{i18n.t('collection.sortGenreDesc')}</option>
                <option value="song_count-false">{i18n.t('collection.sortSongsDesc')}</option>
                <option value="song_count-true">{i18n.t('collection.sortSongsAsc')}</option>
              </Select>
            </div>
          {/if}
        </div>
      </div>

      <div class="pt-2">
        {#if collectionStore.activeSubTab === "albums"}
          <!-- Albums Card Grid View -->
          <div class="grid grid-cols-[repeat(auto-fill,minmax(180px,1fr))] gap-6">
            {#each sortedAlbums as album}
              <AlbumCard
                {album}
                widthClass="w-full"
                oncontextmenu={(e) => handleAlbumContextMenu(e, album)}
              />
            {/each}
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
          </div>
        {:else if collectionStore.activeSubTab === "artists"}
          <!-- Artists List Grid View -->
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
          </div>
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
      if (songs.length > 0 && playlistsStore.activeCustomPlaylist) {
        await playlistsStore.addSongsToPlaylist(playlistsStore.activeCustomPlaylist.id, songs.map(s => s.id));
      } else if (songs.length > 0) {
        toastStore.show(i18n.t("collection.selectPlaylistFirstAlert"));
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
      class="flex items-center gap-1.5 hover:text-brand-accent-text transition-colors cursor-pointer"
    >
      <Play class="w-3.5 h-3.5 fill-current text-brand-accent-text" />
      <span>{i18n.t('playlists.playSelected')}</span>
    </button>
    <button
      onclick={handleBulkAddToPlaylist}
      class="flex items-center gap-1.5 hover:text-brand-accent-text transition-colors cursor-pointer"
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
      class="text-brand-text-secondary hover:text-brand-text-primary transition-colors cursor-pointer"
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
