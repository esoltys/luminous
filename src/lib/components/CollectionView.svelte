<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { collectionStore } from "../stores/collection.svelte";
  import { playerStore } from "../stores/player.svelte";
  import { playlistsStore } from "../stores/playlists.svelte";
  import CoverArt from "./CoverArt.svelte";
  import TagEditor from "./TagEditor.svelte";
  import { Play, Plus, Clock, FileText, Music, FolderClosed, Edit3 } from "lucide-svelte";
  import type { Song, AlbumItem, ArtistItem } from "../types";
  import { VirtualList } from "svelte-virtual-list-ts";

  // activeSubTab and activeTab are managed globally via collectionStore

  let editingSongId = $state<number | null>(null);

  function openTagEditor(songId: number) {
    editingSongId = songId;
  }

  function handleTagEditorSaved() {
    collectionStore.refreshLibrary();
  }

  function getArtistGradient(name: string | null): string {
    if (!name) return "from-purple-900 to-indigo-900";
    let hash = 0;
    for (let i = 0; i < name.length; i++) {
      hash = name.charCodeAt(i) + ((hash << 5) - hash);
    }
    const index = Math.abs(hash) % 5;
    const gradients = [
      "from-indigo-600 to-purple-600",
      "from-rose-600 to-orange-600",
      "from-emerald-600 to-teal-600",
      "from-cyan-600 to-blue-600",
      "from-amber-600 to-red-600"
    ];
    return gradients[index];
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
      alert("Please select or create a playlist first from the Playlists tab.");
    }
  }
</script>

<div class="flex-1 flex flex-col overflow-hidden bg-brand-main text-brand-text-secondary h-full">
  <!-- Top bar with Filter Info -->
  <div class="h-12 px-6 border-b border-brand-border flex items-center justify-between">
    <!-- Filter Pills (Left) -->
    <div class="flex items-center gap-2 overflow-x-auto scrollbar-none max-w-xl py-1 select-none flex-nowrap flex-shrink-0">
      <button
        onclick={() => { collectionStore.activeSubTab = "artists"; }}
        class="px-3 py-1 rounded-full text-xs font-medium border transition-all cursor-pointer flex-shrink-0 {collectionStore.activeSubTab === 'artists' ? 'bg-brand-border border-brand-border text-brand-accent font-semibold shadow-sm' : 'border-transparent text-brand-text-secondary/70 hover:text-brand-text-primary hover:bg-brand-sidebar'}"
      >
        Artists ({collectionStore.searchQuery.trim() !== "" ? collectionStore.filteredArtists.length : collectionStore.stats.total_artists})
      </button>
      <button
        onclick={() => { collectionStore.activeSubTab = "albums"; }}
        class="px-3 py-1 rounded-full text-xs font-medium border transition-all cursor-pointer flex-shrink-0 {collectionStore.activeSubTab === 'albums' ? 'bg-brand-border border-brand-border text-brand-accent font-semibold shadow-sm' : 'border-transparent text-brand-text-secondary/70 hover:text-brand-text-primary hover:bg-brand-sidebar'}"
      >
        Albums ({collectionStore.searchQuery.trim() !== "" ? collectionStore.filteredAlbums.length : collectionStore.stats.total_albums})
      </button>
      <button
        onclick={() => { collectionStore.activeSubTab = "songs"; }}
        class="px-3 py-1 rounded-full text-xs font-medium border transition-all cursor-pointer flex-shrink-0 {collectionStore.activeSubTab === 'songs' ? 'bg-brand-border border-brand-border text-brand-accent font-semibold shadow-sm' : 'border-transparent text-brand-text-secondary/70 hover:text-brand-text-primary hover:bg-brand-sidebar'}"
      >
        Songs ({collectionStore.searchQuery.trim() !== "" ? collectionStore.filteredSongs.length : collectionStore.stats.total_songs})
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
            <option value="title-true">Sort: Title (A-Z)</option>
            <option value="title-false">Sort: Title (Z-A)</option>
            <option value="artist-true">Sort: Artist (A-Z)</option>
            <option value="artist-false">Sort: Artist (Z-A)</option>
            <option value="album-true">Sort: Album (A-Z)</option>
            <option value="album-false">Sort: Album (Z-A)</option>
            <option value="track-true">Sort: Track # (1-9)</option>
            <option value="track-false">Sort: Track # (9-1)</option>
            <option value="length_nanosec-true">Sort: Duration (Shortest)</option>
            <option value="length_nanosec-false">Sort: Duration (Longest)</option>
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
            <option value="album-true">Sort: Album Name (A-Z)</option>
            <option value="album-false">Sort: Album Name (Z-A)</option>
            <option value="artist-true">Sort: Artist Name (A-Z)</option>
            <option value="artist-false">Sort: Artist Name (Z-A)</option>
            <option value="year-false">Sort: Year (Newest)</option>
            <option value="year-true">Sort: Year (Oldest)</option>
            <option value="track_count-false">Sort: Tracks (Most)</option>
            <option value="track_count-true">Sort: Tracks (Least)</option>
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
            <option value="name-true">Sort: Artist Name (A-Z)</option>
            <option value="name-false">Sort: Artist Name (Z-A)</option>
            <option value="album_count-false">Sort: Albums (Most)</option>
            <option value="album_count-true">Sort: Albums (Least)</option>
            <option value="song_count-false">Sort: Tracks (Most)</option>
            <option value="song_count-true">Sort: Tracks (Least)</option>
          </select>
        </div>
      {/if}

      <div class="text-xs text-brand-text-secondary font-medium">
        {#if collectionStore.activeSubTab === "songs"}
          Showing {filteredSongs.length} tracks
        {:else if collectionStore.activeSubTab === "albums"}
          Showing {sortedAlbums.length} albums
        {:else}
          Showing {sortedArtists.length} artists
        {/if}
      </div>
    </div>
  </div>

  <!-- Main View Scrollable Container -->
  <div class="flex-1 p-6 {collectionStore.activeSubTab === 'songs' ? 'overflow-hidden flex flex-col' : 'overflow-y-auto'}">
    {#if collectionStore.activeSubTab === "songs"}
      <!-- Songs Table View -->
      <div class="flex-1 overflow-hidden border border-brand-border rounded-lg bg-brand-sidebar/40 flex flex-col min-h-0">
        <div class="sticky top-0 z-20 flex flex-col bg-brand-sidebar border-b border-brand-border text-xs text-brand-text-secondary uppercase tracking-wider font-semibold select-none">
          <div class="grid grid-cols-[36px_40px_2fr_1.5fr_1.5fr_96px_80px] items-center py-3 px-4">
            <div class="text-center w-9"></div>
            <button onclick={() => toggleSort("track")} class="text-left hover:text-brand-text-primary transition-colors flex items-center gap-1 cursor-pointer font-semibold uppercase tracking-wider">
              # {sortField === "track" ? (sortAsc ? "▲" : "▼") : ""}
            </button>
            <button onclick={() => toggleSort("title")} class="text-left hover:text-brand-text-primary transition-colors flex items-center gap-1 cursor-pointer font-semibold uppercase tracking-wider">
              Title {sortField === "title" ? (sortAsc ? "▲" : "▼") : ""}
            </button>
            <button onclick={() => toggleSort("artist")} class="text-left hover:text-brand-text-primary transition-colors flex items-center gap-1 cursor-pointer font-semibold uppercase tracking-wider">
              Artist {sortField === "artist" ? (sortAsc ? "▲" : "▼") : ""}
            </button>
            <button onclick={() => toggleSort("album")} class="text-left hover:text-brand-text-primary transition-colors flex items-center gap-1 cursor-pointer font-semibold uppercase tracking-wider">
              Album {sortField === "album" ? (sortAsc ? "▲" : "▼") : ""}
            </button>
            <button onclick={() => toggleSort("length_nanosec")} class="flex items-center justify-center hover:text-brand-text-primary transition-colors cursor-pointer font-semibold uppercase tracking-wider">
              <Clock class="w-4 h-4" /> {sortField === "length_nanosec" ? (sortAsc ? "▲" : "▼") : ""}
            </button>
            <div class="text-center">Actions</div>
          </div>
        </div>

        <div class="flex-1 min-h-0 relative">
          {#if filteredSongs.length === 0}
            <div class="py-16 text-center">
              <div class="flex flex-col items-center justify-center max-w-sm mx-auto p-6 bg-brand-sidebar/20 rounded-xl border border-dashed border-brand-border/60 select-none">
                <Music class="w-12 h-12 text-brand-accent/40 mb-3 animate-pulse" />
                <h3 class="text-base font-semibold text-brand-text-primary mb-1">No tracks found</h3>
                <p class="text-xs text-brand-text-secondary/60">
                  {#if collectionStore.searchQuery}
                    We couldn't find any songs matching "{collectionStore.searchQuery}". Try adjusting your keywords.
                  {:else}
                    Your music library is currently empty. Click "Rescan Library" or add a folder to get started.
                  {/if}
                </p>
                {#if collectionStore.searchQuery}
                  <button
                    onclick={() => { collectionStore.searchQuery = ""; collectionStore.search(""); }}
                    class="mt-4 px-3 py-1.5 text-xs bg-brand-accent hover:bg-brand-accent-hover text-brand-text-primary rounded-lg transition-colors font-medium cursor-pointer"
                  >
                    Clear Search Filter
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
                  {playerStore.currentSong && playerStore.currentSong.id === song.id ? 'bg-brand-accent/10 text-brand-accent-hover' : ''}"
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
                    class="absolute flex items-center justify-center opacity-0 group-hover:opacity-100 text-brand-accent hover:text-brand-accent-hover transition-all duration-150 cursor-pointer"
                    title="Play track"
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
                    class="hover:underline hover:text-brand-accent transition-all duration-150 text-left truncate cursor-pointer font-medium {playerStore.currentSong && playerStore.currentSong.id === song.id ? 'text-brand-accent-hover' : 'text-brand-text-primary'}"
                    title="Filter by title: {song.title || 'Unknown Title'}"
                  >
                    {song.title || "Unknown Title"}
                  </button>
                  <span class="px-1 py-0.5 text-[8px] font-semibold tracking-wider rounded uppercase bg-brand-sidebar text-brand-text-secondary border border-brand-border/50 shrink-0">
                    {song.filetype}
                  </span>
                  {#if song.lyrics && song.lyrics.trim() !== ""}
                    <span class="px-1 py-0.5 text-[8px] font-semibold tracking-wider rounded uppercase bg-brand-accent/10 text-brand-accent border border-brand-accent/20 shrink-0" title="Lyrics available">
                      LRC
                    </span>
                  {/if}
                </div>
                <div class="text-brand-text-secondary/90 truncate pr-4 min-w-0">
                  {#if song.artist}
                    <button
                      onclick={(e) => { e.stopPropagation(); collectionStore.navigateTo("collection", "songs", song.artist || ""); }}
                      class="hover:underline hover:text-brand-accent transition-all duration-150 text-left truncate cursor-pointer text-brand-text-secondary/90 text-left"
                      title="Filter by artist: {song.artist}"
                    >
                      {song.artist}
                    </button>
                  {:else}
                    <span class="text-brand-text-secondary/50">Unknown Artist</span>
                  {/if}
                </div>
                <div class="text-brand-text-secondary/70 truncate pr-4 min-w-0">
                  {#if song.album}
                    <button
                      onclick={(e) => { e.stopPropagation(); collectionStore.navigateTo("collection", "songs", song.album || ""); }}
                      class="hover:underline hover:text-brand-accent transition-all duration-150 text-left truncate cursor-pointer text-brand-text-secondary/70 text-left"
                      title="Filter by album: {song.album}"
                    >
                      {song.album}
                    </button>
                  {:else}
                    <span class="text-brand-text-secondary/50">Unknown Album</span>
                  {/if}
                </div>
                <div class="text-center text-brand-text-secondary/80">{formatDuration(song.length_nanosec)}</div>
                <div class="flex items-center justify-center gap-2.5">
                  <button
                    onclick={() => handleAddSongToPlaylist(song.id)}
                    class="text-brand-text-secondary/60 hover:text-brand-accent transition-colors cursor-pointer"
                    title="Add to active playlist"
                  >
                    <Plus class="w-4 h-4" />
                  </button>
                  <button
                    onclick={() => openTagEditor(song.id)}
                    class="text-brand-text-secondary/60 hover:text-brand-accent transition-colors cursor-pointer"
                    title="Edit tags"
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
            ondblclick={async () => {
              const albumName = album.album || "Unknown Album";
              const playlistName = `Album: ${albumName}`;
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
            <div class="aspect-square bg-brand-main rounded-lg mb-3 flex items-center justify-center text-brand-accent border border-brand-border overflow-hidden relative">
              <CoverArt
                songId={undefined}
                artEmbedded={album.art_embedded}
                artAutomatic={album.art_automatic}
                artManual={album.art_manual}
                sizeClass="w-full h-full"
              />
              <button
                onclick={async (e) => {
                  e.stopPropagation();
                  let songs = await invoke<Song[]>("get_songs_by_album", {
                    album: album.album || "",
                  });
                  // Filter out excluded formats
                  songs = songs.filter(song => !collectionStore.isFormatExcluded(song.filetype));
                  if (songs.length > 0) {
                    const songIds = songs.map((s) => s.id);
                    playerStore.playSongs(songIds, 0);
                  }
                }}
                class="absolute inset-0 bg-black/60 opacity-0 group-hover:opacity-100 flex items-center justify-center transition-opacity"
              >
                <div class="w-12 h-12 rounded-full bg-brand-accent text-brand-text-primary flex items-center justify-center scale-75 group-hover:scale-100 transition-transform">
                  <Play class="w-5 h-5 fill-current ml-0.5 text-white" />
                </div>
              </button>
            </div>
            <button
              onclick={(e) => { e.stopPropagation(); collectionStore.navigateTo("collection", "songs", album.album || ""); }}
              class="font-semibold text-sm text-brand-text-primary hover:text-brand-accent hover:underline transition-all duration-150 text-left truncate w-full cursor-pointer"
              title="Filter by album: {album.album || 'Unknown Album'}"
            >
              {album.album || "Unknown Album"}
            </button>
            <button
              onclick={(e) => { e.stopPropagation(); collectionStore.navigateTo("collection", "songs", album.artist || ""); }}
              class="text-xs text-brand-text-secondary hover:text-brand-accent hover:underline transition-all duration-150 text-left truncate w-full cursor-pointer mt-0.5"
              title="Filter by artist: {album.artist || 'Various Artists'}"
            >
              {album.artist || "Various Artists"}
            </button>
            <div class="flex items-center justify-between mt-2 text-[10px] text-brand-text-secondary/50">
              <span>{album.year || ""}</span>
              <span>{album.track_count} tracks</span>
            </div>
          </div>
        {/each}
        {#if sortedAlbums.length === 0}
          <div class="col-span-full py-16 text-center">
            <div class="flex flex-col items-center justify-center max-w-sm mx-auto p-6 bg-brand-sidebar/20 rounded-xl border border-dashed border-brand-border/60 select-none">
              <FolderClosed class="w-12 h-12 text-brand-accent/40 mb-3 animate-pulse" />
              <h3 class="text-base font-semibold text-brand-text-primary mb-1">No albums found</h3>
              <p class="text-xs text-brand-text-secondary/60 font-medium">No albums found in your library.</p>
            </div>
          </div>
        {/if}
      </div>
    {:else if collectionStore.activeSubTab === "artists"}
      <!-- Artists List Grid View -->
      <div class="grid grid-cols-[repeat(auto-fill,minmax(180px,1fr))] gap-6">
        {#each sortedArtists as artist}
          <div class="bg-brand-sidebar border border-brand-border/60 rounded-xl p-4 flex flex-col items-center text-center hover:border-brand-accent/40 transition-all duration-200">
            <div class="w-20 h-20 bg-gradient-to-br {getArtistGradient(artist.name)} rounded-full mb-3 flex items-center justify-center text-white border border-brand-border/40 font-bold text-2xl shadow-md">
              {artist.name ? artist.name.charAt(0).toUpperCase() : "?"}
            </div>
            <button
              onclick={(e) => { e.stopPropagation(); collectionStore.navigateTo("collection", "songs", artist.name || ""); }}
              class="font-semibold text-sm text-brand-text-primary hover:text-brand-accent hover:underline transition-all duration-150 text-center truncate w-full cursor-pointer"
              title="Filter by artist: {artist.name || 'Unknown Artist'}"
            >
              {artist.name || "Unknown Artist"}
            </button>
            <div class="flex gap-2 justify-center mt-2 text-[10px] text-brand-text-secondary/50">
              <span>{artist.album_count} albums</span>
              <span>•</span>
              <span>{artist.song_count} tracks</span>
            </div>
          </div>
        {/each}
        {#if sortedArtists.length === 0}
          <div class="col-span-full py-16 text-center">
            <div class="flex flex-col items-center justify-center max-w-sm mx-auto p-6 bg-brand-sidebar/20 rounded-xl border border-dashed border-brand-border/60 select-none">
              <Music class="w-12 h-12 text-brand-accent/40 mb-3 animate-pulse" />
              <h3 class="text-base font-semibold text-brand-text-primary mb-1">No artists found</h3>
              <p class="text-xs text-brand-text-secondary/60 font-medium">No artists found in your library.</p>
            </div>
          </div>
        {/if}
      </div>
    {/if}
  </div>
</div>

{#if editingSongId !== null}
  <TagEditor
    songId={editingSongId}
    onClose={() => { editingSongId = null; }}
    onSave={handleTagEditorSaved}
  />
{/if}

<style>
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
