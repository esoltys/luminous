<script lang="ts">
  import { collectionStore } from "../stores/collection.svelte";
  import { playlistsStore } from "../stores/playlists.svelte";
  import { playerStore } from "../stores/player.svelte";
  import { themeStore } from "../stores/theme.svelte";
  import { i18n } from "../stores/i18n.svelte";
  import { Library, ListMusic, Settings, RefreshCw, Plus, Trash2, FileText, Home, FolderInput } from "lucide-svelte";
  import { open } from "@tauri-apps/plugin-dialog";

  let { width = 256 }: { width?: number } = $props();

  let showAddDirModal = $state(false);
  let newPlaylistName = $state("");

  let isCollapsed = $derived(width < 180);

  async function handleAddDirectory() {
    try {
      const selected = await open({
        directory: true,
        multiple: false,
        title: i18n.t('settings.selectMusicDirectory'),
      });
      if (selected && typeof selected === "string") {
        await collectionStore.addDirectory(selected);
      }
    } catch (err) {
      console.error("Failed to open folder dialog:", err);
    }
  }

  async function handleImportPlaylist() {
    try {
      const selected = await open({
        multiple: false,
        title: i18n.t('playlists.importPlaylistTooltip'),
        filters: [{ name: 'Playlists (*.m3u, *.m3u8, *.pls, *.xspf)', extensions: ['m3u', 'm3u8', 'pls', 'xspf'] }],
      });
      if (selected && typeof selected === "string") {
        await playlistsStore.importPlaylist(selected);
      }
    } catch (err) {
      console.error("Failed to import playlist:", err);
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

<aside style="width: {width}px;" class="bg-brand-sidebar flex flex-col h-full text-brand-text-secondary select-none flex-shrink-0 overflow-hidden" class:glass-surface={themeStore.isGlassTheme}>
  <!-- Navigation -->
  <nav class="{isCollapsed ? 'p-2' : 'p-4'} space-y-1 flex flex-col items-center">
    <button
      onclick={() => { collectionStore.activeTab = "home"; }}
      class="flex items-center gap-3 transition-all duration-150 {collectionStore.activeTab === 'home' ? 'bg-brand-accent text-brand-accent-contrast shadow-lg shadow-brand-accent/20' : 'text-brand-text-secondary hover:bg-brand-accent/10 hover:text-brand-accent-text-hover'} {isCollapsed ? 'justify-center w-10 h-10 rounded-xl p-0' : 'w-full px-3 py-2 rounded-lg text-sm font-medium'}"
      title={i18n.t('sidebar.home')}
    >
      <Home class={isCollapsed ? "w-5 h-5" : "w-4 h-4"} />
      {#if !isCollapsed}
        <span class="truncate whitespace-nowrap">{i18n.t('sidebar.home')}</span>
      {/if}
    </button>

    <button
      onclick={() => { collectionStore.activeTab = "collection"; collectionStore.activeSubTab = "songs"; collectionStore.selectedArtistName = null; }}
      class="flex items-center gap-3 transition-all duration-150 {collectionStore.activeTab === 'collection' ? 'bg-brand-accent text-brand-accent-contrast shadow-lg shadow-brand-accent/20' : 'text-brand-text-secondary hover:bg-brand-accent/10 hover:text-brand-accent-text-hover'} {isCollapsed ? 'justify-center w-10 h-10 rounded-xl p-0' : 'w-full px-3 py-2 rounded-lg text-sm font-medium'}"
      title={i18n.t('sidebar.collection')}
    >
      <Library class={isCollapsed ? "w-5 h-5" : "w-4 h-4"} />
      {#if !isCollapsed}
        <span class="truncate whitespace-nowrap">{i18n.t('sidebar.collection')}</span>
      {/if}
    </button>

    <button
      onclick={() => { collectionStore.activeTab = "playlists"; }}
      class="flex items-center gap-3 transition-all duration-150 {collectionStore.activeTab === 'playlists' ? 'bg-brand-accent text-brand-accent-contrast shadow-lg shadow-brand-accent/20' : 'text-brand-text-secondary hover:bg-brand-accent/10 hover:text-brand-accent-text-hover'} {isCollapsed ? 'justify-center w-10 h-10 rounded-xl p-0' : 'w-full px-3 py-2 rounded-lg text-sm font-medium'}"
      title={i18n.t('sidebar.playlists')}
    >
      <ListMusic class={isCollapsed ? "w-5 h-5" : "w-4 h-4"} />
      {#if !isCollapsed}
        <span class="truncate whitespace-nowrap">{i18n.t('sidebar.playlists')}</span>
      {/if}
    </button>

    <button
      onclick={() => { collectionStore.activeTab = "lyrics"; }}
      class="flex items-center gap-3 transition-all duration-150 {collectionStore.activeTab === 'lyrics' ? 'bg-brand-accent text-brand-accent-contrast shadow-lg shadow-brand-accent/20' : 'text-brand-text-secondary hover:bg-brand-accent/10 hover:text-brand-accent-text-hover'} {isCollapsed ? 'justify-center w-10 h-10 rounded-xl p-0' : 'w-full px-3 py-2 rounded-lg text-sm font-medium'}"
      title={i18n.t('sidebar.lyrics')}
    >
      <FileText class={isCollapsed ? "w-5 h-5" : "w-4 h-4"} />
      {#if !isCollapsed}
        <span class="truncate whitespace-nowrap">{i18n.t('sidebar.lyrics')}</span>
      {/if}
    </button>

    <button
      onclick={() => { collectionStore.activeTab = "settings"; }}
      class="flex items-center gap-3 transition-all duration-150 {collectionStore.activeTab === 'settings' ? 'bg-brand-accent text-brand-accent-contrast shadow-lg shadow-brand-accent/20' : 'text-brand-text-secondary hover:bg-brand-accent/10 hover:text-brand-accent-text-hover'} {isCollapsed ? 'justify-center w-10 h-10 rounded-xl p-0' : 'w-full px-3 py-2 rounded-lg text-sm font-medium'}"
      title={i18n.t('sidebar.settings')}
    >
      <Settings class={isCollapsed ? "w-5 h-5" : "w-4 h-4"} />
      {#if !isCollapsed}
        <span class="truncate whitespace-nowrap">{i18n.t('sidebar.settings')}</span>
      {/if}
    </button>
  </nav>

  <!-- Playlist quick access (if tab is playlists and not collapsed) -->
  {#if collectionStore.activeTab === 'playlists' && !isCollapsed}
    <!-- Playlist quick access header & form (Fixed at top) -->
    <div class="pl-4 pr-4 pt-3 border-t border-brand-border flex-shrink-0">
      <div class="flex items-center justify-between text-xs text-brand-text-secondary font-semibold mb-2">
        <span>{i18n.t('sidebar.quickAccess')}</span>
        <button
          onclick={handleImportPlaylist}
          class="p-1 rounded hover:bg-brand-main text-brand-text-secondary hover:text-brand-accent-text transition-colors cursor-pointer flex items-center gap-1 text-[10px]"
          title={i18n.t('playlists.importPlaylistTooltip')}
        >
          <FolderInput class="w-3.5 h-3.5" />
        </button>
      </div>
      <form onsubmit={handleCreatePlaylist} class="flex items-center gap-1.5 mb-3">
        <input
          bind:value={newPlaylistName}
          placeholder={i18n.t('playlists.createPlaylistPlaceholder')}
          class="bg-brand-main border border-brand-border rounded px-2 py-1 text-xs w-full text-brand-text-primary focus:outline-none focus:border-brand-accent"
        />
        <button type="submit" class="bg-brand-accent hover:bg-brand-accent-hover text-brand-accent-contrast rounded p-1 cursor-pointer">
          <Plus class="w-3.5 h-3.5" />
        </button>
      </form>
    </div>

    <!-- Scrollable Playlists List -->
    <div class="flex-1 min-h-0 overflow-y-auto pl-4 pr-1 mr-4 pb-4 playlists-scroll-container">
      <ul class="space-y-1 text-xs">
        {#each playlistsStore.playlists as pl}
          <li class="group flex items-center justify-between rounded px-2 py-1.5 {playlistsStore.activePlaylistId === pl.id ? 'bg-brand-main text-brand-accent-text font-medium' : 'text-brand-text-secondary/75 hover:bg-brand-accent/10 hover:text-brand-accent-text-hover'}">
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
  <div class="{isCollapsed ? 'p-2 flex justify-center' : 'p-4'} text-xs flex-shrink-0" class:mb-24={playerStore.hasEverPlayed}>
    {#if collectionStore.isScanning}
      {#if isCollapsed}
        <div
          class="flex items-center justify-center w-10 h-10 cursor-help"
          title={i18n.t('sidebar.scanningPhaseTooltip', { phase: collectionStore.scanProgress?.phase || i18n.t('sidebar.scanning'), scanned: collectionStore.scanProgress?.scanned || 0, total: collectionStore.scanProgress?.total || 0 })}
        >
          <RefreshCw class="w-5 h-5 animate-spin text-brand-accent-text" />
        </div>
      {:else}
        <div class="space-y-1.5 w-full">
          <div class="flex justify-between text-[10px] text-brand-text-secondary/60">
            <span class="capitalize">{i18n.t('sidebar.scanningPhaseLabel', { phase: collectionStore.scanProgress?.phase || i18n.t('sidebar.scanning') })}</span>
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
      {/if}
    {:else}
      <button
        onclick={() => collectionStore.startScan()}
        class="bg-brand-sidebar hover:bg-brand-main text-brand-text-primary transition-all border border-brand-border cursor-pointer flex items-center justify-center {isCollapsed ? 'w-10 h-10 rounded-xl p-0' : 'w-full gap-2 py-2 rounded-lg font-medium'}"
        title={isCollapsed ? i18n.t('sidebar.rescanLibrary') : ""}
      >
        <RefreshCw class={isCollapsed ? "w-5 h-5" : "w-3.5 h-3.5"} />
        {#if !isCollapsed}
          {i18n.t('sidebar.rescanLibrary')}
        {/if}
      </button>
    {/if}
  </div>
</aside>

<style>
  .playlists-scroll-container {
    scrollbar-width: thin;
    scrollbar-color: var(--color-border) transparent;
  }
  .playlists-scroll-container::-webkit-scrollbar {
    width: 6px;
  }
  .playlists-scroll-container::-webkit-scrollbar-track {
    background: transparent;
  }
  .playlists-scroll-container::-webkit-scrollbar-thumb {
    background: var(--color-border);
    border-radius: 3px;
  }
</style>
