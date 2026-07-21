<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { onMount } from "svelte";
  import { open } from "@tauri-apps/plugin-dialog";
  import { collectionStore } from "../stores/collection.svelte";
  import { playlistsStore } from "../stores/playlists.svelte";
  import { playerStore } from "../stores/player.svelte";
  import { i18n } from "../stores/i18n.svelte";
  import type { Playlist } from "../types";
  import PlaylistCard from "./PlaylistCard.svelte";
  import AutoPlaylistCard from "./AutoPlaylistCard.svelte";
  import PlaylistView from "./PlaylistView.svelte";
  import AutoPlaylistDetailView from "./AutoPlaylistDetailView.svelte";
  import { FolderInput, Plus, ListMusic } from "lucide-svelte";

  interface AutoDef {
    id: string;
    kind: "favourites" | "recently_added" | "genre" | "decade";
    genre?: string;
    decade?: string;
    label: string;
    playlistId?: number;
    updated?: number;
    trackCount: number;
    autoPlay?: boolean;
  }

  onMount(async () => {
    try {
      // Auto-playlists (genre and decade) are materialized as real (dynamic_enabled) playlist
      // rows, refreshed at most once every 24h — sync then re-pull the list.
      await invoke("sync_genre_auto_playlists");
      await invoke("sync_decade_auto_playlists");
      await playlistsStore.refreshPlaylists();
    } catch (err) {
      console.error("Failed to sync auto-playlists:", err);
    }
    await playlistsStore.refreshAutoPlaylistCounts();
  });

  let genreAutoPlaylists = $derived(playlistsStore.playlists.filter((p) => p.dynamic_enabled && !p.dynamic_spec?.startsWith("decade:")));
  let decadeAutoPlaylists = $derived(playlistsStore.playlists.filter((p) => p.dynamic_enabled && p.dynamic_spec?.startsWith("decade:")));
  let customPlaylists = $derived(playlistsStore.playlists.filter((p) => !p.dynamic_enabled));

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
    for (const p of decadeAutoPlaylists) {
      if (p.track_count > 0) {
        const dec = p.dynamic_spec?.replace(/^decade:/, "") ?? p.name;
        defs.push({
          id: `auto:decade:${p.id}`,
          kind: "decade",
          decade: dec,
          label: dec,
          playlistId: p.id,
          updated: p.updated,
          trackCount: p.track_count,
          autoPlay: p.auto_play ?? false,
        });
      }
    }
    for (const p of genreAutoPlaylists) {
      if (p.track_count > 0) {
        defs.push({
          id: `auto:genre:${p.id}`,
          kind: "genre",
          genre: p.dynamic_spec?.replace(/^genre:/, "") ?? p.name,
          label: p.dynamic_spec?.replace(/^genre:/, "") ?? p.name,
          playlistId: p.id,
          updated: p.updated,
          trackCount: p.track_count,
          autoPlay: p.auto_play ?? false,
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
  // order applied to decade & genre auto-playlists.
  let sortedAutoDefs = $derived.by(() => {
    const field = autoSortField;
    const asc = autoSortAsc;
    const pinned = autoDefs.filter((d) => d.kind !== "genre" && d.kind !== "decade");
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
    return [...pinned, ...rest];
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
    return [...customPlaylists].sort((a, b) => {
      if (field === "name") {
        return asc ? a.name.localeCompare(b.name) : b.name.localeCompare(a.name);
      }
      const valA = field === "track_count" ? a.track_count : a.updated;
      const valB = field === "track_count" ? b.track_count : b.updated;
      return asc ? valA - valB : valB - valA;
    });
  });

  function openAuto(def: AutoDef) {
    collectionStore.viewAutoPlaylist(
      def.kind === "genre"
        ? { kind: "genre", genre: def.genre, playlistId: def.playlistId, updated: def.updated }
        : def.kind === "decade"
          ? { kind: "decade", decade: def.decade, playlistId: def.playlistId, updated: def.updated }
          : { kind: def.kind }
    );
  }

  function openPlaylist(pl: Playlist) {
    playlistsStore.selectPlaylist(pl.id);
    collectionStore.viewPlaylist(pl.id);
  }

  let newPlaylistName = $state("");
  let showCreateForm = $state(false);

  async function handleCreatePlaylist(e: Event) {
    e.preventDefault();
    const name = newPlaylistName.trim();
    if (name === "") return;
    await playlistsStore.createPlaylist(name);
    newPlaylistName = "";
    showCreateForm = false;
    if (playlistsStore.activePlaylistId !== null) {
      collectionStore.viewPlaylist(playlistsStore.activePlaylistId);
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
    <div class="flex-1 px-6 overflow-y-auto" class:pb-24={!!playerStore.currentSong}>
      <!-- Top bar with Filter Info / Sort controls (sticky) -->
      <div class="h-12 flex items-center justify-between sticky top-0 z-20 bg-brand-main">
        <!-- Showing Count (Left) -->
        <div class="text-xs text-brand-text-secondary font-medium">
          {#if collectionStore.playlistsSubTab === "auto"}
            {sortedAutoDefs.length === 1 ? i18n.t('playlists.showingOnePlaylist') : i18n.t('playlists.showingPlaylists', { count: sortedAutoDefs.length })}
          {:else}
            {sortedPlaylists.length === 1 ? i18n.t('playlists.showingOnePlaylist') : i18n.t('playlists.showingPlaylists', { count: sortedPlaylists.length })}
          {/if}
        </div>

        <!-- Actions + Sort Dropdown (Right) -->
        <div class="flex items-center gap-2">
          {#if collectionStore.playlistsSubTab === "custom"}
            <button
              onclick={handleImportPlaylist}
              class="flex items-center gap-1.5 bg-brand-sidebar hover:bg-brand-main border border-brand-border/60 text-brand-text-primary px-2.5 py-1.5 text-xs font-semibold rounded-lg transition-colors cursor-pointer"
              title={i18n.t('playlists.importPlaylistTooltip')}
            >
              <FolderInput class="w-3.5 h-3.5 text-brand-accent-text" />
              <span>{i18n.t('playlists.importPlaylistBtn')}</span>
            </button>
            <button
              onclick={() => { showCreateForm = !showCreateForm; }}
              class="flex items-center gap-1.5 bg-brand-accent hover:bg-brand-accent-hover text-brand-accent-contrast px-2.5 py-1.5 text-xs font-semibold rounded-lg transition-colors cursor-pointer"
              title={i18n.t('playlists.newPlaylistBtn')}
            >
              <Plus class="w-3.5 h-3.5" />
              <span>{i18n.t('playlists.newPlaylistBtn')}</span>
            </button>
          {/if}

          <div class="relative">
            {#if collectionStore.playlistsSubTab === "auto"}
              <select
                value={`${autoSortField}-${autoSortAsc}`}
                onchange={(e) => {
                  const [field, asc] = e.currentTarget.value.split("-");
                  autoSortField = field as "name" | "track_count" | "updated";
                  autoSortAsc = asc === "true";
                }}
                class="bg-brand-sidebar hover:bg-brand-main border border-brand-border text-brand-text-secondary hover:text-brand-text-primary text-xs rounded-lg pl-2.5 pr-8 py-1.5 focus:outline-none focus:border-brand-accent transition-all cursor-pointer font-medium appearance-none -webkit-appearance-none"
                style="background-image: url(&quot;data:image/svg+xml;charset=utf-8,%3Csvg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 20 20' fill='none'%3E%3Cpath stroke='%239ca3af' stroke-linecap='round' stroke-linejoin='round' stroke-width='1.5' d='m6 8 4 4 4-4'/%3E%3C/svg%3E&quot;); background-position: right 0.625rem center; background-repeat: no-repeat; background-size: 1.25em;"
              >
                <option value="name-true">{i18n.t('playlists.sortNameAsc')}</option>
                <option value="name-false">{i18n.t('playlists.sortNameDesc')}</option>
                <option value="track_count-false">{i18n.t('playlists.sortTrackCountDesc')}</option>
                <option value="track_count-true">{i18n.t('playlists.sortTrackCountAsc')}</option>
                <option value="updated-false">{i18n.t('playlists.sortUpdatedNewest')}</option>
                <option value="updated-true">{i18n.t('playlists.sortUpdatedOldest')}</option>
              </select>
            {:else}
              <select
                value={`${customSortField}-${customSortAsc}`}
                onchange={(e) => {
                  const [field, asc] = e.currentTarget.value.split("-");
                  customSortField = field as "name" | "track_count" | "updated";
                  customSortAsc = asc === "true";
                }}
                class="bg-brand-sidebar hover:bg-brand-main border border-brand-border text-brand-text-secondary hover:text-brand-text-primary text-xs rounded-lg pl-2.5 pr-8 py-1.5 focus:outline-none focus:border-brand-accent transition-all cursor-pointer font-medium appearance-none -webkit-appearance-none"
                style="background-image: url(&quot;data:image/svg+xml;charset=utf-8,%3Csvg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 20 20' fill='none'%3E%3Cpath stroke='%239ca3af' stroke-linecap='round' stroke-linejoin='round' stroke-width='1.5' d='m6 8 4 4 4-4'/%3E%3C/svg%3E&quot;); background-position: right 0.625rem center; background-repeat: no-repeat; background-size: 1.25em;"
              >
                <option value="name-true">{i18n.t('playlists.sortNameAsc')}</option>
                <option value="name-false">{i18n.t('playlists.sortNameDesc')}</option>
                <option value="track_count-false">{i18n.t('playlists.sortTrackCountDesc')}</option>
                <option value="track_count-true">{i18n.t('playlists.sortTrackCountAsc')}</option>
                <option value="updated-false">{i18n.t('playlists.sortUpdatedNewest')}</option>
                <option value="updated-true">{i18n.t('playlists.sortUpdatedOldest')}</option>
              </select>
            {/if}
          </div>
        </div>
      </div>

      {#if collectionStore.playlistsSubTab === "custom" && showCreateForm}
        <form onsubmit={handleCreatePlaylist} class="flex items-center gap-2 mb-4">
          <input
            bind:value={newPlaylistName}
            placeholder={i18n.t('playlists.createPlaylistPlaceholder')}
            class="bg-brand-sidebar border border-brand-border rounded-lg px-3 py-1.5 text-xs w-64 text-brand-text-primary focus:outline-none focus:border-brand-accent"
          />
          <button type="submit" class="bg-brand-accent hover:bg-brand-accent-hover text-brand-accent-contrast rounded-lg px-3 py-1.5 text-xs font-semibold cursor-pointer">
            {i18n.t('sidebar.create')}
          </button>
        </form>
      {/if}

      <div class="pt-2 pb-8">
        {#if collectionStore.playlistsSubTab === "auto"}
          <div class="grid grid-cols-[repeat(auto-fill,minmax(180px,1fr))] gap-6">
            {#each sortedAutoDefs as def (def.id)}
              <AutoPlaylistCard
                label={def.label}
                kind={def.kind}
                genre={def.genre}
                decade={def.decade}
                playlistId={def.playlistId}
                updated={def.updated}
                trackCount={def.trackCount}
                autoPlay={def.autoPlay}
                onClick={() => openAuto(def)}
              />
            {/each}
          </div>
        {:else if sortedPlaylists.length === 0}
          <div class="col-span-full py-16 text-center">
            <div class="flex flex-col items-center justify-center max-w-sm mx-auto p-6 bg-brand-sidebar/20 rounded-xl border border-dashed border-brand-border/60 select-none">
              <ListMusic class="w-12 h-12 text-brand-accent-text/40 mb-3 animate-pulse" />
              <h3 class="text-base font-semibold text-brand-text-primary mb-1">{i18n.t('playlists.noPlaylistsTitle')}</h3>
              <p class="text-xs text-brand-text-secondary/60 font-medium">{i18n.t('playlists.noPlaylistsText')}</p>
            </div>
          </div>
        {:else}
          <div class="grid grid-cols-[repeat(auto-fill,minmax(180px,1fr))] gap-6">
            {#each sortedPlaylists as pl (pl.id)}
              <PlaylistCard playlist={pl} onClick={() => openPlaylist(pl)} />
            {/each}
          </div>
        {/if}
      </div>
    </div>
  </div>
{/if}
