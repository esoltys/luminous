<script lang="ts">
  import { collectionStore } from "../stores/collection.svelte";
  import { navigationStore } from "../stores/navigation.svelte";
  import { playlistsStore } from "../stores/playlists.svelte";
  import { playerStore } from "../stores/player.svelte";
  import { themeStore } from "../stores/theme.svelte";
  import { i18n, formatNumber } from "../stores/i18n.svelte";
  import { updaterStore } from "../stores/updater.svelte";
  import { tagsStore } from "../stores/tags.svelte";
  import { walkthroughStore } from "../stores/walkthrough.svelte";
  import { musicbrainzStore } from "../stores/musicbrainz.svelte";
  import { prefs } from "../stores/prefs.svelte";
  import { scrobblerStore } from "../stores/scrobbler.svelte";
  import MusicBrainzLoginModal from "./MusicBrainzLoginModal.svelte";
  import MusicBrainzProfilePopover from "./MusicBrainzProfilePopover.svelte";
  import PinnedNavList from "./PinnedNavList.svelte";
  import { untrack, onMount } from "svelte";
  import { fade } from "../utils/motion";
  import {
    BooksIcon as Library,
    PlaylistIcon as ListMusic,
    SparkleIcon as Sparkles,
    GearIcon as Settings,
    ChartBarIcon as BarChart2,
    BroomIcon as Broom,
    HouseIcon as Home,
    MicrophoneStageIcon as Mic2,
    DiscIcon as DiscAlbum,
    MusicNotesIcon as Music,
    TagIcon as Tag,
    ArrowUpIcon as ArrowUp,
    QuestionIcon as HelpCircle,
    StackIcon as Layers
  } from "phosphor-svelte";
  import { open } from "@tauri-apps/plugin-dialog";
  import { isSmartPlaylistSpec } from "../utils/filterParser";
  import { SIDEBAR_MIN_WIDTH_PX } from "../constants";

  import { invoke } from "@tauri-apps/api/core";

  let { width = 256, resizing = false }: { width?: number; resizing?: boolean } = $props();

  let showAddDirModal = $state(false);

  let isCollapsed = $derived(width < SIDEBAR_MIN_WIDTH_PX);
  let showUpdateBadge = $derived(updaterStore.updateAvailable || updaterStore.installStatus === "ready-to-restart");
  let showLoginModal = $state(false);
  let showProfilePopover = $state(false);
  const scrobblingPaused = $derived(scrobblerStore.status === "paused");
  const pausedLabel = $derived(i18n.t("sidebar.scrobblingPaused"));
  let profileButtonEl = $state<HTMLButtonElement | null>(null);

  onMount(() => {
    musicbrainzStore.init();
  });

  // Collapsing/expanding swaps icon sizes, padding, and text labels instantly,
  // which looks messy against the sidebar's own smooth width transition (see
  // the aside's transition-[width] duration-200 below). Stagger it instead:
  // collapsing drops the sub-item trees first, then flips the major items to
  // their compact layout right after; expanding flips the major items to
  // their full layout immediately so they grow in step with the widening
  // sidebar, then brings the sub-items back once that settles. No fade here —
  // it read as lag rather than polish, so both swaps are instant pops, just
  // sequenced.
  let layoutCollapsed = $state(untrack(() => isCollapsed));
  let subItemsVisible = $state(untrack(() => !isCollapsed));

  $effect(() => {
    if (isCollapsed) {
      subItemsVisible = false;
      const timer = setTimeout(() => { layoutCollapsed = true; }, 60);
      return () => clearTimeout(timer);
    } else {
      layoutCollapsed = false;
      const timer = setTimeout(() => { subItemsVisible = true; }, 60);
      return () => clearTimeout(timer);
    }
  });

  function selectCollectionTab() {
    navigationStore.activeTab = "collection";
    navigationStore.selectedArtistName = null;
    navigationStore.selectedAlbumName = null;
  }

  function selectPlaylistsTab() {
    navigationStore.activeTab = "playlists";
    navigationStore.selectedPlaylistId = null;
    navigationStore.selectedAutoPlaylist = null;
  }

  function navigateToSourcesSettings() {
    navigationStore.activeTab = "settings";
    invoke("set_app_setting", { key: "active_settings_tab", value: "sources" });
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
    navigationStore.activeTab = "playlists";
    navigationStore.playlistsSubTab = subTab;
    navigationStore.selectedPlaylistId = null;
    navigationStore.selectedAutoPlaylist = null;
  }

</script>

<aside style="width: {width}px;" class="bg-brand-sidebar flex flex-col h-full text-brand-text-secondary select-none flex-shrink-0 overflow-hidden transition-[width] duration-200 ease-out {themeStore.isGlassTheme ? 'glass-surface' : ''}" class:transition-none={resizing}>
  <nav data-walkthrough-target="sidebar" class="{layoutCollapsed ? 'p-2' : 'p-4'} space-y-0.5 flex flex-col items-center flex-1 min-h-0 overflow-y-auto overflow-x-hidden">
    <button
      onclick={() => { navigationStore.activeTab = "home"; }}
      class="flex items-center gap-3 transition-colors duration-150 {navigationStore.activeTab === 'home' ? 'bg-brand-accent text-brand-accent-contrast shadow-lg shadow-brand-accent/20' : 'text-brand-text-secondary hover:bg-brand-accent/10 hover:text-brand-accent-text-hover'} {layoutCollapsed ? 'justify-center w-10 h-10 rounded-xl p-0' : 'w-full px-3 py-1.5 rounded-lg text-sm font-medium'}"
      title={i18n.t('sidebar.home')}
    >
      {#if layoutCollapsed}
        <Home class="w-5 h-5" />
      {:else}
        <div class="w-7 h-7 flex items-center justify-center shrink-0">
          <Home class="w-4 h-4 icon-align" />
        </div>
        <span class="truncate whitespace-nowrap">{i18n.t('sidebar.home')}</span>
      {/if}
    </button>

    <PinnedNavList collapsed={layoutCollapsed} />

    <!-- Collection/Playlists/Lyrics/Stats stay hidden until the library has
         songs — except during the walkthrough, whose "sidebar" step
         narrates all of them, so an empty-library first-run tour would
         otherwise describe nav items the user can't see. -->
    {#if !collectionStore.statsLoaded || collectionStore.stats.total_songs > 0 || walkthroughStore.isActive}
    <div class="w-full flex flex-col {layoutCollapsed ? 'items-center' : ''}">
      <button
        onclick={selectCollectionTab}
        class="flex items-center gap-3 transition-colors duration-150 {navigationStore.activeTab === 'collection' && layoutCollapsed ? 'bg-brand-accent text-brand-accent-contrast shadow-lg shadow-brand-accent/20' : 'text-brand-text-secondary hover:bg-brand-accent/10 hover:text-brand-accent-text-hover'} {layoutCollapsed ? 'justify-center w-10 h-10 rounded-xl p-0' : 'w-full px-3 py-1.5 rounded-lg text-sm font-medium'}"
        title={i18n.t('sidebar.collection')}
      >
        {#if layoutCollapsed}
          {#if navigationStore.activeTab === 'collection' && navigationStore.activeSubTab === 'artists'}
            <Mic2 class="w-5 h-5" />
          {:else if navigationStore.activeTab === 'collection' && navigationStore.activeSubTab === 'albums'}
            <DiscAlbum class="w-5 h-5" />
          {:else if navigationStore.activeTab === 'collection' && navigationStore.activeSubTab === 'songs'}
            <Music class="w-5 h-5" />
          {:else if navigationStore.activeTab === 'collection' && navigationStore.activeSubTab === 'genres'}
            <Tag class="w-5 h-5" />
          {:else}
            <Library class="w-5 h-5" />
          {/if}
        {:else}
          <div class="w-7 h-7 flex items-center justify-center shrink-0">
            {#if navigationStore.activeTab === 'collection' && navigationStore.activeSubTab === 'artists'}
              <Mic2 class="w-4 h-4 icon-align" />
            {:else if navigationStore.activeTab === 'collection' && navigationStore.activeSubTab === 'albums'}
              <DiscAlbum class="w-4 h-4 icon-align" />
            {:else if navigationStore.activeTab === 'collection' && navigationStore.activeSubTab === 'songs'}
              <Music class="w-4 h-4 icon-align" />
            {:else if navigationStore.activeTab === 'collection' && navigationStore.activeSubTab === 'genres'}
              <Tag class="w-4 h-4 icon-align" />
            {:else}
              <Library class="w-4 h-4 icon-align" />
            {/if}
          </div>
          <span class="truncate whitespace-nowrap">{i18n.t('sidebar.collection')}</span>
        {/if}
      </button>

      {#if subItemsVisible}
        <div class="pl-4 pr-1 py-0.5 space-y-0 border-l-2 border-brand-accent/30 ml-[18px] my-0.5">
          <button
            onclick={() => { navigationStore.activeTab = "collection"; navigationStore.activeSubTab = "artists"; navigationStore.selectedArtistName = null; navigationStore.selectedAlbumName = null; }}
            class="w-full flex items-center justify-between px-2.5 py-1 rounded-md text-xs transition-colors {navigationStore.activeTab === 'collection' && navigationStore.activeSubTab === 'artists' && !navigationStore.selectedArtistName && !navigationStore.selectedAlbumName ? 'bg-brand-accent/20 text-brand-accent-text font-semibold' : 'text-brand-text-secondary hover:text-brand-text-primary hover:bg-brand-accent/10'}"
          >
            <div class="flex items-center gap-2 truncate">
              <Mic2 class="w-3.5 h-3.5 icon-align" />
              <span class="truncate">{i18n.t('sidebar.artists')}</span>
            </div>
            <span class="text-[10px] text-brand-text-secondary/60 ml-1">
              ({formatNumber(collectionStore.searchQuery.trim() !== "" ? collectionStore.filteredArtists.length : collectionStore.stats.total_artists)})
            </span>
          </button>

          <button
            onclick={() => { navigationStore.activeTab = "collection"; navigationStore.activeSubTab = "albums"; navigationStore.selectedArtistName = null; navigationStore.selectedAlbumName = null; }}
            class="w-full flex items-center justify-between px-2.5 py-1 rounded-md text-xs transition-colors {navigationStore.activeTab === 'collection' && navigationStore.activeSubTab === 'albums' && !navigationStore.selectedArtistName && !navigationStore.selectedAlbumName ? 'bg-brand-accent/20 text-brand-accent-text font-semibold' : 'text-brand-text-secondary hover:text-brand-text-primary hover:bg-brand-accent/10'}"
          >
            <div class="flex items-center gap-2 truncate">
              <DiscAlbum class="w-3.5 h-3.5 icon-align" />
              <span class="truncate">{i18n.t('sidebar.albums')}</span>
            </div>
            <span class="text-[10px] text-brand-text-secondary/60 ml-1">
              ({formatNumber(collectionStore.searchQuery.trim() !== "" ? collectionStore.filteredAlbums.length : collectionStore.stats.total_albums)})
            </span>
          </button>

          <button
            onclick={() => { navigationStore.activeTab = "collection"; navigationStore.activeSubTab = "songs"; navigationStore.selectedArtistName = null; navigationStore.selectedAlbumName = null; }}
            class="w-full flex items-center justify-between px-2.5 py-1 rounded-md text-xs transition-colors {navigationStore.activeTab === 'collection' && navigationStore.activeSubTab === 'songs' && !navigationStore.selectedArtistName && !navigationStore.selectedAlbumName ? 'bg-brand-accent/20 text-brand-accent-text font-semibold' : 'text-brand-text-secondary hover:text-brand-text-primary hover:bg-brand-accent/10'}"
          >
            <div class="flex items-center gap-2 truncate">
              <Music class="w-3.5 h-3.5 icon-align" />
              <span class="truncate">{i18n.t('sidebar.songs')}</span>
            </div>
            <span class="text-[10px] text-brand-text-secondary/60 ml-1">
              ({formatNumber(collectionStore.searchQuery.trim() !== "" ? collectionStore.filteredSongs.length : collectionStore.stats.total_songs)})
            </span>
          </button>

          <button
            onclick={() => { navigationStore.activeTab = "collection"; navigationStore.activeSubTab = "genres"; navigationStore.selectedArtistName = null; navigationStore.selectedAlbumName = null; }}
            class="w-full flex items-center justify-between px-2.5 py-1 rounded-md text-xs transition-colors {navigationStore.activeTab === 'collection' && navigationStore.activeSubTab === 'genres' && !navigationStore.selectedArtistName && !navigationStore.selectedAlbumName ? 'bg-brand-accent/20 text-brand-accent-text font-semibold' : 'text-brand-text-secondary hover:text-brand-text-primary hover:bg-brand-accent/10'}"
          >
            <div class="flex items-center gap-2 truncate">
              <Tag class="w-3.5 h-3.5 icon-align" />
              <span class="truncate">{i18n.t('sidebar.genres')}</span>
            </div>
            <span class="text-[10px] text-brand-text-secondary/60 ml-1">
              ({formatNumber(tagsStore.allTags.length)})
            </span>
          </button>
        </div>
      {/if}
    </div>

    <div class="w-full flex flex-col {layoutCollapsed ? 'items-center' : ''}">
      <button
        onclick={selectPlaylistsTab}
        class="flex items-center gap-3 transition-colors duration-150 {navigationStore.activeTab === 'playlists' && layoutCollapsed ? 'bg-brand-accent text-brand-accent-contrast shadow-lg shadow-brand-accent/20' : 'text-brand-text-secondary hover:bg-brand-accent/10 hover:text-brand-accent-text-hover'} {layoutCollapsed ? 'justify-center w-10 h-10 rounded-xl p-0' : 'w-full px-3 py-1.5 rounded-lg text-sm font-medium'}"
        title={i18n.t('sidebar.playlists')}
      >
        {#if layoutCollapsed}
          {#if navigationStore.activeTab === 'playlists' && navigationStore.playlistsSubTab === 'auto'}
            <Sparkles class="w-5 h-5" />
          {:else}
            <ListMusic class="w-5 h-5" />
          {/if}
        {:else}
          <div class="w-7 h-7 flex items-center justify-center shrink-0">
            {#if navigationStore.activeTab === 'playlists' && navigationStore.playlistsSubTab === 'auto'}
              <Sparkles class="w-4 h-4 icon-align" />
            {:else}
              <ListMusic class="w-4 h-4 icon-align" />
            {/if}
          </div>
          <span class="truncate whitespace-nowrap">{i18n.t('sidebar.playlists')}</span>
        {/if}
      </button>

      {#if subItemsVisible}
        <div class="pl-4 pr-1 py-0.5 space-y-0 border-l-2 border-brand-accent/30 ml-[18px] my-0.5">
          <button
            onclick={() => openPlaylistsSubTab("auto")}
            class="w-full flex items-center justify-between px-2.5 py-1 rounded-md text-xs transition-colors {navigationStore.activeTab === 'playlists' && navigationStore.playlistsSubTab === 'auto' && !navigationStore.selectedPlaylistId && !navigationStore.selectedAutoPlaylist ? 'bg-brand-accent/20 text-brand-accent-text font-semibold' : 'text-brand-text-secondary hover:text-brand-text-primary hover:bg-brand-accent/10'}"
          >
            <div class="flex items-center gap-2 truncate">
              <Sparkles class="w-3.5 h-3.5 icon-align" />
              <span class="truncate">{i18n.t('sidebar.playlistsAuto')}</span>
            </div>
            <span class="text-[10px] text-brand-text-secondary/60 ml-1">
              ({formatNumber(playlistsStore.visibleAutoPlaylistCount)})
            </span>
          </button>

          <button
            onclick={() => openPlaylistsSubTab("custom")}
            class="w-full flex items-center justify-between px-2.5 py-1 rounded-md text-xs transition-colors {navigationStore.activeTab === 'playlists' && navigationStore.playlistsSubTab === 'custom' && !navigationStore.selectedPlaylistId && !navigationStore.selectedAutoPlaylist ? 'bg-brand-accent/20 text-brand-accent-text font-semibold' : 'text-brand-text-secondary hover:text-brand-text-primary hover:bg-brand-accent/10'}"
          >
            <div class="flex items-center gap-2 truncate">
              <ListMusic class="w-3.5 h-3.5 icon-align" />
              <span class="truncate">{i18n.t('sidebar.playlistsCustom')}</span>
            </div>
            <span class="text-[10px] text-brand-text-secondary/60 ml-1">
              ({formatNumber(playlistsStore.playlists.filter((p) => (!p.dynamic_enabled || isSmartPlaylistSpec(p.dynamic_spec)) && !p.is_queue).length)})
            </span>
          </button>

          <button
            onclick={async () => {
              const queuePl = await playlistsStore.requireQueue();
              playlistsStore.selectPlaylist(queuePl.id);
              navigationStore.viewPlaylist(queuePl.id);
            }}
            class="w-full flex items-center justify-between px-2.5 py-1 rounded-md text-xs transition-colors {navigationStore.activeTab === 'playlists' && navigationStore.selectedPlaylistId && playlistsStore.playlists.find((p) => p.id === navigationStore.selectedPlaylistId)?.name?.toLowerCase() === 'queue' ? 'bg-brand-accent/20 text-brand-accent-text font-semibold' : 'text-brand-text-secondary hover:text-brand-text-primary hover:bg-brand-accent/10'}"
          >
            <div class="flex items-center gap-2 truncate">
              <Layers class="w-3.5 h-3.5 icon-align" />
              <span class="truncate">{i18n.t('playerBar.queueTitle', {}, 'Queue')}</span>
            </div>
            <span class="text-[10px] text-brand-text-secondary/60 ml-1">
              ({formatNumber(playlistsStore.queueTrackCount)})
            </span>
          </button>
        </div>
      {/if}
    </div>

    <button
      onclick={() => { navigationStore.activeTab = "organize"; }}
      class="flex items-center gap-3 transition-colors duration-150 {navigationStore.activeTab === 'organize' ? 'bg-brand-accent text-brand-accent-contrast shadow-lg shadow-brand-accent/20' : 'text-brand-text-secondary hover:bg-brand-accent/10 hover:text-brand-accent-text-hover'} {layoutCollapsed ? 'justify-center w-10 h-10 rounded-xl p-0' : 'w-full px-3 py-1.5 rounded-lg text-sm font-medium'}"
      title={i18n.t('sidebar.organize')}
    >
      {#if layoutCollapsed}
        <Broom class="w-5 h-5" />
      {:else}
        <div class="w-7 h-7 flex items-center justify-center shrink-0">
          <Broom class="w-4 h-4 icon-align" />
        </div>
        <span class="truncate whitespace-nowrap">{i18n.t('sidebar.organize')}</span>
      {/if}
    </button>

    <button
      onclick={() => { navigationStore.activeTab = "stats"; }}
      class="flex items-center gap-3 transition-colors duration-150 {navigationStore.activeTab === 'stats' ? 'bg-brand-accent text-brand-accent-contrast shadow-lg shadow-brand-accent/20' : 'text-brand-text-secondary hover:bg-brand-accent/10 hover:text-brand-accent-text-hover'} {layoutCollapsed ? 'justify-center w-10 h-10 rounded-xl p-0' : 'w-full px-3 py-1.5 rounded-lg text-sm font-medium'}"
      title={i18n.t('sidebar.stats')}
    >
      {#if layoutCollapsed}
        <BarChart2 class="w-5 h-5" />
      {:else}
        <div class="w-7 h-7 flex items-center justify-center shrink-0">
          <BarChart2 class="w-4 h-4 icon-align" />
        </div>
        <span class="truncate whitespace-nowrap">{i18n.t('sidebar.stats')}</span>
      {/if}
    </button>
    {/if}

    <button
      data-walkthrough-target="library-folders"
      onclick={() => { navigationStore.activeTab = "settings"; }}
      class="relative flex items-center gap-3 transition-colors duration-150 {navigationStore.activeTab === 'settings' ? 'bg-brand-accent text-brand-accent-contrast shadow-lg shadow-brand-accent/20' : 'text-brand-text-secondary hover:bg-brand-accent/10 hover:text-brand-accent-text-hover'} {layoutCollapsed ? 'justify-center w-10 h-10 rounded-xl p-0' : 'w-full px-3 py-1.5 rounded-lg text-sm font-medium'}"
      title={showUpdateBadge ? `${i18n.t('sidebar.settings')} (${i18n.t('settings.updateAvailable', {}, 'Update available')})` : i18n.t('sidebar.settings')}
    >
      {#if layoutCollapsed}
        <Settings class="w-5 h-5" />
        {#if showUpdateBadge}
          <span class="absolute top-0.5 right-0.5 p-0.5 rounded-full bg-current/20 border border-current/30 text-current flex items-center justify-center">
            <ArrowUp class="w-2.5 h-2.5 stroke-[2.5]" />
          </span>
        {/if}
      {:else}
        <div class="w-7 h-7 flex items-center justify-center shrink-0">
          <Settings class="w-4 h-4 icon-align" />
        </div>
        <span class="truncate whitespace-nowrap flex-1 text-left">{i18n.t('sidebar.settings')}</span>
        {#if showUpdateBadge}
          <span class="px-1.5 py-0.5 rounded-full bg-current/15 border border-current/25 text-current flex items-center gap-0.5 text-[10px] font-bold">
            <ArrowUp class="w-3 h-3 stroke-[2.5]" />
          </span>
        {/if}
      {/if}
    </button>

    <button
      onclick={() => { navigationStore.activeTab = "help"; }}
      class="flex items-center gap-3 transition-colors duration-150 {navigationStore.activeTab === 'help' ? 'bg-brand-accent text-brand-accent-contrast shadow-lg shadow-brand-accent/20' : 'text-brand-text-secondary hover:bg-brand-accent/10 hover:text-brand-accent-text-hover'} {layoutCollapsed ? 'justify-center w-10 h-10 rounded-xl p-0' : 'w-full px-3 py-1.5 rounded-lg text-sm font-medium'}"
      title={i18n.t('sidebar.help')}
    >
      {#if layoutCollapsed}
        <HelpCircle class="w-5 h-5" />
      {:else}
        <div class="w-7 h-7 flex items-center justify-center shrink-0">
          <HelpCircle class="w-4 h-4 icon-align" />
        </div>
        <span class="truncate whitespace-nowrap">{i18n.t('sidebar.help')}</span>
      {/if}
    </button>
  </nav>


  <!-- MusicBrainz Login / Profile Section (#1388); hidden while Offline (#1398) -->
  {#if prefs.onlineEnabled}
  <div class="{layoutCollapsed ? 'p-2' : 'px-3 py-2'} border-t border-brand-border/40 w-full flex flex-col items-center">
    {#if musicbrainzStore.isLoggedIn}
      <button
        bind:this={profileButtonEl}
        onclick={() => { showProfilePopover = !showProfilePopover; }}
        class="flex items-center gap-2.5 transition-colors duration-150 {showProfilePopover ? 'bg-brand-accent/20 text-brand-accent-text' : 'text-brand-text-secondary hover:bg-brand-accent/10 hover:text-brand-accent-text-hover'} {layoutCollapsed ? 'justify-center w-10 h-10 rounded-xl p-0' : 'w-full px-2.5 py-1.5 rounded-lg text-sm font-medium'}"
        title={scrobblingPaused ? `${musicbrainzStore.username} (MusicBrainz) – ${pausedLabel}` : `${musicbrainzStore.username} (MusicBrainz)`}
      >
        <div class="relative shrink-0 flex items-center justify-center">
          <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 25 28" class="{layoutCollapsed ? 'w-5 h-5' : 'w-4 h-4'} shrink-0">
            <polygon fill="#ba478f" points="12 0 0 7 0 21 12 28 12 0"/>
            <polygon fill="#eb743b" points="13 0 25 7 25 21 13 28 13 0"/>
          </svg>
          <span
            class="absolute -bottom-0.5 -right-0.5 w-2 h-2 rounded-full border border-brand-sidebar {scrobblingPaused ? 'bg-amber-500' : 'bg-emerald-500'}"
            data-testid="musicbrainz-status-dot"
            role={scrobblingPaused ? 'img' : undefined}
            aria-label={scrobblingPaused ? pausedLabel : undefined}
          ></span>
        </div>
        {#if !layoutCollapsed}
          <span class="truncate text-xs font-semibold text-brand-text-primary text-left flex-1 min-w-0">
            {musicbrainzStore.username}
          </span>
        {/if}
      </button>
    {:else}
      <button
        bind:this={profileButtonEl}
        onclick={() => { showLoginModal = true; }}
        class="flex items-center gap-3 transition-colors duration-150 text-brand-text-secondary hover:bg-brand-accent/10 hover:text-brand-accent-text-hover {layoutCollapsed ? 'justify-center w-10 h-10 rounded-xl p-0' : 'w-full px-3 py-1.5 rounded-lg text-sm font-medium'}"
        title={i18n.t('sidebar.musicbrainzLogin', {}, 'Log In to MusicBrainz')}
      >
        <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 25 28" class="{layoutCollapsed ? 'w-5 h-5' : 'w-4 h-4 icon-align'} shrink-0">
          <polygon fill="#ba478f" points="12 0 0 7 0 21 12 28 12 0"/>
          <polygon fill="#eb743b" points="13 0 25 7 25 21 13 28 13 0"/>
        </svg>
        {#if !layoutCollapsed}
          <span class="truncate whitespace-nowrap">{i18n.t('sidebar.musicbrainzLogin', {}, 'Log In to MusicBrainz')}</span>
        {/if}
      </button>
    {/if}
  </div>

  {#if showLoginModal && !musicbrainzStore.isLoggedIn}
    <MusicBrainzLoginModal onClose={() => (showLoginModal = false)} />
  {/if}

  <MusicBrainzProfilePopover
    isOpen={showProfilePopover}
    anchorEl={profileButtonEl}
    onClose={() => (showProfilePopover = false)}
  />
  {/if}

  <!-- Bottom spacer for player bar -->
  <div class:mb-24={!!playerStore.currentSong}></div>
</aside>

<style>
  /* Icons sit in a flex row centered against label text, but text's visible
     glyphs sit lower than its line box center (line-height leading above/below
     cap height isn't symmetric), so a geometrically-centered icon reads as too
     high. Nudge down by half the leading to match the text's optical center. */
  :global(.icon-align) {
    margin-top: calc((1lh - 1em) / 2);
  }

  /* Nothing ever renders behind this panel but the flat bg-main canvas,
     so it paints the glass result as a solid color instead of running a
     backdrop-filter (see flatGlassColor() in theme.svelte.ts). */
  aside.glass-surface {
    position: relative;
    -webkit-backdrop-filter: none !important;
    backdrop-filter: none !important;
    background-color: var(--glass-solid-sidebar) !important;
    border-color: var(--glass-border-color, var(--color-border)) !important;
    box-shadow: var(--glass-shadow, none);
  }
</style>
