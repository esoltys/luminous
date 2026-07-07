<script lang="ts">
  import { playlistsStore } from "../stores/playlists.svelte";
  import { playerStore } from "../stores/player.svelte";
  import { Play, Trash2, ArrowUp, ArrowDown, ListMusic, RotateCcw, RotateCw } from "lucide-svelte";
  import type { PlaylistItem } from "../types";

  // Selected playlist from the store
  let activePlaylist = $derived(
    playlistsStore.playlists.find((p) => p.id === playlistsStore.activePlaylistId)
  );

  function handlePlayPlaylistItem(index: number) {
    if (playlistsStore.activePlaylistId !== null) {
      playerStore.playPlaylistItem(playlistsStore.activePlaylistId, index);
    }
  }

  function handleRemoveItem(uuid: string) {
    if (playlistsStore.activePlaylistId !== null) {
      playlistsStore.removeItemsFromPlaylist(playlistsStore.activePlaylistId, [uuid]);
    }
  }

  function handleMoveUp(index: number) {
    if (playlistsStore.activePlaylistId !== null && index > 0) {
      playlistsStore.reorderItem(playlistsStore.activePlaylistId, index, index - 1);
    }
  }

  function handleMoveDown(index: number) {
    if (
      playlistsStore.activePlaylistId !== null &&
      index < playlistsStore.activePlaylistTracks.length - 1
    ) {
      playlistsStore.reorderItem(playlistsStore.activePlaylistId, index, index + 1);
    }
  }

  function formatDuration(ns: number | undefined): string {
    if (!ns) return "0:00";
    const sec = Math.floor(ns / 1_000_000_000);
    const m = Math.floor(sec / 60);
    const s = sec % 60;
    return `${m}:${s < 10 ? "0" : ""}${s}`;
  }
</script>

<div class="flex-1 flex flex-col overflow-hidden bg-gray-950 text-gray-200 h-full">
  {#if activePlaylist}
    <!-- Top Header bar -->
    <div class="h-16 px-6 border-b border-gray-800 flex items-center justify-between">
      <div class="flex items-center gap-3">
        <ListMusic class="w-5 h-5 text-violet-400" />
        <h2 class="text-base font-bold text-white">{activePlaylist.name}</h2>
        <span class="text-xs text-gray-500 font-medium">({playlistsStore.activePlaylistTracks.length} tracks)</span>
      </div>

      <div class="flex items-center gap-3">
        <!-- Undo/Redo controls -->
        <button
          onclick={() => playlistsStore.undo()}
          class="p-1.5 rounded hover:bg-gray-800 text-gray-400 hover:text-white transition-colors"
          title="Undo last playlist operation"
        >
          <RotateCcw class="w-4 h-4" />
        </button>
        <button
          onclick={() => playlistsStore.redo()}
          class="p-1.5 rounded hover:bg-gray-800 text-gray-400 hover:text-white transition-colors"
          title="Redo last playlist operation"
        >
          <RotateCw class="w-4 h-4" />
        </button>
        <button
          onclick={() => playlistsStore.clearPlaylist(activePlaylist.id)}
          class="bg-red-950/40 hover:bg-red-900/40 text-red-400 hover:text-red-300 border border-red-900/50 px-3 py-1 text-xs font-semibold rounded transition-colors"
        >
          Clear Playlist
        </button>
      </div>
    </div>

    <!-- Tracks List Scrollable Container -->
    <div class="flex-1 overflow-y-auto p-6">
      <div class="w-full border border-gray-800 rounded-lg overflow-hidden bg-gray-900/40">
        <table class="w-full text-left text-sm border-collapse">
          <thead>
            <tr class="border-b border-gray-800 bg-gray-900 text-xs text-gray-400 uppercase tracking-wider font-semibold">
              <th class="py-3 px-4 w-12 text-center">#</th>
              <th class="py-3 px-4">Title</th>
              <th class="py-3 px-4">Artist</th>
              <th class="py-3 px-4">Album</th>
              <th class="py-3 px-4 w-24 text-center">Duration</th>
              <th class="py-3 px-4 w-28 text-center">Reorder</th>
              <th class="py-3 px-4 w-16 text-center">Remove</th>
            </tr>
          </thead>
          <tbody>
            {#each playlistsStore.activePlaylistTracks as item, index}
              <tr
                ondblclick={() => handlePlayPlaylistItem(index)}
                class="border-b border-gray-800/40 hover:bg-gray-800/40 group transition-colors {playerStore.playlistItemUuid === item.uuid ? 'bg-violet-900/10 text-violet-300' : ''}"
              >
                <td class="py-2.5 px-4 text-center text-gray-500 font-medium">
                  {#if playerStore.playlistItemUuid === item.uuid && playerStore.state === 'playing'}
                    <div class="flex items-center justify-center gap-0.5 h-4 w-4 mx-auto">
                      <span class="w-0.5 bg-violet-400 animate-bounce h-full" style="animation-delay: 0.1s"></span>
                      <span class="w-0.5 bg-violet-400 animate-bounce h-2/3" style="animation-delay: 0.2s"></span>
                      <span class="w-0.5 bg-violet-400 animate-bounce h-full" style="animation-delay: 0.3s"></span>
                    </div>
                  {:else}
                    {index + 1}
                  {/if}
                </td>
                <td class="py-2.5 px-4 font-medium truncate max-w-xs {playerStore.playlistItemUuid === item.uuid ? 'text-violet-300' : 'text-white'}">
                  {item.song?.title || "Unknown Title"}
                </td>
                <td class="py-2.5 px-4 text-gray-300 truncate max-w-xs">{item.song?.artist || "Unknown Artist"}</td>
                <td class="py-2.5 px-4 text-gray-400 truncate max-w-xs">{item.song?.album || "Unknown Album"}</td>
                <td class="py-2.5 px-4 text-center text-gray-400">{formatDuration(item.song?.length_nanosec)}</td>
                <td class="py-2.5 px-4 text-center">
                  <div class="flex items-center justify-center gap-1 opacity-0 group-hover:opacity-100 transition-opacity">
                    <button
                      onclick={() => handleMoveUp(index)}
                      disabled={index === 0}
                      class="p-1 hover:text-white disabled:opacity-30 disabled:hover:text-gray-400 transition-colors"
                      title="Move track up"
                    >
                      <ArrowUp class="w-3.5 h-3.5" />
                    </button>
                    <button
                      onclick={() => handleMoveDown(index)}
                      disabled={index === playlistsStore.activePlaylistTracks.length - 1}
                      class="p-1 hover:text-white disabled:opacity-30 disabled:hover:text-gray-400 transition-colors"
                      title="Move track down"
                    >
                      <ArrowDown class="w-3.5 h-3.5" />
                    </button>
                  </div>
                </td>
                <td class="py-2.5 px-4 text-center">
                  <button
                    onclick={() => handleRemoveItem(item.uuid)}
                    class="text-gray-500 hover:text-red-400 transition-colors"
                    title="Remove from playlist"
                  >
                    <Trash2 class="w-4 h-4" />
                  </button>
                </td>
              </tr>
            {/each}
            {#if playlistsStore.activePlaylistTracks.length === 0}
              <tr>
                <td colspan="7" class="py-12 text-center text-gray-500">
                  <ListMusic class="w-12 h-12 mx-auto mb-2 text-gray-600" />
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
    <div class="flex-1 flex flex-col items-center justify-center text-gray-500">
      <ListMusic class="w-16 h-16 mb-4 text-gray-700" />
      <h2 class="text-lg font-bold text-gray-400 mb-1">No Playlist Selected</h2>
      <p class="text-sm">Create or select a playlist from the sidebar to start.</p>
    </div>
  {/if}
</div>
