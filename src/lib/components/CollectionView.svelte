<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { collectionStore } from "../stores/collection.svelte";
  import { playerStore } from "../stores/player.svelte";
  import { playlistsStore } from "../stores/playlists.svelte";
  import { Search, Play, Plus, Clock, FileText, Music, FolderClosed } from "lucide-svelte";
  import type { Song, AlbumItem, ArtistItem } from "../types";

  let { activeSubTab = "songs" } = $props<{ activeSubTab: "songs" | "albums" | "artists" }>();

  let searchQuery = $state("");
  let sortField = $state<keyof Song>("title");
  let sortAsc = $state(true);

  // Computed songs list with filtering and sorting
  let filteredSongs = $derived.by(() => {
    let result = searchQuery.trim() === "" ? collectionStore.songs : collectionStore.searchResults;

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

  function handleSearch(e: Event) {
    const query = (e.target as HTMLInputElement).value;
    collectionStore.search(query);
  }

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

<div class="flex-1 flex flex-col overflow-hidden bg-gray-950 text-gray-200 h-full">
  <!-- Top bar with Search & Filter -->
  <div class="h-16 px-6 border-b border-gray-800 flex items-center justify-between">
    <div class="relative w-80">
      <Search class="absolute left-3 top-2.5 w-4 h-4 text-gray-500" />
      <input
        type="text"
        placeholder="Search track, artist, album..."
        bind:value={searchQuery}
        oninput={handleSearch}
        class="w-full bg-gray-900 border border-gray-800 rounded-lg pl-9 pr-4 py-1.5 text-sm focus:outline-none focus:border-violet-500 text-gray-200"
      />
    </div>
    <div class="text-xs text-gray-400">
      {#if activeSubTab === "songs"}
        Showing {filteredSongs.length} tracks
      {:else if activeSubTab === "albums"}
        Showing {collectionStore.albums.length} albums
      {:else}
        Showing {collectionStore.artists.length} artists
      {/if}
    </div>
  </div>

  <!-- Main View Scrollable Container -->
  <div class="flex-1 overflow-y-auto p-6">
    {#if activeSubTab === "songs"}
      <!-- Songs Table View -->
      <div class="w-full border border-gray-800 rounded-lg overflow-hidden bg-gray-900/40">
        <table class="w-full text-left text-sm border-collapse">
          <thead>
            <tr class="border-b border-gray-800 bg-gray-900 text-xs text-gray-400 uppercase tracking-wider font-semibold">
              <th class="py-3 px-4 w-12 text-center"></th>
              <th onclick={() => toggleSort("title")} class="py-3 px-4 cursor-pointer hover:text-white select-none">
                Title {sortField === "title" ? (sortAsc ? "▲" : "▼") : ""}
              </th>
              <th onclick={() => toggleSort("artist")} class="py-3 px-4 cursor-pointer hover:text-white select-none">
                Artist {sortField === "artist" ? (sortAsc ? "▲" : "▼") : ""}
              </th>
              <th onclick={() => toggleSort("album")} class="py-3 px-4 cursor-pointer hover:text-white select-none">
                Album {sortField === "album" ? (sortAsc ? "▲" : "▼") : ""}
              </th>
              <th onclick={() => toggleSort("length_nanosec")} class="py-3 px-4 cursor-pointer hover:text-white select-none w-24">
                <Clock class="w-4 h-4 mx-auto" />
              </th>
              <th class="py-3 px-4 w-16 text-center">Actions</th>
            </tr>
          </thead>
          <tbody>
            {#each filteredSongs as song}
              <tr
                ondblclick={() => handlePlaySong(song)}
                class="border-b border-gray-800/40 hover:bg-gray-800/40 group transition-colors"
              >
                <td class="py-2.5 px-4 text-center">
                  <button
                    onclick={() => handlePlaySong(song)}
                    class="opacity-0 group-hover:opacity-100 text-violet-400 hover:text-violet-300 transition-opacity"
                  >
                    <Play class="w-4 h-4 fill-current" />
                  </button>
                </td>
                <td class="py-2.5 px-4 font-medium text-white truncate max-w-xs">
                  <div class="flex items-center gap-2">
                    <span class="truncate">{song.title || "Unknown Title"}</span>
                    <span class="px-1 py-0.5 text-[8px] font-semibold tracking-wider rounded uppercase bg-gray-800 text-gray-400 border border-gray-700/50 shrink-0">
                      {song.filetype}
                    </span>
                  </div>
                </td>
                <td class="py-2.5 px-4 text-gray-300 truncate max-w-xs">{song.artist || "Unknown Artist"}</td>
                <td class="py-2.5 px-4 text-gray-400 truncate max-w-xs">{song.album || "Unknown Album"}</td>
                <td class="py-2.5 px-4 text-center text-gray-400">{formatDuration(song.length_nanosec)}</td>
                <td class="py-2.5 px-4 text-center">
                  <button
                    onclick={() => handleAddSongToPlaylist(song.id)}
                    class="text-gray-400 hover:text-violet-400 transition-colors"
                    title="Add to active playlist"
                  >
                    <Plus class="w-4 h-4" />
                  </button>
                </td>
              </tr>
            {/each}
            {#if filteredSongs.length === 0}
              <tr>
                <td colspan="6" class="py-12 text-center text-gray-500">
                  <Music class="w-12 h-12 mx-auto mb-2 text-gray-600" />
                  No songs in library or matching search.
                </td>
              </tr>
            {/if}
          </tbody>
        </table>
      </div>
    {:else if activeSubTab === "albums"}
      <!-- Albums Card Grid View -->
      <div class="grid grid-cols-2 md:grid-cols-4 lg:grid-cols-5 xl:grid-cols-6 gap-6">
        {#each collectionStore.albums as album}
          <div class="bg-gray-900 border border-gray-800/60 rounded-xl p-4 flex flex-col group hover:border-violet-500/40 transition-all duration-200">
            <div class="aspect-square bg-gray-950 rounded-lg mb-3 flex items-center justify-center text-violet-400 border border-gray-800 overflow-hidden relative">
              <FolderClosed class="w-12 h-12" />
              <button
                onclick={async () => {
                  const songs = await invoke<Song[]>("get_songs_by_album", {
                    albumArtist: album.artist || "",
                    album: album.album || "",
                  });
                  if (songs.length > 0) {
                    const songIds = songs.map((s) => s.id);
                    playerStore.playSongs(songIds, 0);
                  }
                }}
                class="absolute inset-0 bg-black/60 opacity-0 group-hover:opacity-100 flex items-center justify-center transition-opacity"
              >
                <div class="w-12 h-12 rounded-full bg-violet-600 text-white flex items-center justify-center scale-75 group-hover:scale-100 transition-transform">
                  <Play class="w-5 h-5 fill-current ml-0.5" />
                </div>
              </button>
            </div>
            <h3 class="font-semibold text-sm text-gray-100 truncate w-full" title={album.album || "Unknown Album"}>
              {album.album || "Unknown Album"}
            </h3>
            <p class="text-xs text-gray-400 truncate w-full" title={album.artist || "Unknown Artist"}>
              {album.artist || "Unknown Artist"}
            </p>
            <div class="flex items-center justify-between mt-2 text-[10px] text-gray-500">
              <span>{album.year || ""}</span>
              <span>{album.track_count} tracks</span>
            </div>
          </div>
        {/each}
        {#if collectionStore.albums.length === 0}
          <div class="col-span-full py-12 text-center text-gray-500">
            <FolderClosed class="w-12 h-12 mx-auto mb-2 text-gray-600" />
            No albums found.
          </div>
        {/if}
      </div>
    {:else if activeSubTab === "artists"}
      <!-- Artists List Grid View -->
      <div class="grid grid-cols-2 md:grid-cols-4 lg:grid-cols-5 xl:grid-cols-6 gap-6">
        {#each collectionStore.artists as artist}
          <div class="bg-gray-900 border border-gray-800/60 rounded-xl p-4 flex flex-col items-center text-center hover:border-violet-500/40 transition-all duration-200">
            <div class="w-20 h-20 bg-gray-950 rounded-full mb-3 flex items-center justify-center text-violet-400 border border-gray-800">
              <Music class="w-8 h-8" />
            </div>
            <h3 class="font-semibold text-sm text-gray-100 truncate w-full" title={artist.name || "Unknown Artist"}>
              {artist.name || "Unknown Artist"}
            </h3>
            <div class="flex gap-2 justify-center mt-2 text-[10px] text-gray-500">
              <span>{artist.album_count} albums</span>
              <span>•</span>
              <span>{artist.song_count} tracks</span>
            </div>
          </div>
        {/each}
        {#if collectionStore.artists.length === 0}
          <div class="col-span-full py-12 text-center text-gray-500">
            <Music class="w-12 h-12 mx-auto mb-2 text-gray-600" />
            No artists found.
          </div>
        {/if}
      </div>
    {/if}
  </div>
</div>
