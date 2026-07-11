<script lang="ts">
  import { playlistsStore } from "../stores/playlists.svelte";
  import { playerStore } from "../stores/player.svelte";
  import { collectionStore } from "../stores/collection.svelte";
  import { Trash2, ListMusic, RotateCcw, RotateCw, Edit3, AlertTriangle, Play } from "lucide-svelte";
  import { getCoverArtUrl } from "../types";
  import type { PlaylistItem } from "../types";
  import TagEditor from "./TagEditor.svelte";

  let editingSongId = $state<number | null>(null);

  function openTagEditor(songId: number) {
    editingSongId = songId;
  }

  function handleTagEditorSaved() {
    if (playlistsStore.activePlaylistId !== null) {
      playlistsStore.selectPlaylist(playlistsStore.activePlaylistId);
    }
  }

  // Selected playlist from the store
  let activePlaylist = $derived(
    playlistsStore.playlists.find((p) => p.id === playlistsStore.activePlaylistId)
  );

  function handlePlayPlaylistItem(index: number) {
    // Don't attempt to play unavailable tracks
    const item = playlistsStore.activePlaylistTracks[index];
    if (!item || isItemUnavailable(item)) return;
    if (playlistsStore.activePlaylistId !== null) {
      playerStore.playPlaylistItem(playlistsStore.activePlaylistId, index);
    }
  }

  /** Returns true if the item's song is missing from disk or has no song data. */
  function isItemUnavailable(item: PlaylistItem): boolean {
    return !item.song || item.song.unavailable === true;
  }

  /** Remove all playlist items whose song is unavailable. */
  function removeUnavailableTracks() {
    if (playlistsStore.activePlaylistId === null) return;
    const uuids = playlistsStore.activePlaylistTracks
      .filter((item) => isItemUnavailable(item))
      .map((item) => item.uuid);
    if (uuids.length > 0) {
      playlistsStore.removeItemsFromPlaylist(playlistsStore.activePlaylistId, uuids);
    }
  }

  /** Count of unavailable tracks in the active playlist. */
  let unavailableCount = $derived(
    playlistsStore.activePlaylistTracks.filter((item) => isItemUnavailable(item)).length
  );

  function handleRemoveItem(uuid: string) {
    if (playlistsStore.activePlaylistId !== null) {
      playlistsStore.removeItemsFromPlaylist(playlistsStore.activePlaylistId, [uuid]);
    }
  }

  // Drag and drop state and handlers
  let draggedIndex = $state<number | null>(null);
  let dragOverIndex = $state<number | null>(null);

  function handleDragStart(event: DragEvent, index: number) {
    // Defer setting draggedIndex to prevent Chrome/Webkit from immediately canceling the drag due to synchronous DOM updates
    setTimeout(() => {
      draggedIndex = index;
    }, 0);
    if (event.dataTransfer) {
      event.dataTransfer.effectAllowed = "move";
      event.dataTransfer.setData("text/plain", index.toString());
    }
  }

  function handleDragOver(event: DragEvent, index: number) {
    if (draggedIndex === null) return;
    event.preventDefault();
    if (event.dataTransfer) {
      event.dataTransfer.dropEffect = "move";
    }
  }

  function handleDragEnter(event: DragEvent, index: number) {
    if (draggedIndex === null) return;
    event.preventDefault();
    dragOverIndex = index;
  }

  function handleDragLeave(index: number) {
    if (dragOverIndex === index) {
      dragOverIndex = null;
    }
  }

  function handleDragEnd() {
    draggedIndex = null;
    dragOverIndex = null;
  }

  function handleDrop(event: DragEvent, index: number) {
    event.preventDefault();
    if (draggedIndex !== null && draggedIndex !== index) {
      if (playlistsStore.activePlaylistId !== null) {
        playlistsStore.reorderItem(playlistsStore.activePlaylistId, draggedIndex, index);
      }
    }
    draggedIndex = null;
    dragOverIndex = null;
  }

  function formatDuration(ns: number | undefined): string {
    if (!ns) return "0:00";
    const sec = Math.floor(ns / 1_000_000_000);
    const m = Math.floor(sec / 60);
    const s = sec % 60;
    return `${m}:${s < 10 ? "0" : ""}${s}`;
  }

  let currentCoverUrl = $derived.by(() => {
    const song = playerStore.currentSong;
    if (!song) return null;
    if (song.art_manual) {
      return getCoverArtUrl(`luminous-art://${song.art_manual}`);
    }
    if (song.art_automatic) {
      if (song.art_automatic.startsWith("album-")) {
        return getCoverArtUrl(`luminous-art://${song.art_automatic}`);
      } else {
        return getCoverArtUrl(`luminous-art://local/${song.art_automatic}`);
      }
    }
    return null;
  });
</script>

<div class="flex-1 flex flex-col overflow-hidden bg-brand-main text-brand-text-secondary h-full relative">
  {#if currentCoverUrl}
    <div class="absolute inset-0 pointer-events-none select-none z-0 overflow-hidden">
      <div 
        class="absolute inset-0 bg-cover bg-center opacity-[0.12] scale-105 blur-[60px] saturate-[180%] transition-all duration-1000"
        style="background-image: url('{currentCoverUrl}');"
      ></div>
      <div class="absolute inset-0 bg-gradient-to-t from-brand-main via-transparent to-brand-main/20"></div>
    </div>
  {/if}

  {#if activePlaylist}
    <!-- Top Header bar -->
    <div class="h-16 px-6 border-b border-brand-border flex items-center justify-between relative z-10 bg-brand-main/40 backdrop-blur-md">
      <div class="flex items-center gap-3">
        <ListMusic class="w-5 h-5 text-brand-accent" />
        <h2 class="text-base font-bold text-brand-text-primary">{activePlaylist.name}</h2>
        <span class="text-xs text-brand-text-secondary/60 font-medium">({playlistsStore.activePlaylistTracks.length} tracks)</span>
      </div>

      <div class="flex items-center gap-3">
        <!-- Undo/Redo controls -->
        <button
          onclick={() => playlistsStore.undo()}
          class="p-1.5 rounded hover:bg-brand-sidebar text-brand-text-secondary hover:text-brand-text-primary transition-colors"
          title="Undo last playlist operation"
        >
          <RotateCcw class="w-4 h-4" />
        </button>
        <button
          onclick={() => playlistsStore.redo()}
          class="p-1.5 rounded hover:bg-brand-sidebar text-brand-text-secondary hover:text-brand-text-primary transition-colors"
          title="Redo last playlist operation"
        >
          <RotateCw class="w-4 h-4" />
        </button>
        {#if unavailableCount > 0}
          <button
            onclick={removeUnavailableTracks}
            class="flex items-center gap-1.5 bg-amber-500/10 hover:bg-amber-500/20 border border-amber-500/30 text-amber-400 hover:text-amber-300 px-3 py-1 text-xs font-semibold rounded transition-colors cursor-pointer"
            title="Remove all {unavailableCount} unavailable track{unavailableCount === 1 ? '' : 's'} from playlist"
          >
            <AlertTriangle class="w-3.5 h-3.5" />
            Remove {unavailableCount} unavailable
          </button>
        {/if}
        <button
          onclick={() => playlistsStore.clearPlaylist(activePlaylist.id)}
          class="bg-brand-sidebar hover:bg-brand-main border border-brand-border text-brand-text-primary px-3 py-1 text-xs font-semibold rounded transition-colors cursor-pointer"
        >
          Clear Playlist
        </button>
      </div>
    </div>

    <!-- Tracks List Scrollable Container -->
    <div class="flex-1 overflow-hidden p-6 relative z-10 flex flex-col">
      <div class="flex-1 overflow-auto border border-brand-border rounded-lg bg-brand-sidebar/30 backdrop-blur-md">
        <table class="w-full text-left text-sm border-collapse min-w-[800px]">
          <thead>
            <tr class="text-xs text-brand-text-secondary uppercase tracking-wider font-semibold">
              <th class="sticky top-0 bg-brand-sidebar border-b border-brand-border py-3 px-4 w-12 text-center z-10">#</th>
              <th class="sticky top-0 bg-brand-sidebar border-b border-brand-border py-3 px-4 z-10">Title</th>
              <th class="sticky top-0 bg-brand-sidebar border-b border-brand-border py-3 px-4 z-10">Artist</th>
              <th class="sticky top-0 bg-brand-sidebar border-b border-brand-border py-3 px-4 z-10">Album</th>
              <th class="sticky top-0 bg-brand-sidebar border-b border-brand-border py-3 px-4 w-24 text-center z-10">Duration</th>
              <th class="sticky top-0 bg-brand-sidebar border-b border-brand-border py-3 px-4 w-20 text-center z-10">Actions</th>
            </tr>
          </thead>
          <tbody>
            {#each playlistsStore.activePlaylistTracks as item, index}
              {@const unavailable = isItemUnavailable(item)}
              <tr
                draggable={!unavailable}
                ondragstart={(e) => !unavailable && handleDragStart(e, index)}
                ondragover={(e) => handleDragOver(e, index)}
                ondragenter={(e) => handleDragEnter(e, index)}
                ondragleave={() => handleDragLeave(index)}
                ondragend={handleDragEnd}
                ondrop={(e) => handleDrop(e, index)}
                ondblclick={() => !unavailable && handlePlayPlaylistItem(index)}
                class="border-b border-brand-border/40 group transition-all duration-150 select-none
                  {unavailable
                    ? 'opacity-50 cursor-not-allowed'
                    : 'hover:bg-brand-sidebar/40 cursor-grab active:cursor-grabbing'}
                  {!unavailable && playerStore.playlistItemUuid === item.uuid ? 'bg-brand-accent/10 text-brand-accent-hover' : ''}
                  {draggedIndex === index ? 'opacity-40 bg-brand-sidebar/20' : ''}
                  {dragOverIndex === index && draggedIndex !== null && draggedIndex !== index
                    ? (index < draggedIndex ? 'border-t-2! border-t-brand-accent bg-brand-accent/5' : 'border-b-2! border-b-brand-accent bg-brand-accent/5')
                    : ''
                  }"
              >
                <td class="py-2.5 px-4 text-center text-brand-text-secondary/50 font-medium w-12 relative">
                  <div class="relative w-4 h-4 mx-auto flex items-center justify-center">
                    {#if playerStore.playlistItemUuid === item.uuid && playerStore.state === 'playing'}
                      <div class="flex items-center justify-center gap-0.5 h-4 w-4 absolute inset-0 group-hover:opacity-0 transition-opacity">
                        <span class="w-0.5 bg-brand-accent animate-bounce h-full" style="animation-delay: 0.1s"></span>
                        <span class="w-0.5 bg-brand-accent animate-bounce h-2/3" style="animation-delay: 0.2s"></span>
                        <span class="w-0.5 bg-brand-accent animate-bounce h-full" style="animation-delay: 0.3s"></span>
                      </div>
                    {:else}
                      <span class="absolute inset-0 flex items-center justify-center group-hover:opacity-0 transition-opacity">
                        {index + 1}
                      </span>
                    {/if}
                    <button
                      onclick={() => !unavailable && handlePlayPlaylistItem(index)}
                      class="absolute inset-0 flex items-center justify-center opacity-0 group-hover:opacity-100 text-brand-accent hover:text-brand-accent-hover transition-opacity cursor-pointer disabled:opacity-0 disabled:cursor-not-allowed"
                      disabled={unavailable}
                      title="Play track"
                    >
                      <Play class="w-4 h-4 fill-current" />
                    </button>
                  </div>
                </td>
                <td class="py-2.5 px-4 font-medium truncate max-w-xs {!unavailable && playerStore.playlistItemUuid === item.uuid ? 'text-brand-accent-hover' : unavailable ? 'text-brand-text-secondary/50' : 'text-brand-text-primary'}">
                  <div class="flex items-center gap-2 max-w-full">
                    {#if unavailable}
                      <!-- Unavailable track: show warning icon + last known title -->
                      <span title="File not found on disk">
                        <AlertTriangle class="w-3.5 h-3.5 shrink-0 text-amber-400/80" />
                      </span>
                      <span class="truncate line-through decoration-brand-text-secondary/40">
                        {item.song?.title ?? "Unknown Title"}
                      </span>
                    {:else if item.song?.title}
                      <button
                        onclick={(e) => { e.stopPropagation(); collectionStore.navigateTo("collection", "songs", item.song?.title || ""); }}
                        class="hover:underline hover:text-brand-accent transition-all duration-150 text-left truncate cursor-pointer font-medium {playerStore.playlistItemUuid === item.uuid ? 'text-brand-accent-hover' : 'text-brand-text-primary'}"
                        title="Filter by title: {item.song.title}"
                      >
                        {item.song.title}
                      </button>
                    {:else}
                      <span class="truncate">Unknown Title</span>
                    {/if}
                    {#if item.song && !unavailable}
                      <span class="px-1 py-0.5 text-[8px] font-semibold tracking-wider rounded uppercase bg-brand-sidebar text-brand-text-secondary border border-brand-border/50 shrink-0">
                        {item.song.filetype}
                      </span>
                      {#if item.song.lyrics && item.song.lyrics.trim() !== ""}
                        <span class="px-1 py-0.5 text-[8px] font-semibold tracking-wider rounded uppercase bg-brand-accent/10 text-brand-accent border border-brand-accent/20 shrink-0" title="Lyrics available">
                          LRC
                        </span>
                      {/if}
                    {/if}
                  </div>
                </td>
                <td class="py-2.5 px-4 text-brand-text-secondary/90 truncate max-w-xs">
                  {#if unavailable}
                    <span class="text-brand-text-secondary/40 italic text-xs">File not found</span>
                  {:else if item.song?.artist}
                    <button
                      onclick={(e) => { e.stopPropagation(); collectionStore.navigateTo("collection", "artists", item.song?.artist || ""); }}
                      class="hover:underline hover:text-brand-accent transition-all duration-150 text-left truncate cursor-pointer text-brand-text-secondary/90"
                      title="Filter by artist: {item.song.artist}"
                    >
                      {item.song.artist}
                    </button>
                  {:else}
                    <span class="text-brand-text-secondary/50">Unknown Artist</span>
                  {/if}
                </td>
                <td class="py-2.5 px-4 text-brand-text-secondary/70 truncate max-w-xs">
                  {#if unavailable}
                    <span class="text-brand-text-secondary/40 italic text-xs">{item.song?.album ?? ""}</span>
                  {:else if item.song?.album}
                    <button
                      onclick={(e) => { e.stopPropagation(); collectionStore.navigateTo("collection", "albums", item.song?.album || ""); }}
                      class="hover:underline hover:text-brand-accent transition-all duration-150 text-left truncate cursor-pointer text-brand-text-secondary/70"
                      title="Filter by album: {item.song.album}"
                    >
                      {item.song.album}
                    </button>
                  {:else}
                    <span class="text-brand-text-secondary/50">Unknown Album</span>
                  {/if}
                </td>
                <td class="py-2.5 px-4 text-center text-brand-text-secondary/80">{formatDuration(item.song?.length_nanosec)}</td>
                <td class="py-2.5 px-4 text-center">
                  <div class="flex items-center justify-center gap-2.5">
                    <button
                      onclick={() => item.song?.id && !unavailable && openTagEditor(item.song.id)}
                      class="text-brand-text-secondary/60 hover:text-brand-accent transition-colors disabled:opacity-30"
                      title="Edit tags"
                      disabled={!item.song || unavailable}
                    >
                      <Edit3 class="w-4 h-4" />
                    </button>
                    <button
                      onclick={() => handleRemoveItem(item.uuid)}
                      class="text-brand-text-secondary/60 hover:text-red-400 transition-colors"
                      title="Remove from playlist"
                    >
                      <Trash2 class="w-4 h-4" />
                    </button>
                  </div>
                </td>
              </tr>
            {/each}
            {#if playlistsStore.activePlaylistTracks.length === 0}
              <tr>
                <td colspan="6" class="py-12 text-center text-brand-text-secondary/45">
                  <ListMusic class="w-12 h-12 mx-auto mb-2 text-brand-text-secondary/30" />
                  Playlist is empty. Add songs from the Collection tab.
                </td>
              </tr>
            {/if}
          </tbody>
        </table>
      </div>
    </div>
  {:else}
    <!-- No playlist selected -->
    <div class="flex-1 flex flex-col items-center justify-center text-brand-text-secondary/60">
      <ListMusic class="w-16 h-16 mb-4 text-brand-text-secondary/30" />
      <h2 class="text-lg font-bold text-brand-text-secondary/80 mb-1">No Playlist Selected</h2>
      <p class="text-sm">Create or select a playlist from the sidebar to start.</p>
    </div>
  {/if}
</div>

{#if editingSongId !== null}
  <TagEditor
    songId={editingSongId}
    onClose={() => { editingSongId = null; }}
    onSave={handleTagEditorSaved}
  />
{/if}
