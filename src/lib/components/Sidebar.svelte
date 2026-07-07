<script lang="ts">
  import { collectionStore } from "../stores/collection.svelte";
  import { playlistsStore } from "../stores/playlists.svelte";
  import { Music, ListMusic, FolderHeart, RefreshCw, Plus, Trash2 } from "lucide-svelte";
  import { open } from "@tauri-apps/plugin-dialog";

  // Navigation tab state
  let { activeTab = $bindable("collection"), activeSubTab = $bindable("songs") } = $props<{
    activeTab: "collection" | "playlists" | "settings";
    activeSubTab: "songs" | "albums" | "artists";
  }>();

  let showAddDirModal = $state(false);
  let newPlaylistName = $state("");

  async function handleAddDirectory() {
    try {
      const selected = await open({
        directory: true,
        multiple: false,
        title: "Select Music Directory",
      });
      if (selected && typeof selected === "string") {
        await collectionStore.addDirectory(selected);
      }
    } catch (err) {
      console.error("Failed to open folder dialog:", err);
    }
  }

  async function handleCreatePlaylist(e: Event) {
    e.preventDefault();
    if (newPlaylistName.trim() !== "") {
      await playlistsStore.createPlaylist(newPlaylistName.trim());
      newPlaylistName = "";
    }
  }
</script>

<aside class="w-64 bg-gray-900 border-r border-gray-800 flex flex-col h-full text-gray-200 select-none">
  <!-- Brand / Title -->
  <div class="h-16 flex items-center px-6 border-b border-gray-800">
    <h1 class="text-xl font-bold tracking-wider text-violet-400 flex items-center gap-2">
      <Music class="w-6 h-6" /> Luminous
    </h1>
  </div>

  <!-- Navigation -->
  <nav class="p-4 space-y-1">
    <button
      onclick={() => { activeTab = "collection"; }}
      class="w-full flex items-center gap-3 px-3 py-2 rounded-lg text-sm font-medium transition-all duration-150 {activeTab === 'collection' ? 'bg-violet-600 text-white shadow-lg shadow-violet-900/40' : 'text-gray-400 hover:bg-gray-800 hover:text-gray-200'}"
    >
      <Music class="w-4 h-4" /> Collection
    </button>

    {#if activeTab === 'collection'}
      <div class="pl-8 pr-2 py-1 space-y-1 text-xs">
        <button
          onclick={() => { activeSubTab = "songs"; }}
          class="w-full text-left py-1.5 px-2 rounded {activeSubTab === 'songs' ? 'text-violet-400 font-semibold' : 'text-gray-500 hover:text-gray-300'}"
        >
          Tracks ({collectionStore.stats.total_songs})
        </button>
        <button
          onclick={() => { activeSubTab = "albums"; }}
          class="w-full text-left py-1.5 px-2 rounded {activeSubTab === 'albums' ? 'text-violet-400 font-semibold' : 'text-gray-500 hover:text-gray-300'}"
        >
          Albums ({collectionStore.stats.total_albums})
        </button>
        <button
          onclick={() => { activeSubTab = "artists"; }}
          class="w-full text-left py-1.5 px-2 rounded {activeSubTab === 'artists' ? 'text-violet-400 font-semibold' : 'text-gray-500 hover:text-gray-300'}"
        >
          Artists ({collectionStore.stats.total_artists})
        </button>
      </div>
    {/if}

    <button
      onclick={() => { activeTab = "playlists"; }}
      class="w-full flex items-center gap-3 px-3 py-2 rounded-lg text-sm font-medium transition-all duration-150 {activeTab === 'playlists' ? 'bg-violet-600 text-white shadow-lg shadow-violet-900/40' : 'text-gray-400 hover:bg-gray-800 hover:text-gray-200'}"
    >
      <ListMusic class="w-4 h-4" /> Playlists
    </button>

    <button
      onclick={() => { activeTab = "settings"; }}
      class="w-full flex items-center gap-3 px-3 py-2 rounded-lg text-sm font-medium transition-all duration-150 {activeTab === 'settings' ? 'bg-violet-600 text-white shadow-lg shadow-violet-900/40' : 'text-gray-400 hover:bg-gray-800 hover:text-gray-200'}"
    >
      <FolderHeart class="w-4 h-4" /> Watched Folders
    </button>
  </nav>

  <!-- Playlist quick access (if tab is playlists) -->
  {#if activeTab === 'playlists'}
    <div class="flex-1 overflow-y-auto px-4 py-2 border-t border-gray-800">
      <div class="flex items-center justify-between text-xs text-gray-500 font-semibold mb-2">
        <span>PLAYLISTS</span>
      </div>
      <form onsubmit={handleCreatePlaylist} class="flex items-center gap-1.5 mb-3">
        <input
          bind:value={newPlaylistName}
          placeholder="New playlist..."
          class="bg-gray-950 border border-gray-800 rounded px-2 py-1 text-xs w-full text-gray-200 focus:outline-none focus:border-violet-500"
        />
        <button type="submit" class="bg-violet-600 hover:bg-violet-500 text-white rounded p-1">
          <Plus class="w-3.5 h-3.5" />
        </button>
      </form>

      <ul class="space-y-1 text-xs">
        {#each playlistsStore.playlists as pl}
          <li class="group flex items-center justify-between rounded px-2 py-1.5 {playlistsStore.activePlaylistId === pl.id ? 'bg-gray-800 text-violet-400 font-medium' : 'text-gray-400 hover:bg-gray-800/50 hover:text-gray-200'}">
            <button
              onclick={() => playlistsStore.selectPlaylist(pl.id)}
              class="w-full text-left truncate"
            >
              {pl.name} <span class="text-[10px] text-gray-600">({pl.track_count})</span>
            </button>
            <button
              onclick={() => playlistsStore.deletePlaylist(pl.id)}
              class="opacity-0 group-hover:opacity-100 text-gray-500 hover:text-red-400 transition-opacity"
            >
              <Trash2 class="w-3.5 h-3.5" />
            </button>
          </li>
        {/each}
      </ul>
    </div>
  {:else}
    <div class="flex-1"></div>
  {/if}

  <!-- Scanning status / trigger -->
  <div class="p-4 border-t border-gray-800 bg-gray-950 text-xs">
    {#if collectionStore.isScanning}
      <div class="space-y-1.5">
        <div class="flex justify-between text-[10px] text-gray-400">
          <span class="capitalize">Phase: {collectionStore.scanProgress?.phase || "Scanning"}</span>
          <span>{collectionStore.scanProgress?.scanned || 0}/{collectionStore.scanProgress?.total || 0}</span>
        </div>
        <div class="w-full bg-gray-800 rounded-full h-1.5 overflow-hidden">
          <div
            class="bg-violet-500 h-1.5 rounded-full transition-all duration-300"
            style="width: {collectionStore.scanProgress?.total ? (collectionStore.scanProgress.scanned / collectionStore.scanProgress.total) * 100 : 0}%"
          ></div>
        </div>
        <p class="text-[10px] text-gray-500 truncate">{collectionStore.scanProgress?.current_path || ""}</p>
      </div>
    {:else}
      <button
        onclick={() => collectionStore.startScan()}
        class="w-full flex items-center justify-center gap-2 bg-gray-800 hover:bg-gray-700 text-gray-200 py-2 rounded-lg font-medium transition-all"
      >
        <RefreshCw class="w-3.5 h-3.5" /> Rescan Library
      </button>
    {/if}
  </div>
</aside>
