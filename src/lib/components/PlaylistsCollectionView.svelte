<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { onMount } from "svelte";
  import { open } from "@tauri-apps/plugin-dialog";
  import { collectionStore } from "../stores/collection.svelte";
  import { navigationStore } from "../stores/navigation.svelte";
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
    kind: "favourites" | "recently_added" | "most_played" | "history" | "genre" | "decade" | "bpm" | "artist_tag" | "missing_metadata";
    genre?: string;
    artistTag?: string;
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
      // Auto-playlists (genre, decade, BPM, and artist tags) are materialized as real (dynamic_enabled) playlist
      // rows, refreshed at most once every 24h — sync then re-pull the list.
      await invoke("sync_all_auto_playlists");
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
      // Pick up genres/decades/BPM buckets/artist tags that just crossed the auto-playlist
      // threshold, and prune ones that no longer have any matching songs.
      await invoke("sync_all_auto_playlists");
      await playlistsStore.refreshPlaylists();

      // Force-regenerate every dynamic playlist (genre/decade auto-playlists
      // and user-created Smart Playlists) with the latest matching songs,
      // bypassing the 24h staleness gate the background sync uses.
      await invoke("refresh_all_auto_playlists");

      await playlistsStore.refreshAutoPlaylistCounts();
    } catch (err) {
      console.error("Failed to refresh playlists:", err);
    } finally {
      isRefreshingAll = false;
    }
  }

  // System genre auto-playlists are keyed one row per curated tag (#548),
  // stored as "tag:<name>" in dynamic_spec — e.g. "tag:Rock", "tag:Jazz".
  let genreAutoPlaylists = $derived(
    playlistsStore.playlists.filter((p) => p.dynamic_enabled && p.dynamic_spec?.startsWith("tag:"))
  );
  let artistTagAutoPlaylists = $derived(
    playlistsStore.playlists.filter((p) => p.dynamic_enabled && p.dynamic_spec?.startsWith("artisttag:"))
  );
  let decadeAutoPlaylists = $derived(playlistsStore.playlists.filter((p) => p.dynamic_enabled && p.dynamic_spec?.startsWith("decade:")));
  let bpmAutoPlaylists = $derived(
    playlistsStore.playlists
      .filter((p) => p.dynamic_enabled && p.dynamic_spec?.startsWith("bpmrange:"))
      .sort((a, b) => BPM_BUCKET_ORDER.indexOf(a.name) - BPM_BUCKET_ORDER.indexOf(b.name))
  );
  // Missing Metadata (#367) is a singleton diagnostic auto-playlist, not a
  // per-value category like genre/decade/BPM/artist tag.
  let missingMetadataAutoPlaylist = $derived(
    playlistsStore.playlists.find((p) => p.dynamic_enabled && p.dynamic_spec === "missingmeta")
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
    if (playlistsStore.mostPlayedCount > 0) {
      defs.push({
        id: "auto:most_played",
        kind: "most_played",
        label: i18n.t("playlists.autoMostPlayed"),
        trackCount: playlistsStore.mostPlayedCount,
      });
    }
    defs.push({
      id: "auto:history",
      kind: "history",
      label: i18n.t("playlists.autoHistory"),
      trackCount: playlistsStore.historyCount,
    });
    if (missingMetadataAutoPlaylist && missingMetadataAutoPlaylist.track_count > 0) {
      defs.push({
        id: `auto:missing_metadata:${missingMetadataAutoPlaylist.id}`,
        kind: "missing_metadata",
        label: getPlaylistDisplayName(missingMetadataAutoPlaylist),
        playlistId: missingMetadataAutoPlaylist.id,
        updated: missingMetadataAutoPlaylist.updated,
        trackCount: missingMetadataAutoPlaylist.track_count,
      });
    }
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
          genre: p.dynamic_spec?.replace(/^tag:/, "") ?? p.name,
          label: getPlaylistDisplayName(p),
          playlistId: p.id,
          updated: p.updated,
          trackCount: p.track_count,
        });
      }
    }
    for (const p of artistTagAutoPlaylists) {
      if (p.track_count > 0) {
        defs.push({
          id: `auto:artist_tag:${p.id}`,
          kind: "artist_tag",
          artistTag: p.dynamic_spec?.replace(/^artisttag:/, "") ?? p.name,
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
  // order applied to decade, genre & artist tag auto-playlists. BPM auto-playlists always
  // sort last, in their fixed intensity order (Down-Tempo → Extreme, set by
  // BPM_BUCKET_ORDER above) — never interleaved into the name/track_count/
  // updated sort applied to genre/decade/artist tag, which would otherwise scramble them
  // (e.g. "High Energy BPM" sorting alphabetically between two genres).
  let sortedAutoDefs = $derived.by(() => {
    const field = autoSortField;
    const asc = autoSortAsc;
    const pinned = autoDefs.filter((d) => d.kind !== "genre" && d.kind !== "decade" && d.kind !== "artist_tag" && d.kind !== "bpm");
    const rest = autoDefs
      .filter((d) => d.kind === "genre" || d.kind === "decade" || d.kind === "artist_tag")
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
    navigationStore.playlistsSubTab === "auto" ? prefs.playlistsAutoViewMode : prefs.playlistsCustomViewMode
  );

  function setActiveViewMode(mode: CollectionViewMode) {
    if (navigationStore.playlistsSubTab === "auto") {
      prefs.setPlaylistsAutoViewMode(mode);
    } else {
      prefs.setPlaylistsCustomViewMode(mode);
    }
  }

  function openAuto(def: AutoDef) {
    navigationStore.viewAutoPlaylist(
      def.kind === "genre"
        ? { kind: "genre", genre: def.genre, playlistId: def.playlistId, updated: def.updated }
        : def.kind === "artist_tag"
          ? { kind: "artist_tag", artistTag: def.artistTag, playlistId: def.playlistId, updated: def.updated }
          : def.kind === "decade"
            ? { kind: "decade", decade: def.decade, playlistId: def.playlistId, updated: def.updated }
            : def.kind === "bpm"
              ? { kind: "bpm", bpm: def.bpm, playlistId: def.playlistId, updated: def.updated }
              : def.kind === "missing_metadata"
                ? { kind: "missing_metadata", playlistId: def.playlistId, updated: def.updated }
                : { kind: def.kind }
    );
  }

  function openPlaylist(pl: Playlist) {
    playlistsStore.selectPlaylist(pl.id);
    navigationStore.viewPlaylist(pl.id);
  }

  async function handleCreateBlankPlaylist() {
    try {
      const playlist = await playlistsStore.createPlaylist(i18n.t("playlists.untitledPlaylistName"));
      if (playlist) {
        navigationStore.viewPlaylist(playlist.id);
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
          navigationStore.viewPlaylist(playlistsStore.activePlaylistId);
        }
      }
    } catch (err) {
      console.error("Failed to import playlist:", err);
    }
  }
</script>

{#if navigationStore.selectedPlaylistId !== null}
  <PlaylistView />
{:else if navigationStore.selectedAutoPlaylist !== null}
  <AutoPlaylistDetailView view={navigationStore.selectedAutoPlaylist} />
{:else}
  <div class="flex-1 flex flex-col overflow-hidden bg-brand-main text-brand-text-secondary h-full">
    <div class="flex-1 px-6 overflow-y-auto {playerStore.currentSong ? 'pb-28' : 'pb-6'}" use:rememberScroll={`playlists:${navigationStore.playlistsSubTab}`}>
      <div class="sticky top-0 z-20 bg-brand-main pt-3">
        {#if navigationStore.playlistsSubTab === "custom"}
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

        <div class="h-12 flex items-center justify-between">
          <div class="text-xs text-brand-text-secondary font-medium">
            {#if navigationStore.playlistsSubTab === "auto"}
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
            {#if navigationStore.playlistsSubTab === "auto"}
              <Select
                value={`${autoSortField}-${autoSortAsc}`}
                onchange={(e) => {
                  const [field, asc] = e.currentTarget.value.split("-");
                  autoSortField = field as "name" | "track_count" | "updated";
                  autoSortAsc = asc === "true";
                }}
                class="bg-brand-sidebar border border-brand-border hover:border-brand-accent/60 text-brand-text-secondary text-xs rounded-full pl-3.5 pr-8 py-1.5 focus:outline-none focus:border-brand-accent transition-all font-medium"
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
                class="bg-brand-sidebar border border-brand-border hover:border-brand-accent/60 text-brand-text-secondary text-xs rounded-full pl-3.5 pr-8 py-1.5 focus:outline-none focus:border-brand-accent transition-all font-medium"
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
        {#if navigationStore.playlistsSubTab === "auto"}
          <div class="grid {activeViewMode === 'rows' ? 'grid-cols-[repeat(auto-fill,minmax(280px,1fr))] gap-2' : 'grid-cols-[repeat(auto-fill,minmax(180px,1fr))] gap-5'}">
            {#each sortedAutoDefs as def (def.id)}
              {#if activeViewMode === "rows"}
                <AutoPlaylistRowCard
                  label={def.label}
                  kind={def.kind}
                  genre={def.genre}
                  artistTag={def.artistTag}
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
                  artistTag={def.artistTag}
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
