<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { collectionStore } from "../stores/collection.svelte";
  import { navigationStore } from "../stores/navigation.svelte";
  import { playerStore } from "../stores/player.svelte";
  import { playlistsStore } from "../stores/playlists.svelte";
  import CoverArt from "./CoverArt.svelte";
  import SongRating from "./SongRating.svelte";
  import TagEditor from "./TagEditor.svelte";
  import {
    PlayIcon as Play,
    PlusIcon as Plus,
    MusicNotesIcon as Music,
    DiscIcon as DiscAlbum,
    MicrophoneStageIcon as Mic2,
    SquaresFourIcon as LayoutGrid,
    RowsIcon as Rows3
  } from "phosphor-svelte";
  import type { Song, AlbumItem, ArtistItem } from "../types";
  import { i18n } from "../stores/i18n.svelte";
  import { toastStore } from "../stores/toast.svelte";
  import { prefs, type CollectionViewMode } from "../stores/prefs.svelte";
  import { getArtistAlbums, getArtistSongs, getArtistGradient } from "../utils/artist";
  import ArtistDetailView from "./ArtistDetailView.svelte";
  import AlbumDetailView from "./AlbumDetailView.svelte";
  import GenreBrowseView from "./GenreBrowseView.svelte";
  import { tagsStore } from "../stores/tags.svelte";
  import SongContextMenu from "./SongContextMenu.svelte";
  import AlbumContextMenu from "./AlbumContextMenu.svelte";
  import AlbumCard from "./AlbumCard.svelte";
  import ArtistCard from "./ArtistCard.svelte";
  import AlbumRowCard from "./AlbumRowCard.svelte";
  import ArtistRowCard from "./ArtistRowCard.svelte";
  import Select from "./Select.svelte";
  import ColumnSelector from "./ColumnSelector.svelte";
  import LibraryWelcome from "./LibraryWelcome.svelte";
  import SearchEmptyState from "./SearchEmptyState.svelte";
  import SongTable, { type SongTableRow } from "./SongTable.svelte";
  import { SONG_TABLE_COLUMNS } from "../utils/songColumns";
  import { rememberScroll } from "../utils/scrollMemory";

  // activeSubTab and activeTab are managed globally via collectionStore

  let editingSongId = $state<number | null>(null);
  let showColumnsMenu = $state(false);
  let contextMenuState = $state<{ x: number; y: number; song: Song } | null>(null);
  let albumContextMenuState = $state<{ x: number; y: number; album: AlbumItem } | null>(null);

  let selectedKeys = $state<Set<string>>(new Set());

  function handleRowContextMenu(event: MouseEvent, row: SongTableRow) {
    if (row.song) contextMenuState = { x: event.clientX, y: event.clientY, song: row.song };
  }

  function handleAlbumContextMenu(event: MouseEvent, album: AlbumItem) {
    event.preventDefault();
    albumContextMenuState = { x: event.clientX, y: event.clientY, album };
  }

  async function handleBulkAddToPlaylist() {
    if (selectedKeys.size === 0) return;
    if (playlistsStore.activeCustomPlaylist) {
      await playlistsStore.addSongsToPlaylist(playlistsStore.activeCustomPlaylist.id, Array.from(selectedKeys, Number));
    } else {
      toastStore.show(i18n.t("collection.selectPlaylistFirstAlert"));
    }
  }

  function handlePlaySelected() {
    if (selectedKeys.size === 0) return;
    const selectedList = filteredSongs.filter((s) => selectedKeys.has(String(s.id)));
    if (selectedList.length > 0) {
      playerStore.playSongs(selectedList.map((s) => s.id), 0);
    }
  }

  function openTagEditor(songId: number) {
    editingSongId = songId;
  }

  function handleTagEditorSaved() {
    collectionStore.refreshLibrary();
    tagsStore.load();
  }

  function getArtistAlbumsFor(name: string | null): AlbumItem[] {
    return getArtistAlbums(collectionStore.albums, name);
  }

  function getArtistSongsFor(name: string | null): Song[] {
    return getArtistSongs(collectionStore.songs, name);
  }

  // Albums and Artists each remember their own Cards/Rows view mode.
  let activeViewMode = $derived(
    navigationStore.activeSubTab === "albums" ? prefs.albumsViewMode : prefs.artistsViewMode
  );

  function setActiveViewMode(mode: CollectionViewMode) {
    if (navigationStore.activeSubTab === "albums") {
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
      let valA: unknown = a[sortField];
      let valB: unknown = b[sortField];

      if (sortField === "title") {
        valA = a.titlesort?.trim() || a.title;
        valB = b.titlesort?.trim() || b.title;
      } else if (sortField === "artist") {
        valA = a.album_artist_sort?.trim() || a.artistsort?.trim() || a.album_artist || a.artist;
        valB = b.album_artist_sort?.trim() || b.artistsort?.trim() || b.album_artist || b.artist;
      } else if (sortField === "album") {
        valA = a.albumsort?.trim() || a.album;
        valB = b.albumsort?.trim() || b.album;
      } else if (sortField === "composer") {
        valA = a.composersort?.trim() || a.composer;
        valB = b.composersort?.trim() || b.composer;
      } else if (sortField === "genre") {
        valA = a.genresort?.trim() || a.genre;
        valB = b.genresort?.trim() || b.genre;
      }

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
      let valA: unknown = a[field];
      let valB: unknown = b[field];

      if (field === "album") {
        valA = a.albumsort?.trim() || a.album;
        valB = b.albumsort?.trim() || b.album;
      } else if (field === "artist") {
        valA = a.artist_sort?.trim() || a.artist;
        valB = b.artist_sort?.trim() || b.artist;
      }

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
      let valA: unknown = field === "genre"
        ? (a.genre?.trim() || i18n.t('artistDetail.unknownGenre'))
        : (field === "name" ? (a.sort_artist?.trim() || a.name) : a[field]);
      let valB: unknown = field === "genre"
        ? (b.genre?.trim() || i18n.t('artistDetail.unknownGenre'))
        : (field === "name" ? (b.sort_artist?.trim() || b.name) : b[field]);

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

  function toggleSort(field: keyof Song) {
    if (sortField === field) {
      sortAsc = !sortAsc;
    } else {
      sortField = field;
      sortAsc = true;
    }
  }

  async function handlePlaySong(song: Song) {
    try {
      await playerStore.playSong(song.id);
    } catch (err) {
      console.error("Failed to play song:", err);
      toastStore.show(i18n.t("playerBar.playSongFailed", {}, "Couldn't play this track."), "error");
    }
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

  async function handleAddSongToPlaylist(songId: number) {
    const songObj = collectionStore.songs.find((s) => s.id === songId);
    await playlistsStore.addSongsToActiveTarget([songId], songObj?.title || "Song");
  }

  async function rateSong(song: Song, rating: number) {
    song.rating = await invoke<number>("set_song_rating", { songId: song.id, rating });
  }

  function songToRow(song: Song): SongTableRow {
    const disconnected = !song.unavailable && collectionStore.isPathOnDisconnectedDrive(song.path);
    return {
      key: String(song.id),
      song,
      disabled: song.unavailable || disconnected,
      disabledTooltip: disconnected ? i18n.t("collection.driveDisconnectedTooltip") : undefined,
    };
  }

  let tableRows = $derived(filteredSongs.map(songToRow));
</script>

{#snippet songsEmptyState()}
  {#if filteredSongs.length === 0 && collectionStore.searchQuery}
    <SearchEmptyState
      icon={Music}
      title={i18n.t('collection.noSongsTitle')}
      matchQueryText={i18n.t('collection.noTracksMatchQuery')}
      query={collectionStore.searchQuery}
      onReset={() => { collectionStore.searchQuery = ""; collectionStore.search(""); }}
    />
  {:else}
    <!-- Library has no songs at all yet (no watched folders) — a distinct
         welcome moment, not a "your search/filters found nothing" state. -->
    <LibraryWelcome />
  {/if}
{/snippet}

{#if navigationStore.selectedAlbumName !== null}
  <AlbumDetailView albumName={navigationStore.selectedAlbumName} />
{:else if navigationStore.selectedArtistName !== null}
  <ArtistDetailView artistName={navigationStore.selectedArtistName} />
{:else}
<div class="flex-1 flex flex-col overflow-hidden bg-brand-main text-brand-text-secondary h-full">
  {#if navigationStore.activeSubTab === "songs"}
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
              class="bg-brand-sidebar border border-brand-border hover:border-brand-accent/60 text-brand-text-secondary text-xs rounded-full pl-3.5 pr-8 py-1.5 focus:outline-none focus:border-brand-accent transition-all font-medium"
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
      <div class="flex-1 overflow-hidden border border-brand-border rounded-lg bg-brand-sidebar/40 flex flex-col min-h-0">
        <SongTable
          rows={tableRows}
          mode="track"
          leadingColumnWidth="36px"
          colDefaults={COLLECTION_COL_DEFAULTS}
          {sortField}
          {sortAsc}
          onToggleSort={(f) => toggleSort(f as keyof Song)}
          bind:selectedKeys
          onRowDoubleClick={(row) => row.song && handlePlaySong(row.song)}
          onRowContextMenu={handleRowContextMenu}
          onRate={rateSong}
          onAddToPlaylist={(song) => handleAddSongToPlaylist(song.id)}
          onEditTags={(song) => openTagEditor(song.id)}
          virtualized
          scrollMemoryKey="collection:songs"
          emptyState={songsEmptyState}
        />
      </div>
    </div>

  {:else if navigationStore.activeSubTab === "genres"}
    <GenreBrowseView />
  {:else}
    <div class="flex-1 px-6 overflow-y-auto {playerStore.currentSong ? 'pb-28' : 'pb-6'}" use:rememberScroll={`collection:${navigationStore.activeSubTab}`}>
      <div class="sticky top-0 z-20 bg-brand-main pt-3">
        <div class="h-12 flex items-center justify-between">
          <div class="text-xs text-brand-text-secondary font-medium">
            {#if navigationStore.activeSubTab === "albums"}
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
            {#if navigationStore.activeSubTab === "albums"}
              <div class="relative">
                <Select
                  value={`${albumSortField}-${albumSortAsc}`}
                  onchange={(e) => {
                    const [field, asc] = e.currentTarget.value.split("-");
                    albumSortField = field as "album" | "artist" | "year" | "rating" | "added";
                    albumSortAsc = asc === "true";
                  }}
                  class="bg-brand-sidebar border border-brand-border hover:border-brand-accent/60 text-brand-text-secondary text-xs rounded-full pl-3.5 pr-8 py-1.5 focus:outline-none focus:border-brand-accent transition-all font-medium"
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
            {:else if navigationStore.activeSubTab === "artists"}
              <div class="relative">
                <Select
                  value={`${artistSortField}-${artistSortAsc}`}
                  onchange={(e) => {
                    const [field, asc] = e.currentTarget.value.split("-");
                    artistSortField = field as "name" | "genre" | "song_count";
                    artistSortAsc = asc === "true";
                  }}
                  class="bg-brand-sidebar border border-brand-border hover:border-brand-accent/60 text-brand-text-secondary text-xs rounded-full pl-3.5 pr-8 py-1.5 focus:outline-none focus:border-brand-accent transition-all font-medium"
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
        {#if navigationStore.activeSubTab === "albums"}
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
        {:else if navigationStore.activeSubTab === "artists"}
        {#if activeViewMode === "rows"}
          <div class="grid grid-cols-[repeat(auto-fill,minmax(280px,1fr))] gap-2">
            {#each sortedArtists as artist}
              {@const artistAlbums = getArtistAlbumsFor(artist.name)}
              {@const artistSongs = getArtistSongsFor(artist.name)}
              <ArtistRowCard
                {artist}
                {artistAlbums}
                {artistSongs}
                onclick={() => navigationStore.viewArtist(artist.name || "")}
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
                onclick={() => navigationStore.viewArtist(artist.name || "")}
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
        await playlistsStore.addSongsToActiveTarget(
          songs.map(s => s.id),
          album.album || i18n.t("collection.unknownAlbum")
        );
      }
    }}
    onGoToArtist={album.artist ? () => navigationStore.viewArtist(album.artist || "") : undefined}
    onClose={() => { albumContextMenuState = null; }}
  />
{/if}

{#if selectedKeys.size > 0 && navigationStore.activeSubTab === 'songs'}
  <div data-floating-toolbar="true" class="absolute left-1/2 -translate-x-1/2 z-40 bg-brand-sidebar/95 border border-brand-border/80 shadow-2xl rounded-full px-5 py-2.5 flex items-center gap-4 text-xs font-semibold backdrop-blur-xl animate-in fade-in slide-in-from-bottom-4 duration-200" class:bottom-6={!playerStore.currentSong} class:bottom-28={!!playerStore.currentSong}>
    <span class="text-brand-accent-text font-bold">
      {i18n.t('playlists.selectedCount', { count: selectedKeys.size })}
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
      onclick={() => { selectedKeys = new Set(); }}
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

