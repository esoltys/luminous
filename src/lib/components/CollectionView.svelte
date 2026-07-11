<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { collectionStore } from "../stores/collection.svelte";
  import { playerStore } from "../stores/player.svelte";
  import { playlistsStore } from "../stores/playlists.svelte";
  import CoverArt from "./CoverArt.svelte";
  import TagEditor from "./TagEditor.svelte";
  import { Play, Plus, Clock, FileText, Music, FolderClosed, Edit3 } from "lucide-svelte";
  import type { Song, AlbumItem, ArtistItem } from "../types";

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

  let sortField = $state<keyof Song>("title");
  let sortAsc = $state(true);

  // Trigger search on collectionStore when query changes
  $effect(() => {
    collectionStore.search(collectionStore.searchQuery);
  });

  // Computed songs list with filtering and sorting
  let filteredSongs = $derived.by(() => {
    let result = collectionStore.searchQuery.trim() === "" ? collectionStore.songs : collectionStore.searchResults;

    // Filter by excluded formats
    result = result.filter(song => !collectionStore.isFormatExcluded(song.filetype));

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

  // Computed albums list with search filtering
  let filteredAlbums = $derived.by(() => {
    const query = collectionStore.searchQuery.trim().toLowerCase();
    if (query === "") return collectionStore.albums;
    return collectionStore.albums.filter(album => 
      (album.album && album.album.toLowerCase().includes(query)) ||
      (album.artist && album.artist.toLowerCase().includes(query))
    );
  });

  // Computed artists list with search filtering
  let filteredArtists = $derived.by(() => {
    const query = collectionStore.searchQuery.trim().toLowerCase();
    if (query === "") return collectionStore.artists;
    return collectionStore.artists.filter(artist => 
      artist.name && artist.name.toLowerCase().includes(query)
    );
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
  <div class="h-12 px-6 border-b border-brand-border flex items-center justify-end">
    <div class="text-xs text-brand-text-secondary">
      {#if collectionStore.activeSubTab === "songs"}
        Showing {filteredSongs.length} tracks
      {:else if collectionStore.activeSubTab === "albums"}
        Showing {filteredAlbums.length} albums
      {:else}
        Showing {filteredArtists.length} artists
      {/if}
    </div>
  </div>

  <!-- Main View Scrollable Container -->
  <div class="flex-1 overflow-y-auto p-6">
    {#if collectionStore.activeSubTab === "songs"}
      <!-- Songs Table View -->
      <div class="w-full border border-brand-border rounded-lg overflow-x-auto bg-brand-sidebar/40">
        <table class="w-full text-left text-sm border-collapse min-w-[800px]">
          <thead>
            <tr class="text-xs text-brand-text-secondary uppercase tracking-wider font-semibold">
              <th class="sticky top-0 bg-brand-sidebar border-b border-brand-border py-3 px-4 w-12 text-center z-10"></th>
              <th onclick={() => toggleSort("title")} class="sticky top-0 bg-brand-sidebar border-b border-brand-border py-3 px-4 cursor-pointer hover:text-brand-text-primary select-none z-10">
                Title {sortField === "title" ? (sortAsc ? "▲" : "▼") : ""}
              </th>
              <th onclick={() => toggleSort("artist")} class="sticky top-0 bg-brand-sidebar border-b border-brand-border py-3 px-4 cursor-pointer hover:text-brand-text-primary select-none z-10">
                Artist {sortField === "artist" ? (sortAsc ? "▲" : "▼") : ""}
              </th>
              <th onclick={() => toggleSort("album")} class="sticky top-0 bg-brand-sidebar border-b border-brand-border py-3 px-4 cursor-pointer hover:text-brand-text-primary select-none z-10">
                Album {sortField === "album" ? (sortAsc ? "▲" : "▼") : ""}
              </th>
              <th onclick={() => toggleSort("length_nanosec")} class="sticky top-0 bg-brand-sidebar border-b border-brand-border py-3 px-4 cursor-pointer hover:text-brand-text-primary select-none w-24 z-10">
                <Clock class="w-4 h-4 mx-auto" />
              </th>
              <th class="sticky top-0 bg-brand-sidebar border-b border-brand-border py-3 px-4 w-16 text-center z-10">Actions</th>
            </tr>
          </thead>
          <tbody>
            {#each filteredSongs as song}
              <tr
                ondblclick={() => handlePlaySong(song)}
                class="border-b border-brand-border/40 hover:bg-brand-sidebar/40 group transition-colors"
              >
                <td class="py-2.5 px-4 text-center">
                  <button
                    onclick={() => handlePlaySong(song)}
                    class="opacity-0 group-hover:opacity-100 text-brand-accent hover:text-brand-accent-hover transition-opacity"
                  >
                    <Play class="w-4 h-4 fill-current" />
                  </button>
                </td>
                <td class="py-2.5 px-4 font-medium text-brand-text-primary truncate max-w-xs">
                  <div class="flex items-center gap-2 max-w-full">
                    <button
                      onclick={(e) => { e.stopPropagation(); collectionStore.navigateTo("collection", "songs", song.title || ""); }}
                      class="hover:underline hover:text-brand-accent transition-all duration-150 text-left truncate cursor-pointer font-medium text-brand-text-primary"
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
                </td>
                <td class="py-2.5 px-4 text-brand-text-secondary/90 truncate max-w-xs">
                  {#if song.artist}
                    <button
                      onclick={(e) => { e.stopPropagation(); collectionStore.navigateTo("collection", "artists", song.artist || ""); }}
                      class="hover:underline hover:text-brand-accent transition-all duration-150 text-left truncate cursor-pointer text-brand-text-secondary/90"
                      title="Filter by artist: {song.artist}"
                    >
                      {song.artist}
                    </button>
                  {:else}
                    <span class="text-brand-text-secondary/50">Unknown Artist</span>
                  {/if}
                </td>
                <td class="py-2.5 px-4 text-brand-text-secondary/70 truncate max-w-xs">
                  {#if song.album}
                    <button
                      onclick={(e) => { e.stopPropagation(); collectionStore.navigateTo("collection", "albums", song.album || ""); }}
                      class="hover:underline hover:text-brand-accent transition-all duration-150 text-left truncate cursor-pointer text-brand-text-secondary/70"
                      title="Filter by album: {song.album}"
                    >
                      {song.album}
                    </button>
                  {:else}
                    <span class="text-brand-text-secondary/50">Unknown Album</span>
                  {/if}
                </td>
                <td class="py-2.5 px-4 text-center text-brand-text-secondary/80">{formatDuration(song.length_nanosec)}</td>
                <td class="py-2.5 px-4 text-center">
                  <div class="flex items-center justify-center gap-2.5">
                    <button
                      onclick={() => handleAddSongToPlaylist(song.id)}
                      class="text-brand-text-secondary/60 hover:text-brand-accent transition-colors"
                      title="Add to active playlist"
                    >
                      <Plus class="w-4 h-4" />
                    </button>
                    <button
                      onclick={() => openTagEditor(song.id)}
                      class="text-brand-text-secondary/60 hover:text-brand-accent transition-colors"
                      title="Edit tags"
                    >
                      <Edit3 class="w-4 h-4" />
                    </button>
                  </div>
                </td>
              </tr>
            {/each}
            {#if filteredSongs.length === 0}
              <tr>
                <td colspan="6" class="py-16 text-center">
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
                </td>
              </tr>
            {/if}
          </tbody>
        </table>
      </div>
    {:else if collectionStore.activeSubTab === "albums"}
      <!-- Albums Card Grid View -->
      <div class={filteredAlbums.length <= 3 ? "flex flex-row flex-wrap gap-6" : "grid grid-cols-[repeat(auto-fill,minmax(180px,1fr))] gap-6"}>
        {#each filteredAlbums as album}
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
            class="{filteredAlbums.length <= 3 ? 'w-48 shrink-0' : 'w-full'} bg-brand-sidebar border border-brand-border/60 rounded-xl p-4 flex flex-col group hover:border-brand-accent/40 transition-all duration-200 cursor-pointer select-none"
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
              onclick={(e) => { e.stopPropagation(); collectionStore.navigateTo("collection", "albums", album.album || ""); }}
              class="font-semibold text-sm text-brand-text-primary hover:text-brand-accent hover:underline transition-all duration-150 text-left truncate w-full cursor-pointer"
              title="Filter by album: {album.album || 'Unknown Album'}"
            >
              {album.album || "Unknown Album"}
            </button>
            <button
              onclick={(e) => { e.stopPropagation(); collectionStore.navigateTo("collection", "artists", album.artist || ""); }}
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
        {#if filteredAlbums.length === 0}
          <div class="col-span-full py-16 text-center">
            <div class="flex flex-col items-center justify-center max-w-sm mx-auto p-6 bg-brand-sidebar/20 rounded-xl border border-dashed border-brand-border/60 select-none">
              <FolderClosed class="w-12 h-12 text-brand-accent/40 mb-3 animate-pulse" />
              <h3 class="text-base font-semibold text-brand-text-primary mb-1">No albums found</h3>
              <p class="text-xs text-brand-text-secondary/60">
                {#if collectionStore.searchQuery}
                  No albums match your search query "{collectionStore.searchQuery}".
                {:else}
                  No albums found in your library.
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
        {/if}
      </div>
    {:else if collectionStore.activeSubTab === "artists"}
      <!-- Artists List Grid View -->
      <div class="grid grid-cols-[repeat(auto-fill,minmax(180px,1fr))] gap-6">
        {#each filteredArtists as artist}
          <div class="bg-brand-sidebar border border-brand-border/60 rounded-xl p-4 flex flex-col items-center text-center hover:border-brand-accent/40 transition-all duration-200">
            <div class="w-20 h-20 bg-gradient-to-br {getArtistGradient(artist.name)} rounded-full mb-3 flex items-center justify-center text-white border border-brand-border/40 font-bold text-2xl shadow-md">
              {artist.name ? artist.name.charAt(0).toUpperCase() : "?"}
            </div>
            <button
              onclick={(e) => { e.stopPropagation(); collectionStore.navigateTo("collection", "artists", artist.name || ""); }}
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
        {#if filteredArtists.length === 0}
          <div class="col-span-full py-16 text-center">
            <div class="flex flex-col items-center justify-center max-w-sm mx-auto p-6 bg-brand-sidebar/20 rounded-xl border border-dashed border-brand-border/60 select-none">
              <Music class="w-12 h-12 text-brand-accent/40 mb-3 animate-pulse" />
              <h3 class="text-base font-semibold text-brand-text-primary mb-1">No artists found</h3>
              <p class="text-xs text-brand-text-secondary/60">
                {#if collectionStore.searchQuery}
                  No artists match your search query "{collectionStore.searchQuery}".
                {:else}
                  No artists found in your library.
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
