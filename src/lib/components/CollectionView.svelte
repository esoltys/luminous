<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { collectionStore } from "../stores/collection.svelte";
  import { playerStore } from "../stores/player.svelte";
  import { playlistsStore } from "../stores/playlists.svelte";
  import CoverArt from "./CoverArt.svelte";
  import TagEditor from "./TagEditor.svelte";
  import { Play, Plus, Clock, FileText, Music, FolderClosed, Edit3 } from "lucide-svelte";
  import type { Song, AlbumItem, ArtistItem } from "../types";
  import { i18n } from "../stores/i18n.svelte";
  import { VirtualList } from "svelte-virtual-list-ts";
  import { getArtistAlbums, getArtistGradient } from "../utils/artist";
  import ArtistDetailView from "./ArtistDetailView.svelte";
  import AlbumDetailView from "./AlbumDetailView.svelte";

  // activeSubTab and activeTab are managed globally via collectionStore

  let editingSongId = $state<number | null>(null);

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
    (typeof window !== "undefined" && localStorage.getItem("sort_album_field") as any) || "album"
  );
  let albumSortAsc = $state(
    typeof window !== "undefined" ? localStorage.getItem("sort_album_asc") !== "false" : true
  );

  let artistSortField = $state<"name" | "album_count" | "song_count">(
    (typeof window !== "undefined" && localStorage.getItem("sort_artist_field") as any) || "name"
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
    if (playlistsStore.activePlaylistId !== null) {
      await playlistsStore.addSongsToPlaylist(playlistsStore.activePlaylistId, [songId]);
    } else {
      alert(i18n.t('collection.selectPlaylistFirstAlert'));
    }
  }
</script>

{#if collectionStore.selectedAlbumName !== null}
  <AlbumDetailView albumName={collectionStore.selectedAlbumName} />
{:else if collectionStore.selectedArtistName !== null}
  <ArtistDetailView artistName={collectionStore.selectedArtistName} />
{:else}
<div class="flex-1 flex flex-col overflow-hidden bg-brand-main text-brand-text-secondary h-full">
  <!-- Top bar with Filter Info -->
  <div class="h-12 px-6 flex items-center justify-between">
    <!-- Filter Pills (Left) -->
    <div class="flex items-center gap-2 overflow-x-auto scrollbar-none max-w-xl py-1 select-none flex-nowrap flex-shrink-0">
      <button
        onclick={() => { collectionStore.activeSubTab = "artists"; collectionStore.selectedArtistName = null; }}
        class="px-3 py-1 rounded-full text-xs font-medium border transition-all cursor-pointer flex-shrink-0 {collectionStore.activeSubTab === 'artists' ? 'bg-brand-border border-brand-border text-white font-semibold shadow-sm' : 'border-transparent text-brand-text-secondary/70 hover:text-brand-text-primary hover:bg-brand-sidebar'}"
      >
        {i18n.t('collection.artists', { count: collectionStore.searchQuery.trim() !== "" ? collectionStore.filteredArtists.length : collectionStore.stats.total_artists })}
      </button>
      <button
        onclick={() => { collectionStore.activeSubTab = "albums"; collectionStore.selectedArtistName = null; }}
        class="px-3 py-1 rounded-full text-xs font-medium border transition-all cursor-pointer flex-shrink-0 {collectionStore.activeSubTab === 'albums' ? 'bg-brand-border border-brand-border text-white font-semibold shadow-sm' : 'border-transparent text-brand-text-secondary/70 hover:text-brand-text-primary hover:bg-brand-sidebar'}"
      >
        {i18n.t('collection.albums', { count: collectionStore.searchQuery.trim() !== "" ? collectionStore.filteredAlbums.length : collectionStore.stats.total_albums })}
      </button>
      <button
        onclick={() => { collectionStore.activeSubTab = "songs"; collectionStore.selectedArtistName = null; }}
        class="px-3 py-1 rounded-full text-xs font-medium border transition-all cursor-pointer flex-shrink-0 {collectionStore.activeSubTab === 'songs' ? 'bg-brand-border border-brand-border text-white font-semibold shadow-sm' : 'border-transparent text-brand-text-secondary/70 hover:text-brand-text-primary hover:bg-brand-sidebar'}"
      >
        {i18n.t('collection.songs', { count: collectionStore.searchQuery.trim() !== "" ? collectionStore.filteredSongs.length : collectionStore.stats.total_songs })}
      </button>
    </div>

    <!-- Showing Count & Sort Dropdown (Right) -->
    <div class="flex items-center gap-4">
      {#if collectionStore.activeSubTab === "songs"}
        <div class="relative">
          <select
            value={`${sortField}-${sortAsc}`}
            onchange={(e) => {
              const [field, asc] = e.currentTarget.value.split("-");
              sortField = field as any;
              sortAsc = asc === "true";
            }}
            class="bg-brand-sidebar hover:bg-brand-main border border-brand-border text-brand-text-secondary hover:text-brand-text-primary text-xs rounded-lg px-2.5 py-1.5 focus:outline-none focus:border-brand-accent transition-all cursor-pointer font-medium"
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
      {:else if collectionStore.activeSubTab === "albums"}
        <div class="relative">
          <select
            value={`${albumSortField}-${albumSortAsc}`}
            onchange={(e) => {
              const [field, asc] = e.currentTarget.value.split("-");
              albumSortField = field as any;
              albumSortAsc = asc === "true";
            }}
            class="bg-brand-sidebar hover:bg-brand-main border border-brand-border text-brand-text-secondary hover:text-brand-text-primary text-xs rounded-lg px-2.5 py-1.5 focus:outline-none focus:border-brand-accent transition-all cursor-pointer font-medium"
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
              artistSortField = field as any;
              artistSortAsc = asc === "true";
            }}
            class="bg-brand-sidebar hover:bg-brand-main border border-brand-border text-brand-text-secondary hover:text-brand-text-primary text-xs rounded-lg px-2.5 py-1.5 focus:outline-none focus:border-brand-accent transition-all cursor-pointer font-medium"
          >
            <option value="name-true">{i18n.t('collection.sortArtistNameAsc')}</option>
            <option value="name-false">{i18n.t('collection.sortArtistNameDesc')}</option>
            <option value="album_count-false">{i18n.t('collection.sortAlbumsDesc')}</option>
            <option value="album_count-true">{i18n.t('collection.sortAlbumsAsc')}</option>
            <option value="song_count-false">{i18n.t('collection.sortTracksDesc')}</option>
            <option value="song_count-true">{i18n.t('collection.sortTracksAsc')}</option>
          </select>
        </div>
      {/if}

      <div class="text-xs text-brand-text-secondary font-medium">
        {#if collectionStore.activeSubTab === "songs"}
          {i18n.t('collection.showingSongs', { count: filteredSongs.length })}
        {:else if collectionStore.activeSubTab === "albums"}
          {i18n.t('collection.showingAlbums', { count: sortedAlbums.length })}
        {:else}
          {i18n.t('collection.showingArtists', { count: sortedArtists.length })}
        {/if}
      </div>
    </div>
  </div>

  <!-- Main View Scrollable Container -->
  <div class="flex-1 px-6 pt-2 pb-24 {collectionStore.activeSubTab === 'songs' ? 'overflow-hidden flex flex-col' : 'overflow-y-auto'}">
    {#if collectionStore.activeSubTab === "songs"}
      <!-- Songs Table View -->
      <div class="flex-1 overflow-hidden border border-brand-border rounded-lg bg-brand-sidebar/40 flex flex-col min-h-0">
        <div class="sticky top-0 z-20 flex flex-col bg-brand-sidebar border-b border-brand-border text-xs text-brand-text-secondary uppercase tracking-wider font-semibold select-none">
          <div class="grid grid-cols-[36px_40px_2fr_1.5fr_1.5fr_96px_80px] items-center py-3 px-4">
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
            <button onclick={() => toggleSort("length_nanosec")} class="flex items-center justify-center hover:text-brand-text-primary transition-colors cursor-pointer font-semibold uppercase tracking-wider">
              <Clock class="w-4 h-4" /> {sortField === "length_nanosec" ? (sortAsc ? "▲" : "▼") : ""}
            </button>
            <div class="text-center">{i18n.t('collection.tableHeaderActions')}</div>
          </div>
        </div>

        <div class="flex-1 min-h-0 relative">
          {#if filteredSongs.length === 0}
            <div class="py-16 text-center">
              <div class="flex flex-col items-center justify-center max-w-sm mx-auto p-6 bg-brand-sidebar/20 rounded-xl border border-dashed border-brand-border/60 select-none">
                <Music class="w-12 h-12 text-brand-accent-text/40 mb-3 animate-pulse" />
                <h3 class="text-base font-semibold text-brand-text-primary mb-1">{i18n.t('collection.noSongsTitle')}</h3>
                <p class="text-xs text-brand-text-secondary/60">
                  {#if collectionStore.searchQuery}
                    {i18n.t('collection.noSongsSearchEmpty', { query: collectionStore.searchQuery })}
                  {:else}
                    {i18n.t('collection.noSongsLibraryEmpty')}
                  {/if}
                </p>
                {#if collectionStore.searchQuery}
                  <button
                    onclick={() => { collectionStore.searchQuery = ""; collectionStore.search(""); }}
                    class="mt-4 px-3 py-1.5 text-xs bg-brand-accent hover:bg-brand-accent-hover text-brand-accent-contrast rounded-lg transition-colors font-medium cursor-pointer"
                  >
                    {i18n.t('collection.clearSearchFilter')}
                  </button>
                {/if}
              </div>
            </div>
          {:else}
            <VirtualList items={filteredSongs} let:item={song}>
              <!-- svelte-ignore a11y_no_static_element_interactions -->
              <!-- svelte-ignore a11y_click_events_have_key_events -->
              <div
                ondblclick={() => handlePlaySong(song)}
                class="grid grid-cols-[36px_40px_2fr_1.5fr_1.5fr_96px_80px] items-center border-b border-brand-border/40 hover:bg-brand-sidebar/40 group transition-colors py-2.5 px-4 text-sm
                  {playerStore.currentSong && playerStore.currentSong.id === song.id ? 'bg-brand-accent/10 text-brand-accent-text-hover' : ''}"
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
                  <button
                    onclick={(e) => { e.stopPropagation(); collectionStore.navigateTo("collection", "songs", song.title || ""); }}
                    class="hover:underline hover:text-brand-accent-text transition-all duration-150 text-left truncate cursor-pointer font-medium {playerStore.currentSong && playerStore.currentSong.id === song.id ? 'text-brand-accent-text-hover' : 'text-brand-text-primary'}"
                    title={i18n.t('collection.filterByTitle', { title: song.title || i18n.t('collection.unknownSong') })}
                  >
                    {song.title || i18n.t('collection.unknownSong')}
                  </button>
                  <span class="px-1 py-0.5 text-[8px] font-semibold tracking-wider rounded uppercase bg-brand-sidebar text-brand-text-secondary border border-brand-border/50 shrink-0">
                    {song.filetype}
                  </span>
                  {#if song.lyrics && song.lyrics.trim() !== ""}
                    <span class="px-1 py-0.5 text-[8px] font-semibold tracking-wider rounded uppercase bg-brand-accent/10 text-brand-accent-text border border-brand-accent/20 shrink-0" title={i18n.t('collection.lyricsAvailable')}>
                      LRC
                    </span>
                  {/if}
                </div>
                <div class="text-brand-text-secondary/90 truncate pr-4 min-w-0">
                  {#if song.artist}
                    <button
                      onclick={(e) => { e.stopPropagation(); collectionStore.viewArtist(song.album_artist?.trim() || song.artist || ""); }}
                      class="hover:underline hover:text-brand-accent-text transition-all duration-150 text-left truncate cursor-pointer text-brand-text-secondary/90 text-left"
                      title={i18n.t('collection.filterByArtist', { artist: song.artist })}
                    >
                      {song.artist}
                    </button>
                  {:else}
                    <span class="text-brand-text-secondary/50">{i18n.t('collection.unknownArtist')}</span>
                  {/if}
                </div>
                <div class="text-brand-text-secondary/70 truncate pr-4 min-w-0">
                  {#if song.album}
                    <button
                      onclick={(e) => { e.stopPropagation(); collectionStore.viewAlbum(song.album || ""); }}
                      class="hover:underline hover:text-brand-accent-text transition-all duration-150 text-left truncate cursor-pointer text-brand-text-secondary/70 text-left"
                      title={i18n.t('collection.filterByAlbum', { album: song.album })}
                    >
                      {song.album}
                    </button>
                  {:else}
                    <span class="text-brand-text-secondary/50">{i18n.t('collection.unknownAlbum')}</span>
                  {/if}
                </div>
                <div class="text-center text-brand-text-secondary/80">{formatDuration(song.length_nanosec)}</div>
                <div class="flex items-center justify-center gap-2.5">
                  <button
                    onclick={() => handleAddSongToPlaylist(song.id)}
                    class="text-brand-text-secondary/60 hover:text-brand-accent-text transition-colors cursor-pointer"
                    title={i18n.t('collection.addPlaylistTooltip')}
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

    {:else if collectionStore.activeSubTab === "albums"}
      <!-- Albums Card Grid View -->
      <div class={sortedAlbums.length <= 3 ? "flex flex-row flex-wrap gap-6" : "grid grid-cols-[repeat(auto-fill,minmax(180px,1fr))] gap-6"}>
        {#each sortedAlbums as album}
          <!-- svelte-ignore a11y_no_static_element_interactions -->
          <!-- svelte-ignore a11y_click_events_have_key_events -->
          <div
            onclick={async () => {
              collectionStore.viewAlbum(album.album || "");
              await handlePlayAlbum(album.album || "");
            }}
            ondblclick={async () => {
              const albumName = album.album || i18n.t('collection.unknownAlbum');
              const playlistName = i18n.t('collection.albumPlaylistName', { name: albumName });
              let existingPlaylist = playlistsStore.playlists.find(p => p.name === playlistName);

              if (existingPlaylist) {
                await playlistsStore.selectPlaylist(existingPlaylist.id);
                await playlistsStore.clearPlaylist(existingPlaylist.id);
              } else {
                await playlistsStore.createPlaylist(playlistName);
              }

              let songs = await invoke<Song[]>("get_songs_by_album", {
                album: album.album || "",
              });

              // Filter out excluded formats
              songs = songs.filter(song => !collectionStore.isFormatExcluded(song.filetype));

              if (songs.length > 0) {
                const songIds = songs.map((s) => s.id);
                if (playlistsStore.activePlaylistId !== null) {
                  await playlistsStore.addSongsToPlaylist(playlistsStore.activePlaylistId, songIds);
                  collectionStore.activeTab = "playlists";
                  await playerStore.playPlaylistItem(playlistsStore.activePlaylistId, 0);
                }
              }
            }}
            class="{sortedAlbums.length <= 3 ? 'w-48 shrink-0' : 'w-full'} bg-brand-sidebar border border-brand-border/60 rounded-xl p-4 flex flex-col group hover:border-brand-accent/40 transition-all duration-200 cursor-pointer select-none"
          >
            <div
              class="aspect-square bg-brand-main rounded-lg mb-3 flex items-center justify-center text-brand-accent-text border border-brand-border overflow-hidden relative"
            >
              <CoverArt
                songId={undefined}
                artEmbedded={album.art_embedded}
                artAutomatic={album.art_automatic}
                artManual={album.art_manual}
                sizeClass="w-full h-full"
              />
              <div
                class="absolute inset-0 bg-black/60 opacity-0 group-hover:opacity-100 flex items-center justify-center transition-opacity"
              >
                <div class="w-12 h-12 rounded-full bg-brand-accent text-brand-accent-contrast flex items-center justify-center scale-75 group-hover:scale-100 transition-transform">
                  <Play class="w-5 h-5 fill-current ml-0.5" />
                </div>
              </div>
            </div>
            <button
              onclick={(e) => { e.stopPropagation(); collectionStore.viewAlbum(album.album || ""); }}
              class="font-semibold text-sm text-brand-text-primary hover:text-brand-accent-text hover:underline transition-all duration-150 text-left truncate w-full cursor-pointer"
              title={i18n.t('collection.filterByAlbum', { album: album.album || i18n.t('collection.unknownAlbum') })}
            >
              {album.album || i18n.t('collection.unknownAlbum')}
            </button>
            {#if album.artist}
              <button
                onclick={(e) => { e.stopPropagation(); collectionStore.viewArtist(album.artist || ""); }}
                class="text-xs text-brand-text-secondary hover:text-brand-accent-text hover:underline transition-all duration-150 text-left truncate w-full cursor-pointer mt-0.5"
                title={i18n.t('collection.filterByArtist', { artist: album.artist })}
              >
                {album.artist}
              </button>
            {:else}
              <span class="text-xs text-brand-text-secondary/60 text-left w-full mt-0.5 truncate">{i18n.t('collection.variousArtists')}</span>
            {/if}
            <div class="flex items-center justify-between mt-2 text-[10px] text-brand-text-secondary/50">
              <span>{album.year || ""}</span>
              <span>{i18n.t('playlists.songsCount', { count: album.track_count })}</span>
            </div>
          </div>
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
      <div class="grid grid-cols-[repeat(auto-fill,minmax(200px,1fr))] gap-6">
        {#each sortedArtists as artist}
          {@const artistAlbums = getArtistAlbumsFor(artist.name)}
          <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
          <!-- svelte-ignore a11y_click_events_have_key_events -->
          <div
            role="button"
            tabindex="0"
            onclick={() => collectionStore.viewArtist(artist.name || "")}
            onkeydown={(e) => { if (e.key === "Enter" || e.key === " ") { e.preventDefault(); collectionStore.viewArtist(artist.name || ""); } }}
            class="artist-card group bg-brand-sidebar border border-brand-border/60 rounded-xl p-4 flex flex-col items-center text-center hover:border-brand-accent/40 transition-all duration-200 cursor-pointer"
          >
            {#if artistAlbums.length > 0}
              <div class="artist-cover-stack relative w-24 h-24 mt-2 mb-4 shrink-0">
                {#each artistAlbums.slice(0, 6) as album, i ((album.album ?? "unknown") + i)}
                  <div
                    class="cover-item absolute inset-0 rounded-lg overflow-hidden border border-brand-border/50 shadow-lg"
                    style="z-index: {10 - i}; transform: translate({i * 12}px, {i * -9}px) rotate({i * 6}deg) scale({1 - i * 0.07}); opacity: {1 - i * 0.1};"
                  >
                    <CoverArt
                      songId={undefined}
                      artEmbedded={album.art_embedded}
                      artAutomatic={album.art_automatic}
                      artManual={album.art_manual}
                      sizeClass="w-full h-full"
                    />
                  </div>
                {/each}
              </div>
            {:else}
              <div class="w-24 h-24 bg-gradient-to-br {getArtistGradient(artist.name)} rounded-full mb-3 flex items-center justify-center text-white border border-brand-border/40 font-bold text-2xl shadow-md shrink-0">
                {artist.name ? artist.name.charAt(0).toUpperCase() : "?"}
              </div>
            {/if}
            <span
              class="font-semibold text-sm text-brand-text-primary group-hover:text-brand-accent-text group-hover:underline transition-all duration-150 text-center truncate w-full"
              title={i18n.t('collection.filterByArtist', { artist: artist.name || i18n.t('collection.unknownArtist') })}
            >
              {artist.name || i18n.t('collection.unknownArtist')}
            </span>
            <div class="flex gap-2 justify-center mt-2 text-[10px] text-brand-text-secondary/50">
              <span>{artist.album_count === 1 ? i18n.t('collection.oneAlbum') : i18n.t('collection.albumsCount', { count: artist.album_count })}</span>
              <span>•</span>
              <span>{i18n.t('playlists.songsCount', { count: artist.song_count })}</span>
            </div>
          </div>
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

{#if editingSongId !== null}
  <TagEditor
    songId={editingSongId}
    onClose={() => { editingSongId = null; }}
    onSave={handleTagEditorSaved}
  />
{/if}

<style>
  .artist-card {
    container-type: inline-size;
  }
  .cover-item:nth-child(n + 4) {
    display: none;
  }
  @container (min-width: 150px) {
    .cover-item:nth-child(4) {
      display: block;
    }
  }
  @container (min-width: 180px) {
    .cover-item:nth-child(5) {
      display: block;
    }
  }
  @container (min-width: 210px) {
    .cover-item:nth-child(6) {
      display: block;
    }
  }

  .scrollbar-none {
    scrollbar-width: none;
  }
  .scrollbar-none::-webkit-scrollbar {
    display: none;
  }

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
