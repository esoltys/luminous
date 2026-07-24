<script lang="ts">
  import { ChevronLeft, ChevronRight, Search, FolderOpen, RefreshCw, PanelLeft, PanelBottom, PanelRight, User, Disc, ListMusic, Music, History, X, Sparkles } from "lucide-svelte";
  import { parseSearchRules, hasAdvancedSearchTerms, isSmartPlaylistSpec } from "../utils/filterParser";
  import { invoke } from "@tauri-apps/api/core";
  import { collectionStore, type AutoPlaylistRef } from "../stores/collection.svelte";
  import { playlistsStore } from "../stores/playlists.svelte";
  import { playerStore } from "../stores/player.svelte";
  import { themeStore } from "../stores/theme.svelte";
  import { i18n } from "../stores/i18n.svelte";
  import { open } from "@tauri-apps/plugin-dialog";
  import { getCoverArtUrl, type RecentSearchItem } from "../types";
  import CoverArt from "./CoverArt.svelte";
  import ReactiveLogoBrand from "./ReactiveLogoBrand.svelte";
  import { fade } from "svelte/transition";

  let searchInput: HTMLInputElement | undefined;
  let searchContainerRef: HTMLDivElement | undefined;
  let isSearchFocused = $state(false);

  let matchingPlaylists = $derived.by(() => {
    const query = collectionStore.searchQuery.trim().toLowerCase();
    if (!query) return [];

    const results: Array<
      | { type: "auto"; id: string; label: string; subtitle: string; ref: AutoPlaylistRef }
      | { type: "custom"; id: number; label: string; subtitle: string }
    > = [];

    // Favourites auto-playlist
    const favLabel = i18n.t("playlists.autoFavourites", {}, "Favourites");
    if (favLabel.toLowerCase().includes(query) || "favourites".includes(query) || "favorites".includes(query)) {
      results.push({
        type: "auto",
        id: "auto:favourites",
        label: favLabel,
        subtitle: i18n.t("playlists.autoPlaylistLabel", {}, "Auto-Playlist"),
        ref: { kind: "favourites" }
      });
    }

    // Recently Added auto-playlist
    const recLabel = i18n.t("playlists.autoRecentlyAdded", {}, "Recently Added");
    if (recLabel.toLowerCase().includes(query) || "recently added".includes(query) || "recent".includes(query)) {
      results.push({
        type: "auto",
        id: "auto:recently_added",
        label: recLabel,
        subtitle: i18n.t("playlists.autoPlaylistLabel", {}, "Auto-Playlist"),
        ref: { kind: "recently_added" }
      });
    }

    // Materialized playlists (genre, decade, custom)
    for (const p of playlistsStore.playlists) {
      if (!p || !p.name) continue;
      const nameLower = p.name.toLowerCase();
      const specLower = (p.dynamic_spec || "").toLowerCase();
      if (nameLower.includes(query) || specLower.includes(query)) {
        if (p.dynamic_enabled && !isSmartPlaylistSpec(p.dynamic_spec)) {
          if (p.dynamic_spec?.startsWith("decade:")) {
            const decade = p.dynamic_spec.replace(/^decade:/, "");
            results.push({
              type: "auto",
              id: `auto:decade:${p.id}`,
              label: p.name,
              subtitle: `${i18n.t("playlists.autoPlaylistLabel", {}, "Auto-Playlist")} • Decade`,
              ref: { kind: "decade", decade, playlistId: p.id, updated: p.updated }
            });
          } else {
            const genre = p.dynamic_spec || p.name;
            results.push({
              type: "auto",
              id: `auto:genre:${p.id}`,
              label: p.name,
              subtitle: `${i18n.t("playlists.autoPlaylistLabel", {}, "Auto-Playlist")} • Genre`,
              ref: { kind: "genre", genre, playlistId: p.id, updated: p.updated }
            });
          }
        } else {
          results.push({
            type: "custom",
            id: p.id,
            label: p.name,
            subtitle: i18n.t("playlists.playlistLabel", {}, "Playlist")
          });
        }
      }
    }

    return results;
  });

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
    } else if (item.kind === "song" && item.query) {
      collectionStore.viewAlbum(item.query);
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

<header in:fade={{ duration: 600 }} class="w-full h-20 bg-brand-sidebar flex items-center px-6 gap-6 z-50 overflow-visible {themeStore.isGlassTheme ? 'glass-surface' : ''}">
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
        oninput={(e) => {
          isSearchFocused = true;
          collectionStore.search((e.target as HTMLInputElement).value);
        }}
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

    <!-- Search Dropdown Popover -->
    {#if isSearchFocused}
      <div
        class="absolute left-0 right-0 top-full mt-2 bg-brand-sidebar rounded-xl border border-brand-border shadow-2xl p-3 z-50 max-h-96 overflow-y-auto"
      >
        {#if hasAdvancedSearchTerms(collectionStore.searchQuery)}
          <button
            type="button"
            onclick={() => {
              const rules = parseSearchRules(collectionStore.searchQuery);
              collectionStore.openSmartBuilder(rules);
              isSearchFocused = false;
            }}
            class="w-full flex items-center justify-between p-2.5 mb-2.5 rounded-xl bg-indigo-600/20 border border-indigo-500/40 hover:bg-indigo-600/30 transition-all cursor-pointer text-left group"
          >
            <div class="flex items-center gap-2.5">
              <div class="p-1.5 rounded-lg bg-indigo-600 text-white">
                <Sparkles class="w-4 h-4" />
              </div>
              <div>
                <div class="text-xs font-bold text-brand-text-primary group-hover:text-indigo-300 transition-colors">
                  Create Smart Playlist from Search
                </div>
                <div class="text-[11px] text-brand-text-secondary/80">
                  Pre-fill builder with rules from active search query
                </div>
              </div>
            </div>
            <span class="text-xs font-semibold text-indigo-400 group-hover:underline">Open Builder →</span>
          </button>
        {/if}

        {#if collectionStore.searchQuery.includes(":")}
          <div class="p-2.5 mb-2.5 rounded-xl bg-brand-main/60 border border-brand-border/40 text-xs text-brand-text-secondary/90">
            <div class="font-semibold text-brand-text-primary mb-1 flex items-center gap-1">
              <span>💡 Filter Syntax Hint</span>
            </div>
            <div class="space-y-0.5 text-[11px]">
              <div><span class="font-medium text-brand-text-primary">Fields:</span> <code class="bg-brand-sidebar px-1 rounded">artist</code>, <code class="bg-brand-sidebar px-1 rounded">album</code>, <code class="bg-brand-sidebar px-1 rounded">title</code>, <code class="bg-brand-sidebar px-1 rounded">genre</code>, <code class="bg-brand-sidebar px-1 rounded">year</code>, <code class="bg-brand-sidebar px-1 rounded">rating</code>, <code class="bg-brand-sidebar px-1 rounded">duration</code>, <code class="bg-brand-sidebar px-1 rounded">playcount</code></div>
              <div><span class="font-medium text-brand-text-primary">Operators:</span> <code class="bg-brand-sidebar px-1 rounded">=</code> <code class="bg-brand-sidebar px-1 rounded">!=</code> <code class="bg-brand-sidebar px-1 rounded">&gt;</code> <code class="bg-brand-sidebar px-1 rounded">&gt;=</code> <code class="bg-brand-sidebar px-1 rounded">&lt;</code> <code class="bg-brand-sidebar px-1 rounded">&lt;=</code></div>
              <div class="text-brand-text-secondary/60 italic pt-0.5">e.g. rating:&gt;=4 year:&lt;2000 genre:jazz "miles davis"</div>
            </div>
          </div>
        {/if}

        <!-- Auto-suggestions (Shown when typing a non-empty search query) -->
        {#if collectionStore.searchQuery.trim() !== ""}
          <div class="mb-3">
            <div class="flex items-center justify-between px-2 py-1 mb-1 border-b border-brand-border/40 select-none">
              <span class="text-xs font-semibold text-brand-text-secondary uppercase tracking-wider">
                {i18n.t('topNav.searchSuggestions', {}, 'Suggestions')}
              </span>
              {#if collectionStore.searchLoading}
                <span class="text-[10px] text-brand-accent-text font-mono">
                  {i18n.t('topNav.searching', {}, 'Searching...')}
                </span>
              {/if}
            </div>

            {#if collectionStore.filteredArtists.length === 0 && collectionStore.filteredAlbums.length === 0 && matchingPlaylists.length === 0 && collectionStore.searchResults.length === 0 && !collectionStore.searchLoading}
              <div class="p-3 text-center text-xs text-brand-text-secondary/60 select-none">
                {i18n.t('topNav.noSuggestions', {}, 'No matching suggestions')}
              </div>
            {:else}
              <div class="flex flex-col gap-1">
                <!-- Matching Artists (Top 3) -->
                {#each collectionStore.filteredArtists.slice(0, 3) as artist (artist.name)}
                  <div
                    role="button"
                    tabindex="0"
                    onclick={() => {
                      if (artist.name) {
                        collectionStore.viewArtist(artist.name);
                        collectionStore.addRecentSearch({
                          kind: "artist",
                          title: artist.name,
                          subtitle: i18n.t('artistDetail.artistLabel', {}, 'Artist'),
                          query: artist.name
                        });
                        isSearchFocused = false;
                      }
                    }}
                    onkeydown={(e) => e.key === 'Enter' && artist.name && collectionStore.viewArtist(artist.name)}
                    class="group flex items-center justify-between p-2 rounded-lg hover:bg-brand-main/80 transition-colors cursor-pointer"
                  >
                    <div class="flex items-center gap-3 min-w-0 flex-1">
                      <div class="w-8 h-8 rounded-full flex-shrink-0 flex items-center justify-center bg-brand-main/60 border border-brand-border/40 overflow-hidden">
                        <User class="w-4 h-4 text-brand-text-secondary" />
                      </div>
                      <div class="flex flex-col min-w-0 flex-1">
                        <span class="text-sm font-medium text-brand-text-primary truncate group-hover:text-brand-accent-text transition-colors">
                          {artist.name}
                        </span>
                        <span class="text-xs text-brand-text-secondary/70 truncate">
                          {i18n.t('artistDetail.artistLabel', {}, 'Artist')}
                        </span>
                      </div>
                    </div>
                  </div>
                {/each}

                <!-- Matching Albums (Top 3) -->
                {#each collectionStore.filteredAlbums.slice(0, 3) as album (album.album)}
                  <div
                    role="button"
                    tabindex="0"
                    onclick={() => {
                      if (album.album) {
                        collectionStore.viewAlbum(album.album);
                        collectionStore.addRecentSearch({
                          kind: "album",
                          title: album.album,
                          subtitle: `${i18n.t('collection.albumLabel', {}, 'Album')} • ${album.artist || 'Unknown'}`,
                          query: album.album,
                          artUrl: album.art_manual || album.art_automatic
                        });
                        isSearchFocused = false;
                      }
                    }}
                    onkeydown={(e) => e.key === 'Enter' && album.album && collectionStore.viewAlbum(album.album)}
                    class="group flex items-center justify-between p-2 rounded-lg hover:bg-brand-main/80 transition-colors cursor-pointer"
                  >
                    <div class="flex items-center gap-3 min-w-0 flex-1">
                      <CoverArt
                        songId={undefined}
                        artManual={album.art_manual}
                        artAutomatic={album.art_automatic}
                        artEmbedded={album.art_embedded}
                        sizeClass="w-8 h-8 rounded-md"
                      />
                      <div class="flex flex-col min-w-0 flex-1">
                        <span class="text-sm font-medium text-brand-text-primary truncate group-hover:text-brand-accent-text transition-colors">
                          {album.album}
                        </span>
                        <span class="text-xs text-brand-text-secondary/70 truncate">
                          {i18n.t('collection.albumLabel', {}, 'Album')} • {album.artist || 'Unknown'}
                        </span>
                      </div>
                    </div>
                  </div>
                {/each}

                <!-- Matching Playlists & Auto-playlists (Top 3) -->
                {#each matchingPlaylists.slice(0, 3) as item (item.id)}
                  <div
                    role="button"
                    tabindex="0"
                    onclick={() => {
                      if (item.type === 'auto') {
                        collectionStore.viewAutoPlaylist(item.ref);
                        collectionStore.addRecentSearch({
                          kind: "playlist",
                          title: item.label,
                          subtitle: item.subtitle,
                          query: item.label
                        });
                      } else {
                        collectionStore.viewPlaylist(item.id);
                        collectionStore.addRecentSearch({
                          kind: "playlist",
                          title: item.label,
                          subtitle: item.subtitle,
                          query: item.label,
                          entityId: item.id
                        });
                      }
                      isSearchFocused = false;
                    }}
                    onkeydown={(e) => {
                      if (e.key === 'Enter') {
                        if (item.type === 'auto') collectionStore.viewAutoPlaylist(item.ref);
                        else collectionStore.viewPlaylist(item.id);
                        isSearchFocused = false;
                      }
                    }}
                    class="group flex items-center justify-between p-2 rounded-lg hover:bg-brand-main/80 transition-colors cursor-pointer"
                  >
                    <div class="flex items-center gap-3 min-w-0 flex-1">
                      <div class="w-8 h-8 rounded-md flex-shrink-0 flex items-center justify-center bg-brand-main/60 border border-brand-border/40 overflow-hidden">
                        <ListMusic class="w-4 h-4 text-brand-text-secondary" />
                      </div>
                      <div class="flex flex-col min-w-0 flex-1">
                        <span class="text-sm font-medium text-brand-text-primary truncate group-hover:text-brand-accent-text transition-colors">
                          {item.label}
                        </span>
                        <span class="text-xs text-brand-text-secondary/70 truncate">
                          {item.subtitle}
                        </span>
                      </div>
                    </div>
                  </div>
                {/each}

                <!-- Matching Songs (Top 3) -->
                {#each collectionStore.searchResults.slice(0, 3) as song (song.id)}
                  <div
                    role="button"
                    tabindex="0"
                    onclick={() => {
                      const songTitle = song.title || "Unknown";
                      if (song.album) {
                        collectionStore.viewAlbum(song.album);
                      } else {
                        collectionStore.searchQuery = songTitle;
                        collectionStore.search(songTitle);
                      }
                      collectionStore.addRecentSearch({
                        kind: "song",
                        title: songTitle,
                        subtitle: `${i18n.t('collection.songLabel', {}, 'Song')} • ${song.artist || 'Unknown'}`,
                        query: song.album || songTitle,
                        artUrl: song.art_manual || song.art_automatic
                      });
                      isSearchFocused = false;
                    }}
                    onkeydown={(e) => {
                      if (e.key === 'Enter') {
                        if (song.album) collectionStore.viewAlbum(song.album);
                        else collectionStore.search(song.title || "");
                        isSearchFocused = false;
                      }
                    }}
                    class="group flex items-center justify-between p-2 rounded-lg hover:bg-brand-main/80 transition-colors cursor-pointer"
                  >
                    <div class="flex items-center gap-3 min-w-0 flex-1">
                      <CoverArt
                        songId={song.id}
                        artManual={song.art_manual}
                        artAutomatic={song.art_automatic}
                        artEmbedded={song.art_embedded}
                        sizeClass="w-8 h-8 rounded-md"
                      />
                      <div class="flex flex-col min-w-0 flex-1">
                        <span class="text-sm font-medium text-brand-text-primary truncate group-hover:text-brand-accent-text transition-colors">
                          {song.title || 'Unknown'}
                        </span>
                        <span class="text-xs text-brand-text-secondary/70 truncate">
                          {i18n.t('collection.songLabel', {}, 'Song')} • {song.artist || 'Unknown'}
                        </span>
                      </div>
                    </div>
                  </div>
                {/each}
              </div>
            {/if}
          </div>
        {/if}

        <!-- Recent Searches Section (Below Auto-suggestions) -->
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
                  {#if item.artUrl}
                    <CoverArt
                      songId={typeof item.entityId === 'number' ? item.entityId : undefined}
                      artManual={item.artUrl}
                      artAutomatic={item.artUrl}
                      sizeClass="w-9 h-9 {item.kind === 'artist' ? 'rounded-full' : 'rounded-md'}"
                    />
                  {:else}
                    <div class="w-9 h-9 flex-shrink-0 flex items-center justify-center bg-brand-main/60 border border-brand-border/40 overflow-hidden {item.kind === 'artist' ? 'rounded-full' : 'rounded-md'}">
                      {#if item.kind === 'artist'}
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
                  {/if}

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
      class="p-1.5 rounded-md transition-all cursor-pointer {collectionStore.sidebarOpen ? 'bg-brand-border text-brand-accent-text shadow-sm' : 'text-brand-text-secondary hover:text-brand-accent-text-hover hover:bg-brand-accent/10'}"
      title={i18n.t('topNav.toggleSidebar')}
    >
      <PanelLeft class="w-4 h-4" />
    </button>
    <button
      onclick={() => collectionStore.toggleImmersiveMode()}
      class="p-1.5 rounded-md transition-all cursor-pointer {collectionStore.immersiveMode ? 'bg-brand-border text-brand-accent-text shadow-sm' : 'text-brand-text-secondary hover:text-brand-accent-text-hover hover:bg-brand-accent/10'}"
      title={i18n.t('topNav.toggleImmersive')}
    >
      <PanelBottom class="w-4 h-4" />
    </button>
    <button
      onclick={() => collectionStore.toggleRightPanel()}
      class="p-1.5 rounded-md transition-all cursor-pointer {collectionStore.rightPanelOpen ? 'bg-brand-border text-brand-accent-text shadow-sm' : 'text-brand-text-secondary hover:text-brand-accent-text-hover hover:bg-brand-accent/10'}"
      title={i18n.t('topNav.toggleRightPanel')}
    >
      <PanelRight class="w-4 h-4" />
    </button>
  </div>

  <!-- Reactive Logo Brand -->
  <!-- overflow-hidden + isolate scoped to just this wrapper (not the header,
       which needs overflow-visible for the search dropdown popover) so the
       logo's SVG glow filter can never bleed into the sidebar/header layers -->
  <div class="flex items-center justify-center flex-shrink-0 overflow-hidden isolate">
    <ReactiveLogoBrand size="lg" />
  </div>
</header>

<style>
  header.glass-surface {
    position: relative;
    backdrop-filter: blur(20px) saturate(180%) !important;
    -webkit-backdrop-filter: blur(20px) saturate(180%) !important;
    background-color: var(--glass-bg-sidebar) !important;
    border-color: var(--glass-border-color, var(--color-border)) !important;
    box-shadow: var(--glass-shadow, none);
  }
</style>

