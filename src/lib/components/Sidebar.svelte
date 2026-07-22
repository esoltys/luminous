<script lang="ts">
  import { collectionStore } from "../stores/collection.svelte";
  import { playlistsStore } from "../stores/playlists.svelte";
  import { playerStore } from "../stores/player.svelte";
  import { themeStore } from "../stores/theme.svelte";
  import { i18n } from "../stores/i18n.svelte";
  import { Library, ListMusic, Sparkles, Settings, FileText, Home, Mic2, DiscAlbum, Music } from "lucide-svelte";
  import { open } from "@tauri-apps/plugin-dialog";
  import { isSmartPlaylistSpec } from "../utils/filterParser";

  import { invoke } from "@tauri-apps/api/core";

  let { width = 256 }: { width?: number } = $props();

  let showAddDirModal = $state(false);

  let isCollapsed = $derived(width < 180);

  function navigateToFoldersSettings() {
    collectionStore.activeTab = "settings";
    invoke("set_app_setting", { key: "active_settings_tab", value: "folders" }).catch((err) => {
      console.error("Failed to set active_settings_tab:", err);
    });
  }

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

  function openPlaylistsSubTab(subTab: "auto" | "custom") {
    collectionStore.activeTab = "playlists";
    collectionStore.playlistsSubTab = subTab;
    collectionStore.selectedPlaylistId = null;
    collectionStore.selectedAutoPlaylist = null;
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

    <div class="w-full flex flex-col {isCollapsed ? 'items-center' : ''}">
      <button
        onclick={() => { collectionStore.activeTab = "collection"; collectionStore.selectedArtistName = null; collectionStore.selectedAlbumName = null; }}
        class="flex items-center gap-3 transition-all duration-150 {collectionStore.activeTab === 'collection' ? 'bg-brand-accent text-brand-accent-contrast shadow-lg shadow-brand-accent/20' : 'text-brand-text-secondary hover:bg-brand-accent/10 hover:text-brand-accent-text-hover'} {isCollapsed ? 'justify-center w-10 h-10 rounded-xl p-0' : 'w-full px-3 py-2 rounded-lg text-sm font-medium'}"
        title={i18n.t('sidebar.collection')}
      >
        <Library class={isCollapsed ? "w-5 h-5" : "w-4 h-4"} />
        {#if !isCollapsed}
          <span class="truncate whitespace-nowrap">{i18n.t('sidebar.collection')}</span>
        {/if}
      </button>

      {#if collectionStore.activeTab === 'collection' && !isCollapsed}
        <div class="pl-4 pr-1 py-1 space-y-0.5 border-l-2 border-brand-accent/30 ml-4 my-1">
          <button
            onclick={() => { collectionStore.activeTab = "collection"; collectionStore.activeSubTab = "artists"; collectionStore.selectedArtistName = null; collectionStore.selectedAlbumName = null; }}
            class="w-full flex items-center justify-between px-2.5 py-1.5 rounded-md text-xs transition-colors cursor-pointer {collectionStore.activeSubTab === 'artists' && !collectionStore.selectedArtistName && !collectionStore.selectedAlbumName ? 'bg-brand-accent/20 text-brand-accent-text font-semibold' : 'text-brand-text-secondary hover:text-brand-text-primary hover:bg-brand-accent/10'}"
          >
            <div class="flex items-center gap-2 truncate">
              <Mic2 class="w-3.5 h-3.5" />
              <span class="truncate">{i18n.t('sidebar.artists')}</span>
            </div>
            <span class="text-[10px] text-brand-text-secondary/60 ml-1">
              ({collectionStore.searchQuery.trim() !== "" ? collectionStore.filteredArtists.length : collectionStore.stats.total_artists})
            </span>
          </button>

          <button
            onclick={() => { collectionStore.activeTab = "collection"; collectionStore.activeSubTab = "albums"; collectionStore.selectedArtistName = null; collectionStore.selectedAlbumName = null; }}
            class="w-full flex items-center justify-between px-2.5 py-1.5 rounded-md text-xs transition-colors cursor-pointer {collectionStore.activeSubTab === 'albums' && !collectionStore.selectedArtistName && !collectionStore.selectedAlbumName ? 'bg-brand-accent/20 text-brand-accent-text font-semibold' : 'text-brand-text-secondary hover:text-brand-text-primary hover:bg-brand-accent/10'}"
          >
            <div class="flex items-center gap-2 truncate">
              <DiscAlbum class="w-3.5 h-3.5" />
              <span class="truncate">{i18n.t('sidebar.albums')}</span>
            </div>
            <span class="text-[10px] text-brand-text-secondary/60 ml-1">
              ({collectionStore.searchQuery.trim() !== "" ? collectionStore.filteredAlbums.length : collectionStore.stats.total_albums})
            </span>
          </button>

          <button
            onclick={() => { collectionStore.activeTab = "collection"; collectionStore.activeSubTab = "songs"; collectionStore.selectedArtistName = null; collectionStore.selectedAlbumName = null; }}
            class="w-full flex items-center justify-between px-2.5 py-1.5 rounded-md text-xs transition-colors cursor-pointer {collectionStore.activeSubTab === 'songs' && !collectionStore.selectedArtistName && !collectionStore.selectedAlbumName ? 'bg-brand-accent/20 text-brand-accent-text font-semibold' : 'text-brand-text-secondary hover:text-brand-text-primary hover:bg-brand-accent/10'}"
          >
            <div class="flex items-center gap-2 truncate">
              <Music class="w-3.5 h-3.5" />
              <span class="truncate">{i18n.t('sidebar.songs')}</span>
            </div>
            <span class="text-[10px] text-brand-text-secondary/60 ml-1">
              ({collectionStore.searchQuery.trim() !== "" ? collectionStore.filteredSongs.length : collectionStore.stats.total_songs})
            </span>
          </button>
        </div>
      {/if}
    </div>

    <div class="w-full flex flex-col {isCollapsed ? 'items-center' : ''}">
      <button
        onclick={() => { collectionStore.activeTab = "playlists"; collectionStore.selectedPlaylistId = null; collectionStore.selectedAutoPlaylist = null; }}
        class="flex items-center gap-3 transition-all duration-150 {collectionStore.activeTab === 'playlists' ? 'bg-brand-accent text-brand-accent-contrast shadow-lg shadow-brand-accent/20' : 'text-brand-text-secondary hover:bg-brand-accent/10 hover:text-brand-accent-text-hover'} {isCollapsed ? 'justify-center w-10 h-10 rounded-xl p-0' : 'w-full px-3 py-2 rounded-lg text-sm font-medium'}"
        title={i18n.t('sidebar.playlists')}
      >
        <ListMusic class={isCollapsed ? "w-5 h-5" : "w-4 h-4"} />
        {#if !isCollapsed}
          <span class="truncate whitespace-nowrap">{i18n.t('sidebar.playlists')}</span>
        {/if}
      </button>

      {#if collectionStore.activeTab === 'playlists' && !isCollapsed}
        <div class="pl-4 pr-1 py-1 space-y-0.5 border-l-2 border-brand-accent/30 ml-4 my-1">
          <button
            onclick={() => openPlaylistsSubTab("auto")}
            class="w-full flex items-center justify-between px-2.5 py-1.5 rounded-md text-xs transition-colors cursor-pointer {collectionStore.playlistsSubTab === 'auto' && !collectionStore.selectedPlaylistId && !collectionStore.selectedAutoPlaylist ? 'bg-brand-accent/20 text-brand-accent-text font-semibold' : 'text-brand-text-secondary hover:text-brand-text-primary hover:bg-brand-accent/10'}"
          >
            <div class="flex items-center gap-2 truncate">
              <Sparkles class="w-3.5 h-3.5" />
              <span class="truncate">{i18n.t('sidebar.playlistsAuto')}</span>
            </div>
            <span class="text-[10px] text-brand-text-secondary/60 ml-1">
              ({playlistsStore.visibleAutoPlaylistCount})
            </span>
          </button>

          <button
            onclick={() => openPlaylistsSubTab("custom")}
            class="w-full flex items-center justify-between px-2.5 py-1.5 rounded-md text-xs transition-colors cursor-pointer {collectionStore.playlistsSubTab === 'custom' && !collectionStore.selectedPlaylistId && !collectionStore.selectedAutoPlaylist ? 'bg-brand-accent/20 text-brand-accent-text font-semibold' : 'text-brand-text-secondary hover:text-brand-text-primary hover:bg-brand-accent/10'}"
          >
            <div class="flex items-center gap-2 truncate">
              <ListMusic class="w-3.5 h-3.5" />
              <span class="truncate">{i18n.t('sidebar.playlistsCustom')}</span>
            </div>
            <span class="text-[10px] text-brand-text-secondary/60 ml-1">
              ({playlistsStore.playlists.filter((p) => !p.dynamic_enabled || isSmartPlaylistSpec(p.dynamic_spec)).length})
            </span>
          </button>
        </div>
      {/if}
    </div>

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

  <div class="flex-1"></div>

  <!-- Bottom spacer for player bar -->
  <div class:mb-24={!!playerStore.currentSong}></div>
</aside>
