<script lang="ts">
  import CollectionHeader from "./CollectionHeader.svelte";
  import ViewModeToggle from "./ViewModeToggle.svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { onMount } from "svelte";
  import { open } from "@tauri-apps/plugin-dialog";
  import { collectionStore } from "../stores/collection.svelte";
  import { navigationStore } from "../stores/navigation.svelte";
  import { playlistsStore } from "../stores/playlists.svelte";
  import { i18n } from "../stores/i18n.svelte";
  import { prefs, type CollectionViewMode } from "../stores/prefs.svelte";
  import type { Playlist } from "../types";
  import PlaylistCard from "./PlaylistCard.svelte";
  import PlaylistRowCard from "./PlaylistRowCard.svelte";
  import AutoPlaylistCard from "./AutoPlaylistCard.svelte";
  import AutoPlaylistRowCard from "./AutoPlaylistRowCard.svelte";
  import PlaylistView from "./PlaylistView.svelte";
  import AutoPlaylistDetailView from "./AutoPlaylistDetailView.svelte";
  import GenreAliasesDialogHost from "./GenreAliasesDialogHost.svelte";
  import SmartPlaylistBuilderModal from "./SmartPlaylistBuilderModal.svelte";
  import PlaylistCardContextMenu from "./PlaylistCardContextMenu.svelte";
  import AutoPlaylistContextMenu from "./AutoPlaylistContextMenu.svelte";
  import EmptyState from "./EmptyState.svelte";
  import Select from "./Select.svelte";
  import Button from "./Button.svelte";
  import {
    FolderOpenIcon as FolderInput,
    PlaylistIcon as ListMusic,
    SparkleIcon as Sparkles,
    ArrowsClockwiseIcon as RefreshCw,
  } from "phosphor-svelte";
  import Plus from "./PlusIcon.svelte";
  import {
    getPlaylistDisplayName,
    isSmartPlaylist,
    isStationList,
    isSystemAutoPlaylist,
  } from "../utils/playlist";
  import { scrobblerStore } from "../stores/scrobbler.svelte";
  import { picardStore } from "../stores/picard.svelte";
  import { rememberScroll } from "../utils/scrollMemory";
  import {
    BPM_BUCKET_ORDER,
    loadPlaylistSortPref,
    savePlaylistSortPref,
    sortByPlaylistField,
    type PlaylistSortField,
  } from "../utils/playlistOrder";

  interface AutoDef {
    id: string;
    kind:
      | "favourites"
      | "recently_added"
      | "most_played"
      | "history"
      | "genre"
      | "decade"
      | "bpm"
      | "artist_tag"
      | "missing_metadata"
      | "missing_musicbrainz"
      | "daypart"
      | "language"
      | "country";
    selector?: string;
    label: string;
    playlistId?: number;
    updated?: number;
    trackCount: number;
  }

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
    playlistsStore.playlists.filter(
      (p) => isSystemAutoPlaylist(p) && p.dynamic_spec?.startsWith("tag:"),
    ),
  );
  let artistTagAutoPlaylists = $derived(
    playlistsStore.playlists.filter(
      (p) => isSystemAutoPlaylist(p) && p.dynamic_spec?.startsWith("artisttag:"),
    ),
  );
  let languageAutoPlaylists = $derived(
    playlistsStore.playlists.filter(
      (p) => isSystemAutoPlaylist(p) && p.dynamic_spec?.startsWith("language:"),
    ),
  );
  let countryAutoPlaylists = $derived(
    playlistsStore.playlists.filter(
      (p) => isSystemAutoPlaylist(p) && p.dynamic_spec?.startsWith("artistcountry:"),
    ),
  );
  let decadeAutoPlaylists = $derived(
    playlistsStore.playlists.filter(
      (p) => isSystemAutoPlaylist(p) && p.dynamic_spec?.startsWith("decade:"),
    ),
  );
  let bpmAutoPlaylists = $derived(
    playlistsStore.playlists
      .filter((p) => isSystemAutoPlaylist(p) && p.dynamic_spec?.startsWith("bpmrange:"))
      .sort((a, b) => BPM_BUCKET_ORDER.indexOf(a.name) - BPM_BUCKET_ORDER.indexOf(b.name)),
  );
  // Missing Metadata (#367) is a singleton diagnostic auto-playlist, not a
  // per-value category like genre/decade/BPM/artist tag.
  let missingMetadataAutoPlaylist = $derived(
    playlistsStore.playlists.find(
      (p) => isSystemAutoPlaylist(p) && p.dynamic_spec === "missingmeta",
    ),
  );
  // Missing MusicBrainz (#83) is surfaced when scrobbling is enabled, identifying
  // songs that lack MusicBrainz recording IDs.
  let missingMusicBrainzAutoPlaylist = $derived(
    playlistsStore.playlists.find(
      (p) => isSystemAutoPlaylist(p) && p.dynamic_spec === "missingmbid",
    ),
  );
  // Daypart Mix (#223) is also a singleton, but unlike missing-metadata its
  // dynamic_spec changes content (bucket/date/genre) every time the daypart
  // boundary crosses — matched by prefix, not exact spec, so the same row
  // (and any Home pin keyed on its id) is found across every crossing.
  let daypartMixPlaylist = $derived(
    playlistsStore.playlists.find(
      (p) => isSystemAutoPlaylist(p) && p.dynamic_spec?.startsWith("daypart:"),
    ),
  );
  let customPlaylists = $derived.by(() => {
    // Include non-dynamic playlists + user-created Smart playlists
    const list = playlistsStore.playlists.filter((p) => !p.dynamic_enabled || isSmartPlaylist(p));
    const queue = list.find((p) => p.is_queue);
    const rest = list.filter((p) => !p.is_queue);
    return queue ? [queue, ...rest] : rest;
  });

  // Auto-playlists that currently resolve to 0 songs are hidden entirely
  // (e.g. a genre's tags got edited away, or no songs are rated 5 stars).
  let autoDefs = $derived.by((): AutoDef[] => {
    const defs: AutoDef[] = [];
    if (daypartMixPlaylist && daypartMixPlaylist.track_count > 0) {
      defs.push({
        id: `auto:daypart:${daypartMixPlaylist.id}`,
        kind: "daypart",
        label: getPlaylistDisplayName(daypartMixPlaylist),
        playlistId: daypartMixPlaylist.id,
        updated: daypartMixPlaylist.updated,
        trackCount: daypartMixPlaylist.track_count,
      });
    }
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
    if (
      picardStore.missingPlaylistEnabled &&
      missingMusicBrainzAutoPlaylist &&
      missingMusicBrainzAutoPlaylist.track_count > 0
    ) {
      defs.push({
        id: `auto:missing_musicbrainz:${missingMusicBrainzAutoPlaylist.id}`,
        kind: "missing_musicbrainz",
        label: getPlaylistDisplayName(missingMusicBrainzAutoPlaylist),
        playlistId: missingMusicBrainzAutoPlaylist.id,
        updated: missingMusicBrainzAutoPlaylist.updated,
        trackCount: missingMusicBrainzAutoPlaylist.track_count,
      });
    }
    for (const p of decadeAutoPlaylists) {
      if (p.track_count > 0) {
        const dec = p.dynamic_spec?.replace(/^decade:/, "") ?? p.name;
        defs.push({
          id: `auto:decade:${p.id}`,
          kind: "decade",
          selector: dec,
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
          selector: p.dynamic_spec?.replace(/^tag:/, "") ?? p.name,
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
          selector: p.dynamic_spec?.replace(/^artisttag:/, "") ?? p.name,
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
          selector: p.dynamic_spec?.replace(/^bpmrange:/, "") ?? "",
          label: getPlaylistDisplayName(p),
          playlistId: p.id,
          updated: p.updated,
          trackCount: p.track_count,
        });
      }
    }
    for (const p of languageAutoPlaylists) {
      if (p.track_count > 0) {
        defs.push({
          id: `auto:language:${p.id}`,
          kind: "language",
          selector: p.dynamic_spec?.replace(/^language:/, "") ?? "",
          label: getPlaylistDisplayName(p),
          playlistId: p.id,
          updated: p.updated,
          trackCount: p.track_count,
        });
      }
    }
    for (const p of countryAutoPlaylists) {
      if (p.track_count > 0) {
        defs.push({
          id: `auto:country:${p.id}`,
          kind: "country",
          selector: p.dynamic_spec?.replace(/^artistcountry:/, "") ?? "",
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
  const initialAutoSort = loadPlaylistSortPref("auto");
  let autoSortField = $state<PlaylistSortField>(initialAutoSort.field);
  let autoSortAsc = $state(initialAutoSort.asc);

  // Moment Mix and Favourites/Recently Added are always pinned first, ahead of the sort
  // order applied to decade, genre & artist tag auto-playlists. BPM auto-playlists always
  // sort last, in their fixed intensity order (Down-Tempo → Extreme, set by
  // BPM_BUCKET_ORDER in utils/playlistOrder) — never interleaved into the name/track_count/
  // updated sort applied to genre/decade/artist tag, which would otherwise scramble them
  // (e.g. "High Energy BPM" sorting alphabetically between two genres).
  let sortedAutoDefs = $derived.by(() => {
    const field = autoSortField;
    const asc = autoSortAsc;
    const pinned = autoDefs.filter(
      (d) =>
        d.kind !== "genre" &&
        d.kind !== "decade" &&
        d.kind !== "artist_tag" &&
        d.kind !== "bpm" &&
        d.kind !== "language" &&
        d.kind !== "country",
    );
    const rest = autoDefs.filter(
      (d) =>
        d.kind === "genre" ||
        d.kind === "decade" ||
        d.kind === "artist_tag" ||
        d.kind === "language" ||
        d.kind === "country",
    );
    const sortedRest = sortByPlaylistField(rest, { field, asc }, (d) => ({
      label: d.label,
      trackCount: d.trackCount,
      updated: d.updated ?? 0,
    }));
    const bpmBlock = autoDefs.filter((d) => d.kind === "bpm");
    return [...pinned, ...sortedRest, ...bpmBlock];
  });

  // ---- Custom grid sort ----
  const initialCustomSort = loadPlaylistSortPref("custom");
  let customSortField = $state<PlaylistSortField>(initialCustomSort.field);
  let customSortAsc = $state(initialCustomSort.asc);

  $effect(() => {
    savePlaylistSortPref("auto", { field: autoSortField, asc: autoSortAsc });
    savePlaylistSortPref("custom", { field: customSortField, asc: customSortAsc });
  });

  let sortedPlaylists = $derived(
    sortByPlaylistField(
      customPlaylists.filter((p) => !p.is_queue),
      { field: customSortField, asc: customSortAsc },
      (p) => ({ label: p.name, trackCount: p.track_count, updated: p.updated }),
    ),
  );

  let sortedStationLists = $derived(
    sortByPlaylistField(
      playlistsStore.playlists.filter((p) => isStationList(p)),
      { field: customSortField, asc: customSortAsc },
      (p) => ({ label: p.name, trackCount: p.track_count, updated: p.updated }),
    ),
  );

  let activeViewMode = $derived(
    navigationStore.playlistsSubTab === "auto"
      ? prefs.playlistsAutoViewMode
      : prefs.playlistsCustomViewMode,
  );

  function setActiveViewMode(mode: CollectionViewMode) {
    if (navigationStore.playlistsSubTab === "auto") {
      prefs.setPlaylistsAutoViewMode(mode);
    } else {
      prefs.setPlaylistsCustomViewMode(mode);
    }
  }

  let contextMenuState = $state<
    | { x: number; y: number; kind: "playlist"; playlist: Playlist }
    | { x: number; y: number; kind: "auto"; def: AutoDef }
    | null
  >(null);

  function handlePlaylistContextMenu(e: MouseEvent, playlist: Playlist) {
    e.preventDefault();
    e.stopPropagation();
    contextMenuState = { x: e.clientX, y: e.clientY, kind: "playlist", playlist };
  }

  function handleAutoContextMenu(e: MouseEvent, def: AutoDef) {
    e.preventDefault();
    e.stopPropagation();
    contextMenuState = { x: e.clientX, y: e.clientY, kind: "auto", def };
  }

  function openAuto(def: AutoDef) {
    navigationStore.viewAutoPlaylist({
      kind: def.kind,
      ...(def.selector !== undefined ? { selector: def.selector } : {}),
      ...(def.playlistId !== undefined ? { playlistId: def.playlistId } : {}),
      ...(def.updated !== undefined ? { updated: def.updated } : {}),
    });
  }

  function openPlaylist(pl: Playlist) {
    playlistsStore.selectPlaylist(pl.id);
    navigationStore.viewPlaylist(pl.id);
  }

  async function handleCreateBlankPlaylist() {
    try {
      const playlist = await playlistsStore.createPlaylist(
        i18n.t("playlists.untitledPlaylistName"),
      );
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
        filters: [
          {
            name: "Playlists (*.m3u, *.m3u8, *.pls, *.xspf)",
            extensions: ["m3u", "m3u8", "pls", "xspf"],
          },
        ],
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
  <GenreAliasesDialogHost />
{:else}
  <div class="flex-1 flex flex-col overflow-hidden bg-brand-main text-brand-text-secondary h-full">
    <div
      class="flex-1 overflow-y-auto pb-dock"
      use:rememberScroll={`playlists:${navigationStore.playlistsSubTab}`}
    >
      <CollectionHeader
        title={navigationStore.playlistsSubTab === "auto"
          ? i18n.t("playlists.autoTitle")
          : i18n.t("playlists.customTitle")}
        subtitle={i18n.plural(
          "playlists.playlistCount",
          navigationStore.playlistsSubTab === "auto"
            ? sortedAutoDefs.length
            : sortedPlaylists.length,
        )}
      >
        {#snippet actions()}
          <button
            onclick={handleRefreshAll}
            disabled={isRefreshingAll}
            class="flex items-center justify-center w-9 h-9 rounded-full border border-brand-border bg-brand-sidebar text-brand-text-secondary hover:text-brand-accent-text hover:border-brand-accent/60 transition-colors disabled:opacity-50 disabled:cursor-not-allowed shadow-xs"
            title={i18n.t("playlists.refreshAllPlaylistsTooltip")}
            aria-label={i18n.t("playlists.refreshAllPlaylistsTooltip")}
          >
            <RefreshCw class="w-4 h-4 {isRefreshingAll ? 'animate-spin' : ''}" />
          </button>
          <ViewModeToggle mode={activeViewMode} onChange={setActiveViewMode} />
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
                <option value="name-true">▲ {i18n.t("playlists.sortLabelName")}</option>
                <option value="name-false">▼ {i18n.t("playlists.sortLabelName")}</option>
                <option value="track_count-true">▲ {i18n.t("playlists.sortLabelSongs")}</option>
                <option value="track_count-false">▼ {i18n.t("playlists.sortLabelSongs")}</option>
                <option value="updated-true">▲ {i18n.t("playlists.sortLabelUpdated")}</option>
                <option value="updated-false">▼ {i18n.t("playlists.sortLabelUpdated")}</option>
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
                <option value="name-true">▲ {i18n.t("playlists.sortLabelName")}</option>
                <option value="name-false">▼ {i18n.t("playlists.sortLabelName")}</option>
                <option value="track_count-true">▲ {i18n.t("playlists.sortLabelSongs")}</option>
                <option value="track_count-false">▼ {i18n.t("playlists.sortLabelSongs")}</option>
                <option value="updated-true">▲ {i18n.t("playlists.sortLabelUpdated")}</option>
                <option value="updated-false">▼ {i18n.t("playlists.sortLabelUpdated")}</option>
              </Select>
            {/if}
          </div>
        {/snippet}

        {#snippet toolbar()}
          {#if navigationStore.playlistsSubTab === "custom"}
            <Button
              onclick={handleCreateBlankPlaylist}
              variant="primary"
              title={i18n.t("playlists.newPlaylistBtn")}
            >
              <Plus class="w-4 h-4" />
              <span>{i18n.t("playlists.newPlaylistBtn")}</span>
            </Button>
            <Button
              onclick={() => collectionStore.openSmartBuilder()}
              variant="accent-soft"
              title={i18n.t("playlists.newSmartPlaylistBtn")}
            >
              <Sparkles class="w-4 h-4" />
              <span>{i18n.t("playlists.newSmartPlaylistBtn")}</span>
            </Button>
            <Button
              onclick={handleImportPlaylist}
              variant="secondary"
              title={i18n.t("playlists.importPlaylistTooltip")}
            >
              <FolderInput class="w-4 h-4 text-brand-accent-text" />
              <span>{i18n.t("playlists.importPlaylistBtn")}</span>
            </Button>
          {/if}
        {/snippet}
      </CollectionHeader>

      <div class="px-6 pb-8">
        {#if navigationStore.playlistsSubTab === "auto"}
          <div
            class="grid {activeViewMode === 'rows'
              ? 'grid-cols-[repeat(auto-fill,minmax(280px,1fr))] gap-2'
              : 'grid-cols-[repeat(auto-fill,minmax(180px,1fr))] gap-5'}"
          >
            {#each sortedAutoDefs as def (def.id)}
              {#if activeViewMode === "rows"}
                <AutoPlaylistRowCard
                  label={def.label}
                  kind={def.kind}
                  selector={def.selector}
                  playlistId={def.playlistId}
                  updated={def.updated}
                  trackCount={def.trackCount}
                  onClick={() => openAuto(def)}
                  oncontextmenu={(e) => handleAutoContextMenu(e, def)}
                />
              {:else}
                <AutoPlaylistCard
                  label={def.label}
                  kind={def.kind}
                  selector={def.selector}
                  playlistId={def.playlistId}
                  updated={def.updated}
                  trackCount={def.trackCount}
                  onClick={() => openAuto(def)}
                  oncontextmenu={(e) => handleAutoContextMenu(e, def)}
                />
              {/if}
            {/each}
          </div>
        {:else}
          {#snippet playlistGrid(items: typeof sortedPlaylists)}
            <div
              class="grid {activeViewMode === 'rows'
                ? 'grid-cols-[repeat(auto-fill,minmax(280px,1fr))] gap-2'
                : 'grid-cols-[repeat(auto-fill,minmax(180px,1fr))] gap-5'}"
            >
              {#each items as pl (pl.id)}
                {#if activeViewMode === "rows"}
                  <PlaylistRowCard
                    playlist={pl}
                    onClick={() => openPlaylist(pl)}
                    oncontextmenu={(e) => handlePlaylistContextMenu(e, pl)}
                  />
                {:else}
                  <PlaylistCard
                    playlist={pl}
                    widthClass="w-full"
                    onClick={() => openPlaylist(pl)}
                    oncontextmenu={(e) => handlePlaylistContextMenu(e, pl)}
                  />
                {/if}
              {/each}
            </div>
          {/snippet}

          {#if sortedPlaylists.length === 0 && sortedStationLists.length === 0}
            <div class="col-span-full py-16 text-center">
              <EmptyState
                card
                icon={ListMusic}
                title={i18n.t("playlists.noPlaylistsTitle")}
                subtitle={i18n.t("playlists.noPlaylistsText")}
                subtitleClass="text-xs text-brand-text-secondary font-medium"
              />
            </div>
          {:else}
            {#if sortedPlaylists.length > 0}
              {@render playlistGrid(sortedPlaylists)}
            {/if}

            {#if sortedStationLists.length > 0}
              <section class="mt-8 space-y-3" data-testid="playlists-station-lists-section">
                <h3 class="label-heading text-brand-text-secondary">
                  {i18n.t("radio.stationListsTitle")}
                </h3>
                {@render playlistGrid(sortedStationLists)}
              </section>
            {/if}
          {/if}
        {/if}
      </div>
    </div>
  </div>
{/if}

{#if contextMenuState?.kind === "playlist"}
  <PlaylistCardContextMenu
    x={contextMenuState.x}
    y={contextMenuState.y}
    playlist={contextMenuState.playlist}
    onClose={() => {
      contextMenuState = null;
    }}
  />
{:else if contextMenuState?.kind === "auto"}
  <AutoPlaylistContextMenu
    x={contextMenuState.x}
    y={contextMenuState.y}
    autoPlaylist={contextMenuState.def}
    label={contextMenuState.def.label}
    onClose={() => {
      contextMenuState = null;
    }}
  />
{/if}
