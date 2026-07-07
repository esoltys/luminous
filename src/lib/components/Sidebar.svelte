<script lang="ts">
  import { collectionStore } from "../stores/collection.svelte";
  import { playlistsStore } from "../stores/playlists.svelte";
  import { Music, ListMusic, Settings, RefreshCw, Plus, Trash2, SlidersHorizontal, FileText } from "lucide-svelte";
  import { open } from "@tauri-apps/plugin-dialog";

  // Navigation tab state
  let { activeTab = $bindable("collection"), activeSubTab = $bindable("songs") } = $props<{
    activeTab: "collection" | "playlists" | "settings" | "equalizer" | "lyrics";
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

<aside class="w-64 bg-brand-sidebar border-r border-brand-border flex flex-col h-full text-brand-text-secondary select-none">
  <!-- Brand / Title -->
  <div class="h-16 flex items-center px-6 border-b border-brand-border">
    <h1 class="text-xl font-bold tracking-wider text-brand-accent flex items-center gap-2">
      <Music class="w-6 h-6" /> Luminous
    </h1>
  </div>

  <!-- Navigation -->
  <nav class="p-4 space-y-1">
    <button
      onclick={() => { activeTab = "collection"; }}
      class="w-full flex items-center gap-3 px-3 py-2 rounded-lg text-sm font-medium transition-all duration-150 {activeTab === 'collection' ? 'bg-brand-accent text-brand-text-primary shadow-lg shadow-brand-accent/20' : 'text-brand-text-secondary hover:bg-brand-main/50 hover:text-brand-text-primary'}"
    >
      <Music class="w-4 h-4" /> Collection
    </button>

    {#if activeTab === 'collection'}
      <div class="pl-8 pr-2 py-1 space-y-1 text-xs">
        <button
          onclick={() => { activeSubTab = "songs"; }}
          class="w-full text-left py-1.5 px-2 rounded {activeSubTab === 'songs' ? 'text-brand-accent font-semibold' : 'text-brand-text-secondary/60 hover:text-brand-text-primary'}"
        >
          Tracks ({collectionStore.stats.total_songs})
        </button>
        <button
          onclick={() => { activeSubTab = "albums"; }}
          class="w-full text-left py-1.5 px-2 rounded {activeSubTab === 'albums' ? 'text-brand-accent font-semibold' : 'text-brand-text-secondary/60 hover:text-brand-text-primary'}"
        >
          Albums ({collectionStore.stats.total_albums})
        </button>
        <button
          onclick={() => { activeSubTab = "artists"; }}
          class="w-full text-left py-1.5 px-2 rounded {activeSubTab === 'artists' ? 'text-brand-accent font-semibold' : 'text-brand-text-secondary/60 hover:text-brand-text-primary'}"
        >
          Artists ({collectionStore.stats.total_artists})
        </button>
      </div>
    {/if}

    <button
      onclick={() => { activeTab = "playlists"; }}
      class="w-full flex items-center gap-3 px-3 py-2 rounded-lg text-sm font-medium transition-all duration-150 {activeTab === 'playlists' ? 'bg-brand-accent text-brand-text-primary shadow-lg shadow-brand-accent/20' : 'text-brand-text-secondary hover:bg-brand-main/50 hover:text-brand-text-primary'}"
    >
      <ListMusic class="w-4 h-4" /> Playlists
    </button>

    <button
      onclick={() => { activeTab = "settings"; }}
      class="w-full flex items-center gap-3 px-3 py-2 rounded-lg text-sm font-medium transition-all duration-150 {activeTab === 'settings' ? 'bg-brand-accent text-brand-text-primary shadow-lg shadow-brand-accent/20' : 'text-brand-text-secondary hover:bg-brand-main/50 hover:text-brand-text-primary'}"
    >
      <Settings class="w-4 h-4" /> Settings
    </button>

    <button
      onclick={() => { activeTab = "equalizer"; }}
      class="w-full flex items-center gap-3 px-3 py-2 rounded-lg text-sm font-medium transition-all duration-150 {activeTab === 'equalizer' ? 'bg-brand-accent text-brand-text-primary shadow-lg shadow-brand-accent/20' : 'text-brand-text-secondary hover:bg-brand-main/50 hover:text-brand-text-primary'}"
    >
      <SlidersHorizontal class="w-4 h-4" /> Equalizer
    </button>

    <button
      onclick={() => { activeTab = "lyrics"; }}
      class="w-full flex items-center gap-3 px-3 py-2 rounded-lg text-sm font-medium transition-all duration-150 {activeTab === 'lyrics' ? 'bg-brand-accent text-brand-text-primary shadow-lg shadow-brand-accent/20' : 'text-brand-text-secondary hover:bg-brand-main/50 hover:text-brand-text-primary'}"
    >
      <FileText class="w-4 h-4" /> Lyrics
    </button>
  </nav>

  <!-- Playlist quick access (if tab is playlists) -->
  {#if activeTab === 'playlists'}
    <div class="flex-1 overflow-y-auto px-4 py-2 border-t border-brand-border">
      <div class="flex items-center justify-between text-xs text-brand-text-secondary font-semibold mb-2">
        <span>PLAYLISTS</span>
      </div>
      <form onsubmit={handleCreatePlaylist} class="flex items-center gap-1.5 mb-3">
        <input
          bind:value={newPlaylistName}
          placeholder="New playlist..."
          class="bg-brand-main border border-brand-border rounded px-2 py-1 text-xs w-full text-brand-text-primary focus:outline-none focus:border-brand-accent"
        />
        <button type="submit" class="bg-brand-accent hover:bg-brand-accent-hover text-brand-text-primary rounded p-1">
          <Plus class="w-3.5 h-3.5 text-white" />
        </button>
      </form>

      <ul class="space-y-1 text-xs">
        {#each playlistsStore.playlists as pl}
          <li class="group flex items-center justify-between rounded px-2 py-1.5 {playlistsStore.activePlaylistId === pl.id ? 'bg-brand-main text-brand-accent font-medium' : 'text-brand-text-secondary/75 hover:bg-brand-main/40 hover:text-brand-text-primary'}">
            <button
              onclick={() => playlistsStore.selectPlaylist(pl.id)}
              class="w-full text-left truncate"
            >
              {pl.name} <span class="text-[10px] text-brand-text-secondary/50">({pl.track_count})</span>
            </button>
            <button
              onclick={() => playlistsStore.deletePlaylist(pl.id)}
              class="opacity-0 group-hover:opacity-100 text-brand-text-secondary hover:text-red-400 transition-opacity"
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
  <div class="p-4 border-t border-brand-border bg-brand-main/40 text-xs">
    {#if collectionStore.isScanning}
      <div class="space-y-1.5">
        <div class="flex justify-between text-[10px] text-brand-text-secondary/60">
          <span class="capitalize">Phase: {collectionStore.scanProgress?.phase || "Scanning"}</span>
          <span>{collectionStore.scanProgress?.scanned || 0}/{collectionStore.scanProgress?.total || 0}</span>
        </div>
        <div class="w-full bg-brand-sidebar rounded-full h-1.5 overflow-hidden">
          <div
            class="bg-brand-accent h-1.5 rounded-full transition-all duration-300"
            style="width: {collectionStore.scanProgress?.total ? (collectionStore.scanProgress.scanned / collectionStore.scanProgress.total) * 100 : 0}%"
          ></div>
        </div>
        <p class="text-[10px] text-brand-text-secondary/50 truncate">{collectionStore.scanProgress?.current_path || ""}</p>
      </div>
    {:else}
      <button
        onclick={() => collectionStore.startScan()}
        class="w-full flex items-center justify-center gap-2 bg-brand-sidebar hover:bg-brand-main text-brand-text-primary py-2 rounded-lg font-medium transition-all border border-brand-border"
      >
        <RefreshCw class="w-3.5 h-3.5" /> Rescan Library
      </button>
    {/if}
  </div>
</aside>
