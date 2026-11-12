<script lang="ts">
  import { collectionStore } from "../stores/collection.svelte";
  import { navigationStore, type ActiveTab } from "../stores/navigation.svelte";
  import { playlistsStore } from "../stores/playlists.svelte";
  import { playerStore } from "../stores/player.svelte";
  import { themeStore } from "../stores/theme.svelte";
  import { i18n } from "../stores/i18n.svelte";
  import { updaterStore } from "../stores/updater.svelte";
  import { walkthroughStore } from "../stores/walkthrough.svelte";
  import { musicbrainzStore } from "../stores/musicbrainz.svelte";
  import { prefs } from "../stores/prefs.svelte";
  import { scrobblerStore } from "../stores/scrobbler.svelte";
  import MusicBrainzProfilePopover from "./MusicBrainzProfilePopover.svelte";
  import PinnedNavList from "./PinnedNavList.svelte";
  import SidebarFilesSection from "./files/SidebarFilesSection.svelte";
  import { untrack, onMount, type Component } from "svelte";
  import { fade, slide } from "../utils/motion";
  import { songDrag } from "../stores/songDrag.svelte";
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
    LightningIcon as Lightning,
    LightningSlashIcon as LightningSlash,
  } from "phosphor-svelte";
  import { SIDEBAR_MIN_WIDTH_PX } from "../constants";
  import { SIDEBAR_SECTION_LABEL_CLASS, sidebarSubItemClass } from "../utils/sidebarNav";

  let { width = 256, resizing = false }: { width?: number; resizing?: boolean } = $props();

  let isCollapsed = $derived(width < SIDEBAR_MIN_WIDTH_PX);
  let showUpdateBadge = $derived(
    updaterStore.updateAvailable || updaterStore.installStatus === "ready-to-restart",
  );
  let showProfilePopover = $state(false);
  let isConnected = $derived(
    musicbrainzStore.isLoggedIn || !!scrobblerStore.lastfmUsername || !!scrobblerStore.username,
  );
  const scrobblingPaused = $derived(scrobblerStore.status === "paused");
  // Undefined unless paused, so the dot and title only carry text when the state differs from the default.
  const pausedLabel = $derived(scrobblingPaused ? i18n.t("sidebar.scrobblingPaused") : undefined);
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
      const timer = setTimeout(() => {
        layoutCollapsed = true;
      }, 60);
      return () => clearTimeout(timer);
    } else {
      layoutCollapsed = false;
      const timer = setTimeout(() => {
        subItemsVisible = true;
      }, 60);
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

  function openPlaylistsSubTab(subTab: "auto" | "custom") {
    navigationStore.activeTab = "playlists";
    navigationStore.playlistsSubTab = subTab;
    navigationStore.selectedPlaylistId = null;
    navigationStore.selectedAutoPlaylist = null;
  }

  let customExpandTimer: ReturnType<typeof setTimeout> | undefined;
  let dragExpandedCustom = $state(false);

  $effect(() => {
    if (!songDrag.isDragging) {
      dragExpandedCustom = false;
      clearTimeout(customExpandTimer);
      customExpandTimer = undefined;
      return;
    }

    if (songDrag.dropTarget?.kind === "custom-category") {
      if (!dragExpandedCustom && !customExpandTimer) {
        customExpandTimer = setTimeout(() => {
          dragExpandedCustom = true;
          customExpandTimer = undefined;
        }, 500);
      }
    } else {
      if (!dragExpandedCustom && customExpandTimer) {
        clearTimeout(customExpandTimer);
        customExpandTimer = undefined;
      }
    }
  });

  $effect(() => () => clearTimeout(customExpandTimer));
</script>

<!-- One top-level entry: icon + label, or icon only when collapsed. `updateBadge` is the
     "update available" marker (Settings only): it rides in the label row when expanded, on
     the icon corner when collapsed, and is spelled out in the tooltip. -->
{#snippet navButton(
  tab: ActiveTab,
  label: string,
  Icon: Component<{ class?: string }>,
  opts: { walkthroughTarget?: string; updateBadge?: boolean } = {},
)}
  <button
    data-walkthrough-target={opts.walkthroughTarget}
    onclick={() => {
      navigationStore.activeTab = tab;
    }}
    class="relative flex items-center gap-3 transition-colors duration-150 {navigationStore.activeTab ===
    tab
      ? 'bg-brand-accent text-brand-accent-contrast shadow-lg shadow-brand-accent/20'
      : 'text-brand-text-secondary hover:bg-brand-accent/10 hover:text-brand-accent-text-hover'} {layoutCollapsed
      ? 'justify-center w-10 h-10 rounded-xl p-0'
      : 'w-full px-3 py-1.5 rounded-lg text-sm font-medium'}"
    title={opts.updateBadge
      ? `${label} (${i18n.t("settings.updateAvailable", {}, "Update available")})`
      : label}
  >
    {#if layoutCollapsed}
      <Icon class="w-5 h-5" />
      {#if opts.updateBadge}
        <span
          class="absolute top-0.5 right-0.5 p-0.5 rounded-full bg-current/20 border border-current/30 text-current flex items-center justify-center"
        >
          <ArrowUp class="w-2.5 h-2.5 stroke-[2.5]" />
        </span>
      {/if}
    {:else}
      <div class="w-7 h-7 flex items-center justify-center shrink-0">
        <Icon class="w-4 h-4" />
      </div>
      <span class="truncate whitespace-nowrap flex-1 text-left">{label}</span>
      {#if opts.updateBadge}
        <span
          class="px-1.5 py-0.5 rounded-full bg-current/15 border border-current/25 text-current flex items-center gap-0.5 text-[10px] font-bold"
        >
          <ArrowUp class="w-3 h-3 stroke-[2.5]" />
        </span>
      {/if}
    {/if}
  </button>
{/snippet}

<!-- Quiet section heading for the expanded sidebar: sentence case, not a button. The collapsed
     sidebar has no room for text and keeps one icon button per section instead. -->
{#snippet sectionLabel(text: string)}
  <div class={SIDEBAR_SECTION_LABEL_CLASS}>{text}</div>
{/snippet}

<aside
  style="width: {width}px;"
  class="bg-brand-sidebar flex flex-col h-full text-brand-text-secondary select-none flex-shrink-0 overflow-hidden transition-[width] duration-200 ease-out {themeStore.isGlassTheme
    ? 'glass-surface'
    : ''}"
  class:transition-none={resizing}
>
  <nav
    data-walkthrough-target="sidebar"
    class="{layoutCollapsed
      ? 'p-2'
      : 'p-4'} space-y-0.5 flex flex-col items-center flex-1 min-h-0 overflow-y-auto overflow-x-hidden [&>*]:shrink-0"
  >
    {@render navButton("home", i18n.t("sidebar.home"), Home)}

    <PinnedNavList collapsed={layoutCollapsed} />

    <!-- Library/Playlists/Organize/Stats stay hidden until the library has
         songs — except during the walkthrough, whose "sidebar" step
         narrates all of them, so an empty-library first-run tour would
         otherwise describe nav items the user can't see. -->
    {#if !collectionStore.statsLoaded || collectionStore.stats.total_songs > 0 || walkthroughStore.isActive}
      <div class="w-full flex flex-col {layoutCollapsed ? 'items-center' : ''}">
        {#if layoutCollapsed}
          <button
            onclick={selectCollectionTab}
            class="flex items-center justify-center w-10 h-10 rounded-xl p-0 transition-colors duration-150 {navigationStore.activeTab ===
            'collection'
              ? 'bg-brand-accent text-brand-accent-contrast shadow-lg shadow-brand-accent/20'
              : 'text-brand-text-secondary hover:bg-brand-accent/10 hover:text-brand-accent-text-hover'}"
            title={i18n.t("sidebar.collection")}
          >
            {#if navigationStore.activeTab === "collection" && navigationStore.activeSubTab === "artists"}
              <Mic2 class="w-5 h-5" />
            {:else if navigationStore.activeTab === "collection" && navigationStore.activeSubTab === "albums"}
              <DiscAlbum class="w-5 h-5" />
            {:else if navigationStore.activeTab === "collection" && navigationStore.activeSubTab === "songs"}
              <Music class="w-5 h-5" />
            {:else if navigationStore.activeTab === "collection" && navigationStore.activeSubTab === "genres"}
              <Tag class="w-5 h-5" />
            {:else}
              <Library class="w-5 h-5" />
            {/if}
          </button>
        {:else}
          {@render sectionLabel(i18n.t("sidebar.collection"))}
        {/if}

        {#if subItemsVisible}
          <div class="w-full py-0.5 space-y-0">
            <button
              onclick={() => {
                navigationStore.activeTab = "collection";
                navigationStore.activeSubTab = "artists";
                navigationStore.selectedArtistName = null;
                navigationStore.selectedAlbumName = null;
              }}
              class={sidebarSubItemClass(
                navigationStore.activeTab === "collection" &&
                  navigationStore.activeSubTab === "artists" &&
                  !navigationStore.selectedArtistName &&
                  !navigationStore.selectedAlbumName,
              )}
            >
              <div class="flex items-center gap-2 truncate">
                <Mic2 class="w-3.5 h-3.5" />
                <span class="truncate">{i18n.t("sidebar.artists")}</span>
              </div>
            </button>

            <button
              onclick={() => {
                navigationStore.activeTab = "collection";
                navigationStore.activeSubTab = "albums";
                navigationStore.selectedArtistName = null;
                navigationStore.selectedAlbumName = null;
              }}
              class={sidebarSubItemClass(
                navigationStore.activeTab === "collection" &&
                  navigationStore.activeSubTab === "albums" &&
                  !navigationStore.selectedArtistName &&
                  !navigationStore.selectedAlbumName,
              )}
            >
              <div class="flex items-center gap-2 truncate">
                <DiscAlbum class="w-3.5 h-3.5" />
                <span class="truncate">{i18n.t("sidebar.albums")}</span>
              </div>
            </button>

            <button
              onclick={() => {
                navigationStore.activeTab = "collection";
                navigationStore.activeSubTab = "songs";
                navigationStore.selectedArtistName = null;
                navigationStore.selectedAlbumName = null;
              }}
              class={sidebarSubItemClass(
                navigationStore.activeTab === "collection" &&
                  navigationStore.activeSubTab === "songs" &&
                  !navigationStore.selectedArtistName &&
                  !navigationStore.selectedAlbumName,
              )}
            >
              <div class="flex items-center gap-2 truncate">
                <Music class="w-3.5 h-3.5" />
                <span class="truncate">{i18n.t("sidebar.songs")}</span>
              </div>
            </button>

            <button
              onclick={() => {
                navigationStore.activeTab = "collection";
                navigationStore.activeSubTab = "genres";
                navigationStore.selectedArtistName = null;
                navigationStore.selectedAlbumName = null;
              }}
              class={sidebarSubItemClass(
                navigationStore.activeTab === "collection" &&
                  navigationStore.activeSubTab === "genres" &&
                  !navigationStore.selectedArtistName &&
                  !navigationStore.selectedAlbumName,
              )}
            >
              <div class="flex items-center gap-2 truncate">
                <Tag class="w-3.5 h-3.5" />
                <span class="truncate">{i18n.t("sidebar.genres")}</span>
              </div>
            </button>
          </div>
        {/if}
      </div>

      <div class="w-full flex flex-col {layoutCollapsed ? 'items-center' : ''}">
        {#if layoutCollapsed}
          <button
            onclick={selectPlaylistsTab}
            class="flex items-center justify-center w-10 h-10 rounded-xl p-0 transition-colors duration-150 {navigationStore.activeTab ===
            'playlists'
              ? 'bg-brand-accent text-brand-accent-contrast shadow-lg shadow-brand-accent/20'
              : 'text-brand-text-secondary hover:bg-brand-accent/10 hover:text-brand-accent-text-hover'}"
            title={i18n.t("sidebar.playlists")}
          >
            {#if navigationStore.activeTab === "playlists" && navigationStore.playlistsSubTab === "auto"}
              <Sparkles class="w-5 h-5" />
            {:else}
              <ListMusic class="w-5 h-5" />
            {/if}
          </button>
        {:else}
          {@render sectionLabel(i18n.t("sidebar.playlists"))}
        {/if}

        {#if subItemsVisible}
          <div class="w-full py-0.5 space-y-0">
            <button
              onclick={() => openPlaylistsSubTab("auto")}
              class={sidebarSubItemClass(
                navigationStore.activeTab === "playlists" &&
                  navigationStore.playlistsSubTab === "auto" &&
                  !navigationStore.selectedPlaylistId &&
                  !navigationStore.selectedAutoPlaylist,
              )}
            >
              <div class="flex items-center gap-2 truncate">
                <Sparkles class="w-3.5 h-3.5" />
                <span class="truncate">{i18n.t("sidebar.playlistsAuto")}</span>
              </div>
            </button>

            <button
              onclick={() => openPlaylistsSubTab("custom")}
              data-drop-target={playlistsStore.customPlaylists.length > 0
                ? "custom-category"
                : undefined}
              class="{sidebarSubItemClass(
                navigationStore.activeTab === 'playlists' &&
                  navigationStore.playlistsSubTab === 'custom' &&
                  !navigationStore.selectedPlaylistId &&
                  !navigationStore.selectedAutoPlaylist,
              )} {dragExpandedCustom ||
              (songDrag.isDragging && songDrag.dropTarget?.kind === 'custom-category')
                ? 'ring-2 ring-brand-accent/60 bg-brand-accent/20 text-brand-text-primary font-semibold'
                : ''}"
            >
              <div class="flex items-center gap-2 truncate">
                <ListMusic class="w-3.5 h-3.5" />
                <span class="truncate">{i18n.t("sidebar.playlistsCustom")}</span>
              </div>
            </button>

            <!-- Drop targets only: smart playlists are left out, since their rules decide what they hold
               and the backend's rule sync removes anything added by hand. -->
            {#if dragExpandedCustom && playlistsStore.customPlaylists.length > 0}
              <div
                class="pl-4 pr-1 py-0.5 space-y-0.5 border-l border-brand-accent/30 my-0.5"
                transition:slide={{ duration: 150 }}
              >
                {#each playlistsStore.customPlaylists as pl (pl.id)}
                  <button
                    type="button"
                    data-drop-target="playlist"
                    data-playlist-id={pl.id}
                    data-playlist-name={pl.name}
                    class="w-full flex items-center justify-between px-2 py-1 rounded-md text-[11px] truncate transition-colors {songDrag.isDragging &&
                    songDrag.dropTarget?.kind === 'playlist' &&
                    songDrag.dropTarget.id === pl.id
                      ? 'bg-brand-accent/30 text-brand-text-primary ring-2 ring-brand-accent font-semibold'
                      : 'text-brand-text-secondary hover:bg-brand-accent/15'}"
                    onclick={() => navigationStore.viewPlaylist(pl.id)}
                  >
                    <span class="truncate">{pl.name}</span>
                  </button>
                {/each}
              </div>
            {/if}
          </div>
        {/if}
      </div>
    {/if}

    <!-- Files stays on an empty library: it's where a new user adds music. -->
    <SidebarFilesSection collapsed={layoutCollapsed} showSubItems={subItemsVisible} />

    {#if !collectionStore.statsLoaded || collectionStore.stats.total_songs > 0 || walkthroughStore.isActive}
      {@render navButton("organize", i18n.t("sidebar.organize"), Broom)}
      {@render navButton("stats", i18n.t("sidebar.stats"), BarChart2)}
    {/if}
  </nav>

  <!-- Fixed footer: the entries that never change stay visible however many pins or file
       sources push the nav above into scrolling. The Connect row hides while Offline (#1398);
       Settings and Help do not. -->
  <div
    data-testid="sidebar-footer"
    class="{layoutCollapsed
      ? 'p-2'
      : 'px-4 py-2'} border-t border-brand-border/40 w-full flex flex-col items-center space-y-0.5"
  >
    {@render navButton("settings", i18n.t("sidebar.settings"), Settings, {
      walkthroughTarget: "library-folders",
      updateBadge: showUpdateBadge,
    })}
    {@render navButton("help", i18n.t("sidebar.help"), HelpCircle)}

    {#if prefs.onlineEnabled}
      {#if isConnected}
        <button
          bind:this={profileButtonEl}
          onclick={() => {
            showProfilePopover = !showProfilePopover;
          }}
          class="flex items-center gap-3 transition-colors duration-150 {showProfilePopover
            ? 'bg-brand-accent/20 text-brand-accent-text'
            : 'text-brand-text-secondary hover:bg-brand-accent/10 hover:text-brand-accent-text-hover'} {layoutCollapsed
            ? 'justify-center w-10 h-10 rounded-xl p-0'
            : 'w-full px-3 py-1.5 rounded-lg text-sm font-medium'}"
          title={`${i18n.t("sidebar.connected", {}, "Connected")}${pausedLabel ? ` – ${pausedLabel}` : ""}`}
        >
          <div class="shrink-0 flex items-center justify-center {layoutCollapsed ? '' : 'w-7 h-7'}">
            <!-- Inner wrapper anchors the status dot to the icon, not the w-7 slot. -->
            <div class="relative flex items-center justify-center">
              <Lightning class="{layoutCollapsed ? 'w-5 h-5' : 'w-4 h-4'} shrink-0" />
              <span
                class="absolute -bottom-0.5 -right-0.5 w-2 h-2 rounded-full border {scrobblingPaused
                  ? 'bg-transparent border-brand-text-primary'
                  : 'bg-brand-text-primary border-brand-sidebar'}"
                data-testid="musicbrainz-status-dot"
                role={pausedLabel ? "img" : undefined}
                aria-label={pausedLabel}
              ></span>
            </div>
          </div>
          {#if !layoutCollapsed}
            <span
              class="truncate text-xs font-semibold text-brand-text-primary text-left flex-1 min-w-0"
            >
              {i18n.t("sidebar.connected", {}, "Connected")}
            </span>
          {/if}
        </button>
      {:else}
        <button
          onclick={() => {
            navigationStore.openSettings("integrations");
          }}
          class="flex items-center gap-3 transition-colors duration-150 text-brand-text-secondary hover:bg-brand-accent/10 hover:text-brand-accent-text-hover {layoutCollapsed
            ? 'justify-center w-10 h-10 rounded-xl p-0'
            : 'w-full px-3 py-1.5 rounded-lg text-sm font-medium'}"
          title={i18n.t("sidebar.connectIntegrations", {}, "Connect...")}
        >
          <div class="shrink-0 flex items-center justify-center {layoutCollapsed ? '' : 'w-7 h-7'}">
            <LightningSlash class="{layoutCollapsed ? 'w-5 h-5' : 'w-4 h-4'} shrink-0" />
          </div>
          {#if !layoutCollapsed}
            <span class="text-left leading-tight min-w-0"
              >{i18n.t("sidebar.connectIntegrations", {}, "Connect...")}</span
            >
          {/if}
        </button>
      {/if}
    {/if}
  </div>

  {#if prefs.onlineEnabled}
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
  /* Horizontal centre of the expanded nav icons, measured from the content edge
     inside the sidebar's p-4: nav button px-3 (12) + half the w-7 icon slot
     (14). The footer's logo slot uses the same padding, and the sub-item rows
     start their 14px icons at axis - 7px so they share this line, so change
     this value together with those paddings. */
  aside {
    --sidebar-icon-axis: 26px;
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
