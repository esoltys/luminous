<script lang="ts">
  import { ChevronLeft, ChevronRight, Search, FolderOpen, RefreshCw, PanelLeft, PanelBottom, PanelRight, User, Disc, ListMusic, Music, History, X } from "lucide-svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { collectionStore } from "../stores/collection.svelte";
  import { playerStore } from "../stores/player.svelte";
  import { themeStore } from "../stores/theme.svelte";
  import { i18n } from "../stores/i18n.svelte";
  import { open } from "@tauri-apps/plugin-dialog";
  import { getCoverArtUrl, type RecentSearchItem } from "../types";
  import ReactiveLogoBrand from "./ReactiveLogoBrand.svelte";
  import { fade } from "svelte/transition";

  let searchInput: HTMLInputElement | undefined;
  let searchContainerRef: HTMLDivElement | undefined;
  let isSearchFocused = $state(false);

  function navigateToFoldersSettings() {
    collectionStore.activeTab = "settings";
    invoke("set_app_setting", { key: "active_settings_tab", value: "folders" });
  }

  // Handle Ctrl+L to focus search & Escape to close search dropdown
  function handleKeyDown(e: KeyboardEvent) {
    if ((e.ctrlKey || e.metaKey) && e.key === "l") {
      e.preventDefault();
      searchInput?.focus();
      isSearchFocused = true;
    } else if (e.key === "Escape" && isSearchFocused) {
      isSearchFocused = false;
    }

    // Dedicated keyboard "Browser Back"/"Browser Forward" keys
    if (e.key === "BrowserBack") {
      e.preventDefault();
      collectionStore.goBack();
    } else if (e.key === "BrowserForward") {
      e.preventDefault();
      collectionStore.goForward();
    }
  }

  // Mouse side (thumb) buttons
  function handleMouseUp(e: MouseEvent) {
    if (e.button === 3) {
      e.preventDefault();
      collectionStore.goBack();
    } else if (e.button === 4) {
      e.preventDefault();
      collectionStore.goForward();
    }
  }

  // Close dropdown on click outside
  function handleWindowMouseDown(e: MouseEvent) {
    if (searchContainerRef && !searchContainerRef.contains(e.target as Node)) {
      isSearchFocused = false;
    }
  }

  // Search handler (prevent reload & record query to recent searches)
  function handleSearch(e: Event) {
    e.preventDefault();
    const q = collectionStore.searchQuery.trim();
    if (q) {
      collectionStore.addRecentSearch({
        kind: "query",
        title: q,
        query: q,
        subtitle: "Search query"
      });
      collectionStore.search(q);
    }
    isSearchFocused = false;
  }

  function selectRecentSearch(item: RecentSearchItem) {
    if (item.kind === "artist") {
      collectionStore.viewArtist(item.title);
    } else if (item.kind === "album") {
      collectionStore.viewAlbum(item.title);
    } else if (item.kind === "playlist" && item.entityId) {
      collectionStore.viewPlaylist(Number(item.entityId));
    } else {
      collectionStore.searchQuery = item.query || item.title;
      collectionStore.search(item.query || item.title);
    }
    collectionStore.addRecentSearch({
      kind: item.kind,
      title: item.title,
      subtitle: item.subtitle,
      query: item.query,
      artUrl: item.artUrl,
      entityId: item.entityId
    });
    isSearchFocused = false;
  }

  // Clear search query
  function clearSearch() {
    collectionStore.searchQuery = "";
    collectionStore.search("");
  }

  async function handleOpenFiles() {
    try {
      const selected = await open({
        multiple: true,
        directory: false,
        title: i18n.t('topNav.openFilesTitle', {}, "Open Audio Files or Playlists"),
        filters: [
          {
            name: "Supported Files",
            extensions: ["mp3", "flac", "ogg", "opus", "m4a", "aac", "alac", "wav", "aiff", "aif", "wv", "mpc", "ape", "tta", "dsf", "dff", "asf", "wma", "m4b", "m3u"]
          },
          {
            name: "Audio Files",
            extensions: ["mp3", "flac", "ogg", "opus", "m4a", "aac", "alac", "wav", "aiff", "aif", "wv", "mpc", "ape", "tta", "dsf", "dff", "asf", "wma", "m4b"]
          },
          {
            name: "Playlists",
            extensions: ["m3u"]
          }
        ]
      });

      if (selected) {
        const paths = Array.isArray(selected) ? selected : [selected];
        if (paths.length > 0) {
          await playerStore.openAndPlay(paths);
        }
      }
    } catch (err) {
      console.error("Failed to open files/playlists:", err);
    }
  }

</script>

<svelte:window on:keydown={handleKeyDown} on:mouseup={handleMouseUp} on:mousedown={handleWindowMouseDown} />

<header in:fade={{ duration: 600 }} class="w-full h-20 bg-brand-sidebar flex items-center px-6 gap-6 z-40 overflow-visible" class:glass-surface={themeStore.isGlassTheme}>
  <!-- History Navigation Controls -->
  <div class="flex items-center gap-2">
    <button
      onclick={() => collectionStore.goBack()}
      disabled={!collectionStore.canGoBack}
      class="p-2 rounded-lg text-brand-text-secondary hover:bg-brand-main hover:text-brand-text-primary disabled:opacity-50 disabled:cursor-not-allowed transition-colors"
      title={i18n.t('topNav.goBack')}
    >
      <ChevronLeft class="w-5 h-5" />
    </button>
    <button
      onclick={() => collectionStore.goForward()}
      disabled={!collectionStore.canGoForward}
      class="p-2 rounded-lg text-brand-text-secondary hover:bg-brand-main hover:text-brand-text-primary disabled:opacity-50 disabled:cursor-not-allowed transition-colors"
      title={i18n.t('topNav.goForward')}
    >
      <ChevronRight class="w-5 h-5" />
    </button>
  </div>

  <!-- Universal Search Bar Container -->
  <div bind:this={searchContainerRef} class="relative flex-1 max-w-2xl">
    <form onsubmit={handleSearch} class="w-full flex items-center gap-3 bg-brand-main rounded-lg px-4 py-2 border border-brand-border focus-within:border-brand-accent transition-colors">
      <Search class="w-4 h-4 text-brand-text-secondary flex-shrink-0" />
      <input
        bind:this={searchInput}
        bind:value={collectionStore.searchQuery}
        onfocus={() => { isSearchFocused = true; }}
        type="text"
        placeholder={i18n.t('topNav.searchPlaceholder')}
        class="flex-1 bg-transparent text-brand-text-primary text-sm focus:outline-none placeholder-brand-text-secondary/50"
      />

      <!-- Manage Library / Rescan button -->
      <button
        type="button"
        onclick={navigateToFoldersSettings}
        class="p-1 text-brand-text-secondary hover:text-brand-accent-text transition-colors flex-shrink-0 cursor-pointer flex items-center gap-1.5"
        title={collectionStore.isScanning
          ? `${i18n.t('sidebar.scanProgressClickHint')} (${collectionStore.scanProgress?.scanned || 0}/${collectionStore.scanProgress?.total || 0})`
          : i18n.t('sidebar.manageLibrary')}
      >
        <RefreshCw class="w-4 h-4 {collectionStore.isScanning ? 'animate-spin text-brand-accent-text' : ''}" />
        {#if collectionStore.isScanning}
          <span class="text-[10px] text-brand-accent-text font-mono font-medium">
            {collectionStore.scanProgress?.scanned || 0}/{collectionStore.scanProgress?.total || 0}
          </span>
        {/if}
      </button>

      <!-- Open Files/Playlists button -->
      <button
        type="button"
        onclick={handleOpenFiles}
        class="p-1 text-brand-text-secondary hover:text-brand-accent-text transition-colors flex-shrink-0 cursor-pointer"
        title={i18n.t('topNav.openFilesTooltip')}
      >
        <FolderOpen class="w-4 h-4" />
      </button>

      <!-- Search feedback / progress -->
      {#if collectionStore.searchLoading}
        <div class="animate-spin rounded-full h-4 w-4 border-2 border-brand-accent border-t-transparent flex-shrink-0" title={i18n.t('topNav.searching')}></div>
      {:else if collectionStore.searchQuery}
        <span class="text-[10px] bg-brand-border/60 px-1.5 py-0.5 rounded text-brand-text-secondary font-mono flex-shrink-0 select-none">
          {i18n.t('topNav.tracksCount', { count: collectionStore.searchResults.length })}
        </span>
        <button
          type="button"
          onclick={clearSearch}
          class="p-1 text-brand-text-secondary hover:text-brand-accent-text transition-colors flex-shrink-0 font-bold leading-none text-sm"
          title={i18n.t('topNav.clearSearch')}
        >
          ✕
        </button>
      {/if}
    </form>

    <!-- Recent Searches Dropdown -->
    {#if isSearchFocused}
      <div
        class="absolute left-0 right-0 top-full mt-2 bg-brand-sidebar/95 backdrop-blur-md rounded-xl border border-brand-border shadow-2xl p-3 z-50 max-h-96 overflow-y-auto"
      >
        <div class="flex items-center justify-between px-2 py-1 mb-2 border-b border-brand-border/40 select-none">
          <span class="text-xs font-semibold text-brand-text-secondary uppercase tracking-wider">
            {i18n.t('topNav.recentSearches', {}, 'Recent searches')}
          </span>
          {#if collectionStore.recentSearches.length > 0}
            <button
              type="button"
              onclick={(e) => { e.stopPropagation(); collectionStore.clearRecentSearches(); }}
              class="text-xs text-brand-text-secondary hover:text-brand-accent-text transition-colors cursor-pointer"
            >
              {i18n.t('topNav.clearRecentSearches', {}, 'Clear recent searches')}
            </button>
          {/if}
        </div>

        {#if collectionStore.recentSearches.length === 0}
          <div class="p-4 text-center text-xs text-brand-text-secondary/60 select-none">
            {i18n.t('topNav.noRecentSearches', {}, 'No recent searches')}
          </div>
        {:else}
          <div class="flex flex-col gap-1">
            {#each collectionStore.recentSearches as item (item.id)}
              <div
                role="button"
                tabindex="0"
                onclick={() => selectRecentSearch(item)}
                onkeydown={(e) => e.key === 'Enter' && selectRecentSearch(item)}
                class="group flex items-center justify-between p-2 rounded-lg hover:bg-brand-main/80 transition-colors cursor-pointer"
              >
                <div class="flex items-center gap-3 min-w-0 flex-1">
                  <!-- Avatar / Artwork / Icon -->
                  <div class="w-9 h-9 flex-shrink-0 flex items-center justify-center bg-brand-main/60 border border-brand-border/40 overflow-hidden {item.kind === 'artist' ? 'rounded-full' : 'rounded-md'}">
                    {#if item.artUrl}
                      <img src={getCoverArtUrl(item.artUrl)} alt="" class="w-full h-full object-cover" />
                    {:else if item.kind === 'artist'}
                      <User class="w-4 h-4 text-brand-text-secondary" />
                    {:else if item.kind === 'album'}
                      <Disc class="w-4 h-4 text-brand-text-secondary" />
                    {:else if item.kind === 'playlist'}
                      <ListMusic class="w-4 h-4 text-brand-text-secondary" />
                    {:else if item.kind === 'song'}
                      <Music class="w-4 h-4 text-brand-text-secondary" />
                    {:else}
                      <History class="w-4 h-4 text-brand-text-secondary" />
                    {/if}
                  </div>

                  <!-- Title & Subtitle -->
                  <div class="flex flex-col min-w-0 flex-1">
                    <span class="text-sm font-medium text-brand-text-primary truncate group-hover:text-brand-accent-text transition-colors">
                      {item.title}
                    </span>
                    <span class="text-xs text-brand-text-secondary/70 truncate capitalize">
                      {item.subtitle || item.kind}
                    </span>
                  </div>
                </div>

                <!-- Delete Single Item Button -->
                <button
                  type="button"
                  onclick={(e) => {
                    e.stopPropagation();
                    collectionStore.removeRecentSearch(item.id);
                  }}
                  class="p-1.5 opacity-0 group-hover:opacity-100 hover:text-red-400 text-brand-text-secondary/60 transition-all cursor-pointer rounded"
                  title="Remove from recent searches"
                >
                  <X class="w-3.5 h-3.5" />
                </button>
              </div>
            {/each}
          </div>
        {/if}
      </div>
    {/if}
  </div>

  <!-- Layout Panel Toggles Group -->
  <div class="flex items-center gap-1.5 bg-brand-main/60 p-1 rounded-lg border border-brand-border/60 ml-auto flex-shrink-0 select-none">
    <button
      onclick={() => collectionStore.toggleSidebar()}
      class="p-1.5 rounded-md transition-all cursor-pointer {collectionStore.sidebarOpen ? 'bg-brand-border text-brand-accent-text shadow-sm' : 'text-brand-text-secondary hover:text-brand-text-primary hover:bg-brand-main/50'}"
      title={i18n.t('topNav.toggleSidebar')}
    >
      <PanelLeft class="w-4 h-4" />
    </button>
    <button
      onclick={() => collectionStore.toggleImmersiveMode()}
      class="p-1.5 rounded-md transition-all cursor-pointer {!collectionStore.immersiveMode ? 'bg-brand-border text-brand-accent-text shadow-sm' : 'text-brand-text-secondary hover:text-brand-text-primary hover:bg-brand-main/50'}"
      title={i18n.t('topNav.toggleImmersive')}
    >
      <PanelBottom class="w-4 h-4" />
    </button>
    <button
      onclick={() => collectionStore.toggleRightPanel()}
      class="p-1.5 rounded-md transition-all cursor-pointer {collectionStore.rightPanelOpen ? 'bg-brand-border text-brand-accent-text shadow-sm' : 'text-brand-text-secondary hover:text-brand-text-primary hover:bg-brand-main/50'}"
      title={i18n.t('topNav.toggleRightPanel')}
    >
      <PanelRight class="w-4 h-4" />
    </button>
  </div>

  <!-- Reactive Logo Brand -->
  <div class="flex items-center justify-center flex-shrink-0">
    <ReactiveLogoBrand size="lg" />
  </div>
</header>

