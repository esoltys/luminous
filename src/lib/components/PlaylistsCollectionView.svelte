<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { onMount } from "svelte";
  import { open } from "@tauri-apps/plugin-dialog";
  import { collectionStore } from "../stores/collection.svelte";
  import { playlistsStore } from "../stores/playlists.svelte";
  import { playerStore } from "../stores/player.svelte";
  import { i18n } from "../stores/i18n.svelte";
  import { prefs, type CollectionViewMode } from "../stores/prefs.svelte";
  import type { Playlist } from "../types";
  import PlaylistCard from "./PlaylistCard.svelte";
  import PlaylistRowCard from "./PlaylistRowCard.svelte";
  import AutoPlaylistCard from "./AutoPlaylistCard.svelte";
  import AutoPlaylistRowCard from "./AutoPlaylistRowCard.svelte";
  import PlaylistView from "./PlaylistView.svelte";
  import AutoPlaylistDetailView from "./AutoPlaylistDetailView.svelte";
  import SmartPlaylistBuilderModal from "./SmartPlaylistBuilderModal.svelte";
  import EmptyState from "./EmptyState.svelte";
  import Select from "./Select.svelte";
  import Button from "./Button.svelte";
  import { FolderInput, Plus, ListMusic, Sparkles, LayoutGrid, Rows3, RefreshCw } from "lucide-svelte";
  import { isSmartPlaylistSpec } from "../utils/filterParser";
  import { getPlaylistDisplayName } from "../utils/playlist";
  import { rememberScroll } from "../utils/scrollMemory";

  interface AutoDef {
    id: string;
    kind: "favourites" | "recently_added" | "history" | "genre" | "decade" | "bpm";
    genre?: string;
    decade?: string;
    bpm?: string;
    label: string;
    playlistId?: number;
    updated?: number;
    trackCount: number;
  }

  // Fixed intensity order for the BPM auto-playlist bucket names (mirrors
  // src-tauri/src/playlist.rs's BPM_BUCKETS) — alphabetical sort would
  // scramble Down-Tempo/Mid-Tempo/Uptempo/High Energy/Extreme.
  const BPM_BUCKET_ORDER = ["Down-Tempo BPM", "Mid-Tempo BPM", "Uptempo BPM", "High Energy BPM", "Extreme BPM"];

  onMount(async () => {
    try {
      // Auto-playlists (genre, decade, and BPM) are materialized as real (dynamic_enabled) playlist
      // rows, refreshed at most once every 24h — sync then re-pull the list.
      await invoke("sync_genre_auto_playlists");
      await invoke("sync_decade_auto_playlists");
      await invoke("sync_bpm_auto_playlists");
      await playlistsStore.refreshPlaylists();
    } catch (err) {
      console.error("Failed to sync auto-playlists:", err);
    }
    await playlistsStore.refreshAutoPlaylistCounts();
  });

  let isRefreshingAll = $state(false);

  async function handleRefreshAll() {
    if (isRefreshingAll) return;
    isRefreshingAll = true;
    try {
      // Pick up genres/decades/BPM buckets that just crossed the auto-playlist
      // threshold, and prune ones that no longer have any matching songs.
      await invoke("sync_genre_auto_playlists");
      await invoke("sync_decade_auto_playlists");
      await invoke("sync_bpm_auto_playlists");
      await playlistsStore.refreshPlaylists();

      // Force-regenerate every dynamic playlist (genre/decade auto-playlists
      // and user-created Smart Playlists) with the latest matching songs,
      // bypassing the 24h staleness gate the background sync uses.
      const dynamicIds = playlistsStore.playlists.filter((p) => p.dynamic_enabled).map((p) => p.id);
      await Promise.all(dynamicIds.map((id) => invoke("refresh_auto_playlist", { playlistId: id })));

      await playlistsStore.refreshAutoPlaylistCounts();
    } catch (err) {
      console.error("Failed to refresh playlists:", err);
    } finally {
      isRefreshingAll = false;
    }
  }

  // System genre auto-playlists are stored with a raw genre name as dynamic_spec (e.g. "Rock", "Jazz").
  // Smart playlists built via the Smart Playlist builder always contain a "field:value" rule (e.g. "genre:jazz rating:>=4").
  let genreAutoPlaylists = $derived(
    playlistsStore.playlists.filter(
      (p) =>
        p.dynamic_enabled &&
        !p.dynamic_spec?.startsWith("decade:") &&
        !p.dynamic_spec?.startsWith("bpmrange:") &&
        !isSmartPlaylistSpec(p.dynamic_spec)
    )
  );
  let decadeAutoPlaylists = $derived(playlistsStore.playlists.filter((p) => p.dynamic_enabled && p.dynamic_spec?.startsWith("decade:")));
  let bpmAutoPlaylists = $derived(
    playlistsStore.playlists
      .filter((p) => p.dynamic_enabled && p.dynamic_spec?.startsWith("bpmrange:"))
      .sort((a, b) => BPM_BUCKET_ORDER.indexOf(a.name) - BPM_BUCKET_ORDER.indexOf(b.name))
  );
  let customPlaylists = $derived.by(() => {
    // Include non-dynamic playlists + user-created Smart playlists
    const list = playlistsStore.playlists.filter((p) => !p.dynamic_enabled || isSmartPlaylistSpec(p.dynamic_spec));
    const queue = list.find((p) => p.is_queue);
    const rest = list.filter((p) => !p.is_queue);
    return queue ? [queue, ...rest] : rest;
  });

  // Auto-playlists that currently resolve to 0 songs are hidden entirely
  // (e.g. a genre's tags got edited away, or no songs are rated 5 stars).
  let autoDefs = $derived.by((): AutoDef[] => {
    const defs: AutoDef[] = [];
    if (playlistsStore.favouritesCount > 0) {
      defs.push({
        id: "auto:favourites",
        kind: "favourites",
        label: i18n.t("playlists.autoFavourites"),
        trackCount: playlistsStore.favouritesCount,
      });
    }
    if (playlistsStore.recentlyAddedCount > 0) {
      defs.push({
        id: "auto:recently_added",
        kind: "recently_added",
        label: i18n.t("playlists.autoRecentlyAdded"),
        trackCount: playlistsStore.recentlyAddedCount,
      });
    }
    defs.push({
      id: "auto:history",
      kind: "history",
      label: i18n.t("playlists.autoHistory"),
      trackCount: playlistsStore.historyCount,
    });
    for (const p of decadeAutoPlaylists) {
      if (p.track_count > 0) {
        const dec = p.dynamic_spec?.replace(/^decade:/, "") ?? p.name;
        defs.push({
          id: `auto:decade:${p.id}`,
          kind: "decade",
          decade: dec,
          label: getPlaylistDisplayName(p),
          playlistId: p.id,
          updated: p.updated,
          trackCount: p.track_count,
        });
      }
    }
    for (const p of genreAutoPlaylists) {
      if (p.track_count > 0) {
        defs.push({
          id: `auto:genre:${p.id}`,
          kind: "genre",
          genre: p.dynamic_spec?.replace(/^genre:/, "") ?? p.name,
          label: getPlaylistDisplayName(p),
          playlistId: p.id,
          updated: p.updated,
          trackCount: p.track_count,
        });
      }
    }
    for (const p of bpmAutoPlaylists) {
      if (p.track_count > 0) {
        defs.push({
          id: `auto:bpm:${p.id}`,
          kind: "bpm",
          bpm: p.dynamic_spec?.replace(/^bpmrange:/, "") ?? "",
          label: getPlaylistDisplayName(p),
          playlistId: p.id,
          updated: p.updated,
          trackCount: p.track_count,
        });
      }
    }
    return defs;
  });

  // ---- Auto grid sort (mirrors the Custom grid's field+direction sort) ----
  let autoSortField = $state<"name" | "track_count" | "updated">(
    (typeof window !== "undefined" &&
      (localStorage.getItem("sort_auto_playlist_field") as "name" | "track_count" | "updated")) ||
      "name"
  );
  let autoSortAsc = $state(
    typeof window !== "undefined" ? localStorage.getItem("sort_auto_playlist_asc") !== "false" : true
  );

  // Favourites/Recently Added are always pinned first, ahead of the sort
  // order applied to decade & genre auto-playlists. BPM auto-playlists always
  // sort last, in their fixed intensity order (Down-Tempo → Extreme, set by
  // BPM_BUCKET_ORDER above) — never interleaved into the name/track_count/
  // updated sort applied to genre/decade, which would otherwise scramble them
  // (e.g. "High Energy BPM" sorting alphabetically between two genres).
  let sortedAutoDefs = $derived.by(() => {
    const field = autoSortField;
    const asc = autoSortAsc;
    const pinned = autoDefs.filter((d) => d.kind !== "genre" && d.kind !== "decade" && d.kind !== "bpm");
    const rest = autoDefs
      .filter((d) => d.kind === "genre" || d.kind === "decade")
      .sort((a, b) => {
        if (field === "name") {
          return asc ? a.label.localeCompare(b.label) : b.label.localeCompare(a.label);
        }
        const valA = field === "track_count" ? a.trackCount : (a.updated ?? 0);
        const valB = field === "track_count" ? b.trackCount : (b.updated ?? 0);
        return asc ? valA - valB : valB - valA;
      });
    const bpmBlock = autoDefs.filter((d) => d.kind === "bpm");
    return [...pinned, ...rest, ...bpmBlock];
  });

  // ---- Custom grid sort ----
  let customSortField = $state<"name" | "track_count" | "updated">(
    (typeof window !== "undefined" &&
      (localStorage.getItem("sort_custom_playlist_field") as "name" | "track_count" | "updated")) ||
      "name"
  );
  let customSortAsc = $state(
    typeof window !== "undefined" ? localStorage.getItem("sort_custom_playlist_asc") !== "false" : true
  );

  $effect(() => {
    if (typeof window !== "undefined") {
      localStorage.setItem("sort_auto_playlist_field", autoSortField);
      localStorage.setItem("sort_auto_playlist_asc", autoSortAsc.toString());
      localStorage.setItem("sort_custom_playlist_field", customSortField);
      localStorage.setItem("sort_custom_playlist_asc", customSortAsc.toString());
    }
  });

  let sortedPlaylists = $derived.by(() => {
    const field = customSortField;
    const asc = customSortAsc;
    return customPlaylists
      .filter((p) => !p.is_queue)
      .sort((a, b) => {
        if (field === "name") {
          return asc ? a.name.localeCompare(b.name) : b.name.localeCompare(a.name);
        }
        const valA = field === "track_count" ? a.track_count : a.updated;
        const valB = field === "track_count" ? b.track_count : b.updated;
        return asc ? valA - valB : valB - valA;
      });
  });

  let activeViewMode = $derived(
    collectionStore.playlistsSubTab === "auto" ? prefs.playlistsAutoViewMode : prefs.playlistsCustomViewMode
  );

  function setActiveViewMode(mode: CollectionViewMode) {
    if (collectionStore.playlistsSubTab === "auto") {
      prefs.setPlaylistsAutoViewMode(mode);
    } else {
      prefs.setPlaylistsCustomViewMode(mode);
    }
  }

  function openAuto(def: AutoDef) {
    collectionStore.viewAutoPlaylist(
      def.kind === "genre"
        ? { kind: "genre", genre: def.genre, playlistId: def.playlistId, updated: def.updated }
        : def.kind === "decade"
          ? { kind: "decade", decade: def.decade, playlistId: def.playlistId, updated: def.updated }
          : def.kind === "bpm"
            ? { kind: "bpm", bpm: def.bpm, playlistId: def.playlistId, updated: def.updated }
            : { kind: def.kind }
    );
  }

  function openPlaylist(pl: Playlist) {
    playlistsStore.selectPlaylist(pl.id);
    collectionStore.viewPlaylist(pl.id);
  }

  async function handleCreateBlankPlaylist() {
    try {
      const playlist = await playlistsStore.createPlaylist(i18n.t("playlists.untitledPlaylistName"));
      if (playlist) {
        collectionStore.viewPlaylist(playlist.id);
      }
    } catch (err) {
      console.error("Failed to create playlist:", err);
    }
  }

  async function handleImportPlaylist() {
    try {
      const selected = await open({
        multiple: false,
        title: i18n.t("playlists.importPlaylistTooltip"),
        filters: [{ name: "Playlists (*.m3u, *.m3u8, *.pls, *.xspf)", extensions: ["m3u", "m3u8", "pls", "xspf"] }],
      });
      if (selected && typeof selected === "string") {
        await playlistsStore.importPlaylist(selected);
        if (playlistsStore.activePlaylistId !== null) {
          collectionStore.viewPlaylist(playlistsStore.activePlaylistId);
        }
      }
    } catch (err) {
      console.error("Failed to import playlist:", err);
    }
  }
</script>

{#if collectionStore.selectedPlaylistId !== null}
  <PlaylistView />
{:else if collectionStore.selectedAutoPlaylist !== null}
  <AutoPlaylistDetailView view={collectionStore.selectedAutoPlaylist} />
{:else}
  <div class="flex-1 flex flex-col overflow-hidden bg-brand-main text-brand-text-secondary h-full">
    <div class="flex-1 px-6 overflow-y-auto {playerStore.currentSong ? 'pb-28' : 'pb-6'}" use:rememberScroll={`playlists:${collectionStore.playlistsSubTab}`}>
      <div class="sticky top-0 z-20 bg-brand-main pt-3">
        {#if collectionStore.playlistsSubTab === "custom"}
          <div class="h-10 flex items-center gap-2 mb-2">
            <Button onclick={handleCreateBlankPlaylist} variant="primary" title={i18n.t('playlists.newPlaylistBtn')}>
              <Plus class="w-4 h-4" />
              <span>{i18n.t('playlists.newPlaylistBtn')}</span>
            </Button>
            <Button onclick={() => collectionStore.openSmartBuilder()} variant="accent-soft" title={i18n.t('playlists.newSmartPlaylistBtn')}>
              <Sparkles class="w-4 h-4" />
              <span>{i18n.t('playlists.newSmartPlaylistBtn')}</span>
            </Button>
            <Button onclick={handleImportPlaylist} variant="secondary" title={i18n.t('playlists.importPlaylistTooltip')}>
              <FolderInput class="w-4 h-4 text-brand-accent-text" />
              <span>{i18n.t('playlists.importPlaylistBtn')}</span>
            </Button>
          </div>
        {/if}

        <div class="h-9 flex items-center justify-between">
          <div class="text-xs text-brand-text-secondary font-medium">
            {#if collectionStore.playlistsSubTab === "auto"}
              {sortedAutoDefs.length === 1 ? i18n.t('playlists.showingOnePlaylist') : i18n.t('playlists.showingPlaylists', { count: sortedAutoDefs.length })}
            {:else}
              {sortedPlaylists.length === 1 ? i18n.t('playlists.showingOnePlaylist') : i18n.t('playlists.showingPlaylists', { count: sortedPlaylists.length })}
            {/if}
          </div>

          <div class="flex items-center gap-2">
            <button
              onclick={handleRefreshAll}
              disabled={isRefreshingAll}
              class="flex items-center justify-center w-9 h-9 rounded-full border border-brand-border bg-brand-sidebar text-brand-text-secondary hover:text-brand-accent-text hover:border-brand-accent/60 transition-colors disabled:opacity-50 disabled:cursor-not-allowed shadow-xs"
              title={i18n.t('playlists.refreshAllPlaylistsTooltip')}
              aria-label={i18n.t('playlists.refreshAllPlaylistsTooltip')}
            >
              <RefreshCw class="w-4 h-4 {isRefreshingAll ? 'animate-spin' : ''}" />
            </button>
            <div class="inline-flex items-center gap-0.5 bg-brand-sidebar border border-brand-border rounded-full p-1">
              <button
                onclick={() => setActiveViewMode("cards")}
                class="flex items-center justify-center w-7 h-7 rounded-full transition-colors {activeViewMode === 'cards' ? 'bg-brand-accent text-white' : 'text-brand-text-secondary hover:text-brand-text-primary'}"
                title={i18n.t('collection.viewCards')}
                aria-label={i18n.t('collection.viewCards')}
                aria-pressed={activeViewMode === "cards"}
              >
                <LayoutGrid class="w-4 h-4" />
              </button>
              <button
                onclick={() => setActiveViewMode("rows")}
                class="flex items-center justify-center w-7 h-7 rounded-full transition-colors {activeViewMode === 'rows' ? 'bg-brand-accent text-white' : 'text-brand-text-secondary hover:text-brand-text-primary'}"
                title={i18n.t('collection.viewRows')}
                aria-label={i18n.t('collection.viewRows')}
                aria-pressed={activeViewMode === "rows"}
              >
                <Rows3 class="w-4 h-4" />
              </button>
            </div>
            <div class="relative">
            {#if collectionStore.playlistsSubTab === "auto"}
              <Select
                value={`${autoSortField}-${autoSortAsc}`}
                onchange={(e) => {
                  const [field, asc] = e.currentTarget.value.split("-");
                  autoSortField = field as "name" | "track_count" | "updated";
                  autoSortAsc = asc === "true";
                }}
                class="bg-brand-sidebar border border-brand-border hover:border-brand-accent/60 text-brand-text-secondary text-xs rounded-full pl-2.5 pr-8 py-1.5 focus:outline-none focus:border-brand-accent transition-all font-medium"
              >
                <option value="name-true">▲ {i18n.t('playlists.sortLabelName')}</option>
                <option value="name-false">▼ {i18n.t('playlists.sortLabelName')}</option>
                <option value="track_count-true">▲ {i18n.t('playlists.sortLabelSongs')}</option>
                <option value="track_count-false">▼ {i18n.t('playlists.sortLabelSongs')}</option>
                <option value="updated-true">▲ {i18n.t('playlists.sortLabelUpdated')}</option>
                <option value="updated-false">▼ {i18n.t('playlists.sortLabelUpdated')}</option>
              </Select>
            {:else}
              <Select
                value={`${customSortField}-${customSortAsc}`}
                onchange={(e) => {
                  const [field, asc] = e.currentTarget.value.split("-");
                  customSortField = field as "name" | "track_count" | "updated";
                  customSortAsc = asc === "true";
                }}
                class="bg-brand-sidebar border border-brand-border hover:border-brand-accent/60 text-brand-text-secondary text-xs rounded-full pl-2.5 pr-8 py-1.5 focus:outline-none focus:border-brand-accent transition-all font-medium"
              >
                <option value="name-true">▲ {i18n.t('playlists.sortLabelName')}</option>
                <option value="name-false">▼ {i18n.t('playlists.sortLabelName')}</option>
                <option value="track_count-true">▲ {i18n.t('playlists.sortLabelSongs')}</option>
                <option value="track_count-false">▼ {i18n.t('playlists.sortLabelSongs')}</option>
                <option value="updated-true">▲ {i18n.t('playlists.sortLabelUpdated')}</option>
                <option value="updated-false">▼ {i18n.t('playlists.sortLabelUpdated')}</option>
              </Select>
            {/if}
            </div>
          </div>
        </div>
      </div>

      <div class="pt-2 pb-8">
        {#if collectionStore.playlistsSubTab === "auto"}
          <div class="grid {activeViewMode === 'rows' ? 'grid-cols-[repeat(auto-fill,minmax(280px,1fr))] gap-2' : 'grid-cols-[repeat(auto-fill,minmax(180px,1fr))] gap-5'}">
            {#each sortedAutoDefs as def (def.id)}
              {#if activeViewMode === "rows"}
                <AutoPlaylistRowCard
                  label={def.label}
                  kind={def.kind}
                  genre={def.genre}
                  decade={def.decade}
                  bpm={def.bpm}
                  playlistId={def.playlistId}
                  updated={def.updated}
                  trackCount={def.trackCount}
                  onClick={() => openAuto(def)}
                />
              {:else}
                <AutoPlaylistCard
                  label={def.label}
                  kind={def.kind}
                  genre={def.genre}
                  decade={def.decade}
                  bpm={def.bpm}
                  playlistId={def.playlistId}
                  updated={def.updated}
                  trackCount={def.trackCount}
                  onClick={() => openAuto(def)}
                />
              {/if}
            {/each}
          </div>
        {:else}
          {#if sortedPlaylists.length === 0}
            <div class="col-span-full py-16 text-center">
              <EmptyState
                card
                icon={ListMusic}
                title={i18n.t('playlists.noPlaylistsTitle')}
                subtitle={i18n.t('playlists.noPlaylistsText')}
                subtitleClass="text-xs text-brand-text-secondary font-medium"
              />
            </div>
          {:else}
            <div class="grid {activeViewMode === 'rows' ? 'grid-cols-[repeat(auto-fill,minmax(280px,1fr))] gap-2' : 'grid-cols-[repeat(auto-fill,minmax(180px,1fr))] gap-5'}">
              {#each sortedPlaylists as pl (pl.id)}
                {#if activeViewMode === "rows"}
                  <PlaylistRowCard playlist={pl} onClick={() => openPlaylist(pl)} />
                {:else}
                  <PlaylistCard playlist={pl} onClick={() => openPlaylist(pl)} />
                {/if}
              {/each}
            </div>
          {/if}
        {/if}
      </div>
    </div>
  </div>
{/if}
