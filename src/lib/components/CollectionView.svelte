<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { collectionStore } from "../stores/collection.svelte";
  import { playerStore } from "../stores/player.svelte";
  import { playlistsStore } from "../stores/playlists.svelte";
  import CoverArt from "./CoverArt.svelte";
  import SongRating from "./SongRating.svelte";
  import TagEditor from "./TagEditor.svelte";
  import { Play, Plus, Clock, FileText, Music, FolderClosed, Edit3, Columns, FilterX } from "lucide-svelte";
  import type { Song, AlbumItem, ArtistItem } from "../types";
  import { i18n } from "../stores/i18n.svelte";
  import { VirtualList } from "svelte-virtual-list-ts";
  import { getArtistAlbums, getArtistGradient } from "../utils/artist";
  import ArtistDetailView from "./ArtistDetailView.svelte";
  import AlbumDetailView from "./AlbumDetailView.svelte";
  import SongContextMenu from "./SongContextMenu.svelte";
  import AlbumContextMenu from "./AlbumContextMenu.svelte";
  import AlbumCard from "./AlbumCard.svelte";
  import ArtistCard from "./ArtistCard.svelte";

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
      alert(i18n.t("collection.selectPlaylistFirstAlert"));
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
    const cols: string[] = ["36px", "40px", "2fr", "1.5fr", "1.5fr"];
    const vc = collectionStore.visibleColumns;

    if (vc.format) cols.push("64px");
    if (vc.year) cols.push("60px");
    if (vc.genre) cols.push("1.2fr");
    if (vc.bitrate) cols.push("70px");
    if (vc.rating) cols.push("96px");
    if (vc.playcount) cols.push("70px");
    if (vc.duration) cols.push("80px");

    cols.push("80px");
    return `grid-template-columns: ${cols.join(" ")}`;
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
    // Filter out excluded formats
    songs = songs.filter(song => !collectionStore.isFormatExcluded(song.filetype));
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
      alert(i18n.t('collection.selectPlaylistFirstAlert'));
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
        <!-- Columns Toggle Popover -->
        <div class="relative">
          <button
            onclick={() => { showColumnsMenu = !showColumnsMenu; }}
            class="flex items-center gap-1.5 bg-brand-sidebar hover:bg-brand-main border border-brand-border text-brand-text-secondary hover:text-brand-text-primary text-xs rounded-lg px-2.5 py-1.5 focus:outline-none transition-all cursor-pointer font-medium"
            title="Custom Visible Columns"
          >
            <Columns class="w-3.5 h-3.5 text-brand-accent-text" />
            <span>Columns</span>
          </button>

          {#if showColumnsMenu}
            <div class="absolute right-0 top-full mt-2 bg-brand-sidebar border border-brand-border rounded-xl shadow-2xl p-3 z-50 w-52 flex flex-col gap-1.5 select-none">
              <div class="text-[11px] font-bold text-brand-text-secondary uppercase tracking-wider px-2 pb-1 border-b border-brand-border/40">
                Visible Columns
              </div>
              <label class="flex items-center gap-2 px-2 py-1 hover:bg-brand-main/60 rounded-lg text-xs cursor-pointer text-brand-text-primary">
                <input type="checkbox" checked={collectionStore.visibleColumns.format} onchange={() => collectionStore.toggleColumn("format")} class="rounded accent-brand-accent" />
                Format
              </label>
              <label class="flex items-center gap-2 px-2 py-1 hover:bg-brand-main/60 rounded-lg text-xs cursor-pointer text-brand-text-primary">
                <input type="checkbox" checked={collectionStore.visibleColumns.year} onchange={() => collectionStore.toggleColumn("year")} class="rounded accent-brand-accent" />
                Year
              </label>
              <label class="flex items-center gap-2 px-2 py-1 hover:bg-brand-main/60 rounded-lg text-xs cursor-pointer text-brand-text-primary">
                <input type="checkbox" checked={collectionStore.visibleColumns.genre} onchange={() => collectionStore.toggleColumn("genre")} class="rounded accent-brand-accent" />
                Genre
              </label>
              <label class="flex items-center gap-2 px-2 py-1 hover:bg-brand-main/60 rounded-lg text-xs cursor-pointer text-brand-text-primary">
                <input type="checkbox" checked={collectionStore.visibleColumns.bitrate} onchange={() => collectionStore.toggleColumn("bitrate")} class="rounded accent-brand-accent" />
                Bitrate
              </label>
              <label class="flex items-center gap-2 px-2 py-1 hover:bg-brand-main/60 rounded-lg text-xs cursor-pointer text-brand-text-primary">
                <input type="checkbox" checked={collectionStore.visibleColumns.rating} onchange={() => collectionStore.toggleColumn("rating")} class="rounded accent-brand-accent" />
                Rating
              </label>
              <label class="flex items-center gap-2 px-2 py-1 hover:bg-brand-main/60 rounded-lg text-xs cursor-pointer text-brand-text-primary">
                <input type="checkbox" checked={collectionStore.visibleColumns.playcount} onchange={() => collectionStore.toggleColumn("playcount")} class="rounded accent-brand-accent" />
                Play Count
              </label>
              <label class="flex items-center gap-2 px-2 py-1 hover:bg-brand-main/60 rounded-lg text-xs cursor-pointer text-brand-text-primary">
                <input type="checkbox" checked={collectionStore.visibleColumns.duration} onchange={() => collectionStore.toggleColumn("duration")} class="rounded accent-brand-accent" />
                Duration
              </label>
            </div>
          {/if}
        </div>

        <div class="relative">
          <select
            value={`${sortField}-${sortAsc}`}
            onchange={(e) => {
              const [field, asc] = e.currentTarget.value.split("-");
              sortField = field as keyof Song;
              sortAsc = asc === "true";
            }}
            class="bg-brand-sidebar hover:bg-brand-main border border-brand-border text-brand-text-secondary hover:text-brand-text-primary text-xs rounded-lg pl-2.5 pr-8 py-1.5 focus:outline-none focus:border-brand-accent transition-all cursor-pointer font-medium appearance-none -webkit-appearance-none"
            style="background-image: url(&quot;data:image/svg+xml;charset=utf-8,%3Csvg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 20 20' fill='none'%3E%3Cpath stroke='%239ca3af' stroke-linecap='round' stroke-linejoin='round' stroke-width='1.5' d='m6 8 4 4 4-4'/%3E%3C/svg%3E&quot;); background-position: right 0.625rem center; background-repeat: no-repeat; background-size: 1.25em;"
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
          </select>
        </div>
      </div>
    </div>

    <!-- Main View Songs Container -->
    <div class="flex-1 px-6 pt-2 overflow-hidden flex flex-col" class:pb-24={!!playerStore.currentSong}>
      <!-- Songs Table View -->
      <div class="flex-1 overflow-hidden border border-brand-border rounded-lg bg-brand-sidebar/40 flex flex-col min-h-0">
        <div class="sticky top-0 z-20 flex flex-col bg-brand-sidebar border-b border-brand-border text-xs text-brand-text-secondary uppercase tracking-wider font-semibold select-none">
          <div class="grid items-center py-3 px-4" style={gridColsStyle}>
            <div class="text-center w-9"></div>
            <button onclick={() => toggleSort("track")} class="text-left hover:text-brand-text-primary transition-colors flex items-center gap-1 cursor-pointer font-semibold uppercase tracking-wider">
              {i18n.t('collection.tableHeaderTrack')} {sortField === "track" ? (sortAsc ? "▲" : "▼") : ""}
            </button>
            <button onclick={() => toggleSort("title")} class="text-left hover:text-brand-text-primary transition-colors flex items-center gap-1 cursor-pointer font-semibold uppercase tracking-wider">
              {i18n.t('collection.tableHeaderTitle')} {sortField === "title" ? (sortAsc ? "▲" : "▼") : ""}
            </button>
            <button onclick={() => toggleSort("artist")} class="text-left hover:text-brand-text-primary transition-colors flex items-center gap-1 cursor-pointer font-semibold uppercase tracking-wider">
              {i18n.t('collection.tableHeaderArtist')} {sortField === "artist" ? (sortAsc ? "▲" : "▼") : ""}
            </button>
            <button onclick={() => toggleSort("album")} class="text-left hover:text-brand-text-primary transition-colors flex items-center gap-1 cursor-pointer font-semibold uppercase tracking-wider">
              {i18n.t('collection.tableHeaderAlbum')} {sortField === "album" ? (sortAsc ? "▲" : "▼") : ""}
            </button>
            {#if collectionStore.visibleColumns.format}
              <button onclick={() => toggleSort("filetype")} class="text-left hover:text-brand-text-primary transition-colors flex items-center gap-1 cursor-pointer font-semibold uppercase tracking-wider">
                Format {sortField === "filetype" ? (sortAsc ? "▲" : "▼") : ""}
              </button>
            {/if}
            {#if collectionStore.visibleColumns.year}
              <button onclick={() => toggleSort("year")} class="text-left hover:text-brand-text-primary transition-colors flex items-center gap-1 cursor-pointer font-semibold uppercase tracking-wider">
                Year {sortField === "year" ? (sortAsc ? "▲" : "▼") : ""}
              </button>
            {/if}
            {#if collectionStore.visibleColumns.genre}
              <button onclick={() => toggleSort("genre")} class="text-left hover:text-brand-text-primary transition-colors flex items-center gap-1 cursor-pointer font-semibold uppercase tracking-wider">
                Genre {sortField === "genre" ? (sortAsc ? "▲" : "▼") : ""}
              </button>
            {/if}
            {#if collectionStore.visibleColumns.bitrate}
              <button onclick={() => toggleSort("bitrate")} class="text-left hover:text-brand-text-primary transition-colors flex items-center gap-1 cursor-pointer font-semibold uppercase tracking-wider">
                Bitrate {sortField === "bitrate" ? (sortAsc ? "▲" : "▼") : ""}
              </button>
            {/if}
            {#if collectionStore.visibleColumns.rating}
              <button onclick={() => toggleSort("rating")} class="flex items-center justify-center hover:text-brand-text-primary transition-colors cursor-pointer font-semibold uppercase tracking-wider">
                {i18n.t('collection.tableHeaderRating')} {sortField === "rating" ? (sortAsc ? "▲" : "▼") : ""}
              </button>
            {/if}
            {#if collectionStore.visibleColumns.playcount}
              <button onclick={() => toggleSort("playcount")} class="text-center hover:text-brand-text-primary transition-colors flex items-center justify-center gap-1 cursor-pointer font-semibold uppercase tracking-wider">
                Plays {sortField === "playcount" ? (sortAsc ? "▲" : "▼") : ""}
              </button>
            {/if}
            {#if collectionStore.visibleColumns.duration}
              <button onclick={() => toggleSort("length_nanosec")} class="flex items-center justify-center hover:text-brand-text-primary transition-colors cursor-pointer font-semibold uppercase tracking-wider">
                <Clock class="w-4 h-4" /> {sortField === "length_nanosec" ? (sortAsc ? "▲" : "▼") : ""}
              </button>
            {/if}
            <div class="text-center">{i18n.t('collection.tableHeaderActions')}</div>
          </div>
        </div>

        <div class="flex-1 min-h-0 relative">
          {#if filteredSongs.length === 0}
            <div class="py-16 text-center">
              <div class="flex flex-col items-center justify-center max-w-sm mx-auto p-6 bg-brand-sidebar/20 rounded-xl border border-dashed border-brand-border/60 select-none">
                <FilterX class="w-12 h-12 text-brand-accent-text/40 mb-3 animate-pulse" />
                <h3 class="text-base font-semibold text-brand-text-primary mb-1">All results filtered out</h3>
                <p class="text-xs text-brand-text-secondary/60 mb-4">
                  {#if collectionStore.searchQuery}
                    No tracks match your query: <code class="bg-brand-sidebar px-1 py-0.5 rounded font-mono text-brand-accent-text">{collectionStore.searchQuery}</code>
                  {:else}
                    {i18n.t('collection.noSongsLibraryEmpty')}
                  {/if}
                </p>
                {#if collectionStore.searchQuery}
                  <button
                    onclick={() => { collectionStore.searchQuery = ""; collectionStore.search(""); }}
                    class="px-3.5 py-2 text-xs bg-brand-accent hover:bg-brand-accent-hover text-brand-accent-contrast rounded-xl shadow transition-all font-semibold cursor-pointer flex items-center gap-1.5"
                  >
                    <FilterX class="w-3.5 h-3.5" />
                    Reset Search & Filters
                  </button>
                {/if}
              </div>
            </div>
          {:else}
            <VirtualList items={filteredSongs} let:item={song}>
              <!-- svelte-ignore a11y_no_static_element_interactions -->
              <!-- svelte-ignore a11y_click_events_have_key_events -->
              <div
                data-song-row="true"
                onclick={(e) => handleSongClick(e, song)}
                ondblclick={() => handlePlaySong(song)}
                oncontextmenu={(e) => handleContextMenu(e, song)}
                style={gridColsStyle}
                class="grid items-center border-b border-brand-border/40 hover:bg-brand-sidebar/40 group transition-colors py-2.5 px-4 text-sm cursor-pointer
                  {selectedSongIds.has(song.id) ? 'bg-brand-accent/20 border-l-2 border-brand-accent text-brand-accent-text-hover' : (playerStore.currentSong && playerStore.currentSong.id === song.id ? 'bg-brand-accent/10 text-brand-accent-text-hover' : '')}"
              >
                <div class="text-center flex justify-center relative w-9 h-6 items-center">
                  {#if playerStore.currentSong && playerStore.currentSong.id === song.id && playerStore.state === 'playing'}
                    <div class="flex items-center justify-center gap-0.5 h-4 w-4 absolute group-hover:opacity-0 transition-opacity">
                      <span class="w-0.5 bg-brand-accent animate-bounce h-full" style="animation-delay: 0.1s"></span>
                      <span class="w-0.5 bg-brand-accent animate-bounce h-2/3" style="animation-delay: 0.2s"></span>
                      <span class="w-0.5 bg-brand-accent animate-bounce h-full" style="animation-delay: 0.3s"></span>
                    </div>
                  {/if}
                  <button
                    onclick={() => handlePlaySong(song)}
                    class="absolute flex items-center justify-center opacity-0 group-hover:opacity-100 text-brand-accent-text hover:text-brand-accent-text-hover transition-all duration-150 cursor-pointer"
                    title={i18n.t('collection.playSong')}
                  >
                    <Play class="w-4 h-4 fill-current" />
                  </button>
                </div>
                <div class="text-brand-text-secondary/70 truncate pr-4 min-w-0 font-medium">
                  {song.track !== undefined && song.track !== null ? song.track : "—"}
                </div>
                <div class="font-medium truncate pr-4 flex items-center gap-2 min-w-0">
                  <span
                    class="truncate font-medium {playerStore.currentSong && playerStore.currentSong.id === song.id ? 'text-brand-accent-text-hover' : 'text-brand-text-primary'}"
                    title={song.title || i18n.t('collection.unknownSong')}
                  >
                    {song.title || i18n.t('collection.unknownSong')}
                  </span>
                </div>
                <div class="text-brand-text-secondary/90 truncate pr-4 flex items-center min-w-0">
                  {#if song.artist}
                    <button
                      onclick={(e) => { e.stopPropagation(); collectionStore.viewArtist(song.album_artist?.trim() || song.artist || ""); }}
                      class="hover:underline hover:text-brand-accent-text transition-all duration-150 text-left truncate cursor-pointer text-brand-text-secondary/90"
                      title={i18n.t('collection.filterByArtist', { artist: song.artist })}
                    >
                      {song.artist}
                    </button>
                  {:else}
                    <span class="text-brand-text-secondary/50">{i18n.t('collection.unknownArtist')}</span>
                  {/if}
                </div>
                <div class="text-brand-text-secondary/70 truncate pr-4 flex items-center min-w-0">
                  {#if song.album}
                    <button
                      onclick={(e) => { e.stopPropagation(); collectionStore.viewAlbum(song.album || ""); }}
                      class="hover:underline hover:text-brand-accent-text transition-all duration-150 text-left truncate cursor-pointer text-brand-text-secondary/70"
                      title={i18n.t('collection.filterByAlbum', { album: song.album })}
                    >
                      {song.album}
                    </button>
                  {:else}
                    <span class="text-brand-text-secondary/50">{i18n.t('collection.unknownAlbum')}</span>
                  {/if}
                </div>
                {#if collectionStore.visibleColumns.format}
                  <div class="text-brand-text-secondary/70 truncate pr-2 min-w-0 text-xs font-semibold uppercase">
                    {song.filetype ? song.filetype.toUpperCase() : "—"}
                  </div>
                {/if}
                {#if collectionStore.visibleColumns.year}
                  <div class="text-brand-text-secondary/70 truncate pr-2 min-w-0 text-xs font-medium">
                    {song.year || "—"}
                  </div>
                {/if}
                {#if collectionStore.visibleColumns.genre}
                  <div class="text-brand-text-secondary/70 truncate pr-2 min-w-0 text-xs font-medium">
                    {song.genre || "—"}
                  </div>
                {/if}
                {#if collectionStore.visibleColumns.bitrate}
                  <div class="text-brand-text-secondary/70 truncate pr-2 min-w-0 text-xs font-mono">
                    {song.bitrate ? `${song.bitrate}k` : "—"}
                  </div>
                {/if}
                {#if collectionStore.visibleColumns.rating}
                  <div class="flex justify-center">
                    <SongRating rating={song.rating} onRate={(r) => rateSong(song, r)} />
                  </div>
                {/if}
                {#if collectionStore.visibleColumns.playcount}
                  <div class="text-center text-brand-text-secondary/80 font-mono text-xs">
                    {song.playcount ?? 0}
                  </div>
                {/if}
                {#if collectionStore.visibleColumns.duration}
                  <div class="text-center text-brand-text-secondary/80 font-mono text-xs">{formatDuration(song.length_nanosec)}</div>
                {/if}
                <div class="flex items-center justify-center gap-2.5">
                  <button
                    onclick={() => handleAddSongToPlaylist(song.id)}
                    class="text-brand-text-secondary/60 hover:text-brand-accent-text transition-colors cursor-pointer"
                    title={playlistsStore.activeCustomPlaylist
                      ? i18n.t('collection.addPlaylistTooltip', { name: playlistsStore.activeCustomPlaylist.name })
                      : i18n.t('collection.addPlaylistTooltipDefault')}
                  >
                    <Plus class="w-4 h-4" />
                  </button>
                  <button
                    onclick={() => openTagEditor(song.id)}
                    class="text-brand-text-secondary/60 hover:text-brand-accent-text transition-colors cursor-pointer"
                    title={i18n.t('collection.editTagsTooltip')}
                  >
                    <Edit3 class="w-4 h-4" />
                  </button>
                </div>
              </div>
            </VirtualList>
          {/if}
        </div>
      </div>
    </div>

  {:else}
    <!-- Scrollable Container for Albums / Artists Views -->
    <div class="flex-1 px-6 overflow-y-auto" class:pb-24={!!playerStore.currentSong}>
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
              <select
                value={`${albumSortField}-${albumSortAsc}`}
                onchange={(e) => {
                  const [field, asc] = e.currentTarget.value.split("-");
                  albumSortField = field as "album" | "artist" | "year" | "track_count";
                  albumSortAsc = asc === "true";
                }}
                class="bg-brand-sidebar hover:bg-brand-main border border-brand-border text-brand-text-secondary hover:text-brand-text-primary text-xs rounded-lg pl-2.5 pr-8 py-1.5 focus:outline-none focus:border-brand-accent transition-all cursor-pointer font-medium appearance-none -webkit-appearance-none"
                style="background-image: url(&quot;data:image/svg+xml;charset=utf-8,%3Csvg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 20 20' fill='none'%3E%3Cpath stroke='%239ca3af' stroke-linecap='round' stroke-linejoin='round' stroke-width='1.5' d='m6 8 4 4 4-4'/%3E%3C/svg%3E&quot;); background-position: right 0.625rem center; background-repeat: no-repeat; background-size: 1.25em;"
              >
                <option value="album-true">{i18n.t('collection.sortAlbumNameAsc')}</option>
                <option value="album-false">{i18n.t('collection.sortAlbumNameDesc')}</option>
                <option value="artist-true">{i18n.t('collection.sortArtistNameAsc')}</option>
                <option value="artist-false">{i18n.t('collection.sortArtistNameDesc')}</option>
                <option value="year-false">{i18n.t('collection.sortYearDesc')}</option>
                <option value="year-true">{i18n.t('collection.sortYearAsc')}</option>
                <option value="track_count-false">{i18n.t('collection.sortTracksDesc')}</option>
                <option value="track_count-true">{i18n.t('collection.sortTracksAsc')}</option>
              </select>
            </div>
          {:else if collectionStore.activeSubTab === "artists"}
            <div class="relative">
              <select
                value={`${artistSortField}-${artistSortAsc}`}
                onchange={(e) => {
                  const [field, asc] = e.currentTarget.value.split("-");
                  artistSortField = field as "name" | "genre" | "song_count";
                  artistSortAsc = asc === "true";
                }}
                class="bg-brand-sidebar hover:bg-brand-main border border-brand-border text-brand-text-secondary hover:text-brand-text-primary text-xs rounded-lg pl-2.5 pr-8 py-1.5 focus:outline-none focus:border-brand-accent transition-all cursor-pointer font-medium appearance-none -webkit-appearance-none"
                style="background-image: url(&quot;data:image/svg+xml;charset=utf-8,%3Csvg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 20 20' fill='none'%3E%3Cpath stroke='%239ca3af' stroke-linecap='round' stroke-linejoin='round' stroke-width='1.5' d='m6 8 4 4 4-4'/%3E%3C/svg%3E&quot;); background-position: right 0.625rem center; background-repeat: no-repeat; background-size: 1.25em;"
              >
                <option value="name-true">{i18n.t('collection.sortArtistNameAsc')}</option>
                <option value="name-false">{i18n.t('collection.sortArtistNameDesc')}</option>
                <option value="genre-true">{i18n.t('collection.sortGenreAsc')}</option>
                <option value="genre-false">{i18n.t('collection.sortGenreDesc')}</option>
                <option value="song_count-false">{i18n.t('collection.sortSongsDesc')}</option>
                <option value="song_count-true">{i18n.t('collection.sortSongsAsc')}</option>
              </select>
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
            {#if sortedAlbums.length === 0}
              <div class="col-span-full py-16 text-center">
                <div class="flex flex-col items-center justify-center max-w-sm mx-auto p-6 bg-brand-sidebar/20 rounded-xl border border-dashed border-brand-border/60 select-none">
                  <FolderClosed class="w-12 h-12 text-brand-accent-text/40 mb-3 animate-pulse" />
                  <h3 class="text-base font-semibold text-brand-text-primary mb-1">{i18n.t('collection.noAlbumsTitle')}</h3>
                  <p class="text-xs text-brand-text-secondary/60 font-medium">{i18n.t('collection.noAlbumsText')}</p>
                </div>
              </div>
            {/if}
          </div>
        {:else if collectionStore.activeSubTab === "artists"}
          <!-- Artists List Grid View -->
          <div class="grid grid-cols-[repeat(auto-fill,minmax(180px,1fr))] gap-6">
            {#each sortedArtists as artist}
              {@const artistAlbums = getArtistAlbumsFor(artist.name)}
              <ArtistCard
                {artist}
                {artistAlbums}
                onclick={() => collectionStore.viewArtist(artist.name || "")}
              />
            {/each}
            {#if sortedArtists.length === 0}
              <div class="col-span-full py-16 text-center">
                <div class="flex flex-col items-center justify-center max-w-sm mx-auto p-6 bg-brand-sidebar/20 rounded-xl border border-dashed border-brand-border/60 select-none">
                  <Music class="w-12 h-12 text-brand-accent-text/40 mb-3 animate-pulse" />
                  <h3 class="text-base font-semibold text-brand-text-primary mb-1">{i18n.t('collection.noArtistsTitle')}</h3>
                  <p class="text-xs text-brand-text-secondary/60 font-medium">{i18n.t('collection.noArtistsText')}</p>
                </div>
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
      songs = songs.filter(s => !collectionStore.isFormatExcluded(s.filetype));
      if (songs.length > 0 && playlistsStore.activeCustomPlaylist) {
        await playlistsStore.addSongsToPlaylist(playlistsStore.activeCustomPlaylist.id, songs.map(s => s.id));
      } else if (songs.length > 0) {
        alert(i18n.t("collection.selectPlaylistFirstAlert"));
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
