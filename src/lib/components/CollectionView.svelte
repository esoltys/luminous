<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { collectionStore } from "../stores/collection.svelte";
  import { playerStore } from "../stores/player.svelte";
  import { playlistsStore } from "../stores/playlists.svelte";
  import CoverArt from "./CoverArt.svelte";
  import TagEditor from "./TagEditor.svelte";
  import { Search, Play, Plus, Clock, FileText, Music, FolderClosed, Edit3 } from "lucide-svelte";
  import type { Song, AlbumItem, ArtistItem } from "../types";

  let { activeSubTab = "songs", activeTab = $bindable() } = $props<{
    activeSubTab: "songs" | "albums" | "artists";
    activeTab?: "collection" | "playlists" | "settings" | "equalizer" | "lyrics";
  }>();

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

<div class="flex-1 flex flex-col overflow-hidden bg-brand-main text-brand-text-secondary h-full">
  <!-- Top bar with Search & Filter -->
  <div class="h-16 px-6 border-b border-brand-border flex items-center justify-between">
    <div class="relative w-80">
      <Search class="absolute left-3 top-2.5 w-4 h-4 text-brand-text-secondary/50" />
      <input
        type="text"
        placeholder="Search track, artist, album..."
        bind:value={searchQuery}
        oninput={handleSearch}
        class="w-full bg-brand-sidebar border border-brand-border rounded-lg pl-9 pr-4 py-1.5 text-sm focus:outline-none focus:border-brand-accent text-brand-text-primary"
      />
    </div>
    <div class="text-xs text-brand-text-secondary">
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
      <div class="w-full border border-brand-border rounded-lg overflow-hidden bg-brand-sidebar/40">
        <table class="w-full text-left text-sm border-collapse">
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
                  <div class="flex items-center gap-2">
                    <span class="truncate">{song.title || "Unknown Title"}</span>
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
                <td class="py-2.5 px-4 text-brand-text-secondary/90 truncate max-w-xs">{song.artist || "Unknown Artist"}</td>
                <td class="py-2.5 px-4 text-brand-text-secondary/70 truncate max-w-xs">{song.album || "Unknown Album"}</td>
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

              const songs = await invoke<Song[]>("get_songs_by_album", {
                albumArtist: album.artist || "",
                album: album.album || "",
              });

              if (songs.length > 0) {
                const songIds = songs.map((s) => s.id);
                if (playlistsStore.activePlaylistId !== null) {
                  await playlistsStore.addSongsToPlaylist(playlistsStore.activePlaylistId, songIds);
                  if (activeTab) {
                    activeTab = "playlists";
                  }
                  await playerStore.playPlaylistItem(playlistsStore.activePlaylistId, 0);
                }
              }
            }}
            class="bg-brand-sidebar border border-brand-border/60 rounded-xl p-4 flex flex-col group hover:border-brand-accent/40 transition-all duration-200 cursor-pointer select-none"
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
                <div class="w-12 h-12 rounded-full bg-brand-accent text-brand-text-primary flex items-center justify-center scale-75 group-hover:scale-100 transition-transform">
                  <Play class="w-5 h-5 fill-current ml-0.5 text-white" />
                </div>
              </button>
            </div>
            <h3 class="font-semibold text-sm text-brand-text-primary truncate w-full" title={album.album || "Unknown Album"}>
              {album.album || "Unknown Album"}
            </h3>
            <p class="text-xs text-brand-text-secondary truncate w-full" title={album.artist || "Unknown Artist"}>
              {album.artist || "Unknown Artist"}
            </p>
            <div class="flex items-center justify-between mt-2 text-[10px] text-brand-text-secondary/50">
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
          <div class="bg-brand-sidebar border border-brand-border/60 rounded-xl p-4 flex flex-col items-center text-center hover:border-brand-accent/40 transition-all duration-200">
            <div class="w-20 h-20 bg-gradient-to-br {getArtistGradient(artist.name)} rounded-full mb-3 flex items-center justify-center text-white border border-brand-border/40 font-bold text-2xl shadow-md">
              {artist.name ? artist.name.charAt(0).toUpperCase() : "?"}
            </div>
            <h3 class="font-semibold text-sm text-brand-text-primary truncate w-full" title={artist.name || "Unknown Artist"}>
              {artist.name || "Unknown Artist"}
            </h3>
            <div class="flex gap-2 justify-center mt-2 text-[10px] text-brand-text-secondary/50">
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

{#if editingSongId !== null}
  <TagEditor
    songId={editingSongId}
    onClose={() => { editingSongId = null; }}
    onSave={handleTagEditorSaved}
  />
{/if}
