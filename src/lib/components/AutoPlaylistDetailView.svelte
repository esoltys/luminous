<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { listen } from "@tauri-apps/api/event";
  import { applySongStats, type SongStatsPayload } from "../utils/stats";
  import { collectionStore, type AutoPlaylistRef } from "../stores/collection.svelte";
  import { playerStore } from "../stores/player.svelte";
  import { playlistsStore } from "../stores/playlists.svelte";
  import { songsToCoverStack } from "../utils/covers";
  import CoverStack from "./CoverStack.svelte";
  import SongRating from "./SongRating.svelte";
  import TagEditor from "./TagEditor.svelte";
  import SongContextMenu from "./SongContextMenu.svelte";
  import PopulationModeTabs from "./PopulationModeTabs.svelte";
  import Toggle from "./Toggle.svelte";
  import SortableHeader from "./SortableHeader.svelte";
  import SongSelectionToolbar from "./SongSelectionToolbar.svelte";
  import EmptyState from "./EmptyState.svelte";
  import PlayShuffleButtons from "./PlayShuffleButtons.svelte";
  import NowPlayingBars from "./NowPlayingBars.svelte";
  import IconActionButton from "./IconActionButton.svelte";
  import LinkButton from "./LinkButton.svelte";
  import ColumnSelector from "./ColumnSelector.svelte";
  import Input from "./Input.svelte";
  import ContextMenu from "./ContextMenu.svelte";
  import ContextMenuItem from "./ContextMenuItem.svelte";
  import ContextMenuDivider from "./ContextMenuDivider.svelte";
  import { Clock, Play, Plus, FolderPlus, Edit3, Music, RefreshCw, CheckCircle2, Heart, Calendar, Hourglass, Search, RotateCcw, RotateCw, MoreHorizontal, X, Trash2, Eraser } from "lucide-svelte";
import { shuffleArray } from "../utils/shuffle";
  import { formatDate, formatFileSize, formatSampleRate, formatBitDepth, formatChannels } from "../utils/formatters";
  import type { PlaylistItem, QueuePopulationMode, Song } from "../types";
  import { i18n } from "../stores/i18n.svelte";
  import { toastStore } from "../stores/toast.svelte";
  import { getPopulationModeSuffix } from "../utils/playlist";
  import { rememberScroll } from "../utils/scrollMemory";
  import Modal from "./Modal.svelte";
  import Button from "./Button.svelte";

  let { view }: { view: AutoPlaylistRef } = $props();

  let kind = $derived(view.kind);

  let gridColsStyle = $derived.by(() => {
    const cols: string[] = ["56px"]; // play indicator + position number, always present
    const vc = collectionStore.visibleColumns;

    if (vc.title) cols.push("2fr");
    if (vc.artist) cols.push("1.5fr");
    if (vc.album) cols.push("1.5fr");
    if (vc.composer) cols.push("1.5fr");
    if (vc.album_artist) cols.push("1.5fr");
    if (vc.format) cols.push("64px");
    if (vc.year) cols.push("60px");
    if (vc.genre) cols.push("1.2fr");
    if (vc.grouping) cols.push("1.2fr");
    if (vc.bpm) cols.push("60px");
    if (vc.initial_key) cols.push("60px");
    if (vc.bitrate) cols.push("70px");
    if (vc.samplerate) cols.push("75px");
    if (vc.bitdepth) cols.push("65px");
    if (vc.channels) cols.push("70px");
    if (vc.filesize) cols.push("75px");
    if (vc.rating) cols.push("96px");
    if (vc.playcount) cols.push("70px");
    if (vc.skipcount) cols.push("70px");
    if (vc.lastplayed) cols.push("90px");
    if (vc.added) cols.push("90px");
    if (vc.duration) cols.push("80px");
    if (vc.path) cols.push("2fr");
    if (vc.actions) cols.push("80px");

    return `grid-template-columns: ${cols.join(" ")}`;
  });
  let genre = $derived(view.genre);
  let decade = $derived(view.decade);
  let playlistId = $derived(view.playlistId);
  let updated = $derived(view.updated);

  let songs = $state<Song[]>([]);
  let loading = $state(true);
  let editingSongId = $state<number | null>(null);
  let contextMenuState = $state<{ x: number; y: number; song: Song } | null>(null);

  // Save auto-playlist as custom playlist modal state
  let showSaveModal = $state(false);
  let savePlaylistName = $state("");

  let selectedSongIds = $state<Set<number>>(new Set());
  let lastSelectedSongId = $state<number | null>(null);

  function handleContextMenu(event: MouseEvent, song: Song) {
    event.preventDefault();
    if (!selectedSongIds.has(song.id)) {
      selectedSongIds = new Set([song.id]);
      lastSelectedSongId = song.id;
    }
    contextMenuState = { x: event.clientX, y: event.clientY, song };
  }

  function handleSongClick(e: MouseEvent, song: Song) {
    if (e.shiftKey && lastSelectedSongId !== null) {
      const idx1 = songs.findIndex((s) => s.id === lastSelectedSongId);
      const idx2 = songs.findIndex((s) => s.id === song.id);
      if (idx1 !== -1 && idx2 !== -1) {
        const start = Math.min(idx1, idx2);
        const end = Math.max(idx1, idx2);
        const newSet = new Set(e.ctrlKey || e.metaKey ? selectedSongIds : []);
        for (let i = start; i <= end; i++) {
          newSet.add(songs[i].id);
        }
        selectedSongIds = newSet;
      }
    } else if (e.ctrlKey || e.metaKey) {
      const newSet = new Set(selectedSongIds);
      if (newSet.has(song.id)) {
        newSet.delete(song.id);
      } else {
        newSet.add(song.id);
      }
      selectedSongIds = newSet;
      lastSelectedSongId = song.id;
    } else if (selectedSongIds.size === 1 && selectedSongIds.has(song.id)) {
      selectedSongIds = new Set();
      lastSelectedSongId = null;
    } else {
      selectedSongIds = new Set([song.id]);
      lastSelectedSongId = song.id;
    }
  }

  function handleKeydown(e: KeyboardEvent) {
    if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "a") {
      const target = e.target as HTMLElement;
      if (target && (target.tagName === "INPUT" || target.tagName === "TEXTAREA")) return;
      e.preventDefault();
      selectedSongIds = new Set(songs.map((s) => s.id));
    } else if (e.key === "Escape") {
      selectedSongIds = new Set();
    }
  }

  function handleWindowMouseDown(e: MouseEvent) {
    if (selectedSongIds.size === 0) return;
    const target = e.target as HTMLElement;
    if (!target) return;
    if (
      target.closest("[data-song-row]") ||
      target.closest("[role='menu']") ||
      target.closest("[data-floating-toolbar]") ||
      target.closest("button") ||
      target.closest("input")
    ) {
      return;
    }
    selectedSongIds = new Set();
  }

  async function handleBulkAddToPlaylist() {
    if (selectedSongIds.size === 0) return;
    if (playlistsStore.activeCustomPlaylist) {
      await playlistsStore.addSongsToPlaylist(playlistsStore.activeCustomPlaylist.id, Array.from(selectedSongIds));
    } else {
      toastStore.show(i18n.t("collection.selectPlaylistFirstAlert"));
    }
  }

  function handlePlaySelected() {
    if (selectedSongIds.size === 0) return;
    const selectedList = songs.filter((s) => selectedSongIds.has(s.id));
    if (selectedList.length > 0) {
      playerStore.playSongs(selectedList.map((s) => s.id), 0, playlistId, undefined, displayName);
    }
  }

  let displayName = $derived.by(() => {
    if (kind === "favourites") return i18n.t("playlists.autoFavourites");
    if (kind === "recently_added") return i18n.t("playlists.autoRecentlyAdded");
    if (kind === "history") return i18n.t("playlists.autoHistory");
    const base = kind === "decade"
      ? (decade || i18n.t("artistDetail.unknownYear"))
      : (genre || i18n.t("artistDetail.unknownGenre"));
    const suffix = getPopulationModeSuffix(populationMode);
    return suffix ? `${base} ${suffix}` : base;
  });

  let topCovers = $derived((kind === "genre" || kind === "decade") ? songsToCoverStack(songs) : []);

  let updatedLabel = $derived.by(() => {
    if ((kind !== "genre" && kind !== "decade") || updated === undefined) return null;
    return new Date(updated * 1000).toLocaleDateString();
  });

  let totalDurationLabel = $derived.by(() => {
    const totalNs = songs.reduce((sum, s) => sum + (s.length_nanosec ?? 0), 0);
    const totalMinutes = Math.round(totalNs / 1_000_000_000 / 60);
    const h = Math.floor(totalMinutes / 60);
    const m = totalMinutes % 60;
    return h > 0 ? `${h}h ${m}m` : `${m}m`;
  });

  async function fetchSongs(k: typeof kind, g: typeof genre, d: typeof decade, pid: typeof playlistId): Promise<Song[]> {
    if ((k === "genre" || k === "decade") && pid !== undefined) {
      const items = await invoke<PlaylistItem[]>("get_playlist_tracks", { playlistId: pid });
      return items.filter((item) => !!item.song).map((item) => item.song as Song);
    }
    if (k === "favourites") return invoke<Song[]>("get_favourite_songs");
    if (k === "recently_added") return invoke<Song[]>("get_recently_added_songs", { limit: 50 });
    if (k === "history") return invoke<Song[]>("get_recently_played_songs", { limit: 100 });
    if (k === "decade") return invoke<Song[]>("get_songs_by_decade", { decade: d ?? "", limit: 50 });
    return invoke<Song[]>("get_songs_by_genre", { genre: g ?? "", limit: 50 });
  }

  // Track backing playlist track_count reactively so refills trigger an instant re-fetch in the UI
  let backingTrackCount = $derived.by(() => {
    if (playlistId === undefined) return 0;
    const pl = playlistsStore.playlists.find((p) => p.id === playlistId);
    return pl?.track_count ?? 0;
  });

  $effect(() => {
    const k = kind;
    const g = genre;
    const d = decade;
    const pid = playlistId;
    const count = backingTrackCount;
    loading = true;
    fetchSongs(k, g, d, pid)
      .then((fetchedSongs) => {
        if (kind !== k || genre !== g || decade !== d || playlistId !== pid) return;
        songs = fetchedSongs;
      })
      .catch((err) => {
        console.error("Failed to load auto-playlist detail:", err);
      })
      .finally(() => {
        if (kind === k && genre === g && decade === d && playlistId === pid) loading = false;
      });
  });

  async function handlePlaySong(song: Song) {
    const index = songs.findIndex((s) => s.id === song.id);
    const songIds = songs.map((s) => s.id);
    const queuePl = await playlistsStore.ensureQueuePlaylist();
    await playerStore.playSongs(songIds, index >= 0 ? index : 0, queuePl?.id, undefined, "Queue");
    if (queuePl) {
      playlistsStore.selectPlaylist(queuePl.id);
      collectionStore.viewPlaylist(queuePl.id);
    }
  }

  async function handlePlayAll() {
    if (songs.length === 0) return;
    const queuePl = await playlistsStore.ensureQueuePlaylist();
    await playerStore.setShuffleMode("off");
    await playerStore.playSongs(songs.map((s) => s.id), 0, queuePl?.id, undefined, "Queue");
    if (queuePl) {
      playlistsStore.selectPlaylist(queuePl.id);
      collectionStore.viewPlaylist(queuePl.id);
    }
  }

  async function handleShufflePlay() {
    if (songs.length === 0) return;
    const queuePl = await playlistsStore.ensureQueuePlaylist();
    const shuffledIds = shuffleArray(songs.map((s) => s.id));
    await playerStore.setShuffleMode("all");
    await playerStore.playSongs(shuffledIds, 0, queuePl?.id, undefined, "Queue");
    if (queuePl) {
      playlistsStore.selectPlaylist(queuePl.id);
      collectionStore.viewPlaylist(queuePl.id);
    }
  }

  async function handleAddSongToPlaylist(songId: number) {
    if (playlistsStore.activeCustomPlaylist) {
      await playlistsStore.addSongsToPlaylist(playlistsStore.activeCustomPlaylist.id, [songId]);
    } else {
      toastStore.show(i18n.t("collection.selectPlaylistFirstAlert"));
    }
  }

  async function handleAddAllToPlaylist() {
    if (songs.length === 0) return;
    if (playlistsStore.activeCustomPlaylist) {
      await playlistsStore.addSongsToPlaylist(playlistsStore.activeCustomPlaylist.id, songs.map((s) => s.id));
    } else {
      toastStore.show(i18n.t("collection.selectPlaylistFirstAlert"));
    }
  }

  function handleSaveAsCustomPlaylist() {
    if (songs.length === 0) return;
    savePlaylistName = displayName;
    showSaveModal = true;
  }

  async function confirmSaveAsCustomPlaylist() {
    if (!savePlaylistName || !savePlaylistName.trim()) return;
    try {
      const created = await playlistsStore.createPlaylist(savePlaylistName.trim());
      await playlistsStore.addSongsToPlaylist(created.id, songs.map((s) => s.id));
      playlistsStore.selectPlaylist(created.id);
      collectionStore.viewPlaylist(created.id);
      showSaveModal = false;
    } catch (err) {
      console.error("Failed to save auto-playlist as custom playlist:", err);
    }
  }

  /**
   * Derive current auto_play state from the backing playlist row (genre/decade only).
   * For virtual playlists (favourites, recently_added) there is no row — so we
   * always show the toggle but store the preference in app_settings.
   */
  let autoPlay = $derived.by(() => {
    if (playlistId === undefined) return true;
    const pl = playlistsStore.playlists.find((p) => p.id === playlistId);
    return pl?.auto_play ?? true;
  });

  async function handleToggleAutoPlay() {
    if (playlistId === undefined) return;
    await playlistsStore.setPlaylistAutoPlay(playlistId, !autoPlay);
  }

  /** Derive the current queue population mode from the backing playlist row (#120). */
  let populationMode = $derived.by((): QueuePopulationMode => {
    if (playlistId === undefined) return "all";
    const pl = playlistsStore.playlists.find((p) => p.id === playlistId);
    return pl?.population_mode ?? "all";
  });

  let emptyStateMessage = $derived.by(() => {
    switch (populationMode) {
      case "favourites":
        return i18n.t("playlists.populationModeEmptyFavourites");
      case "familiar":
        return i18n.t("playlists.populationModeEmptyFamiliar");
      case "discover":
        return i18n.t("playlists.populationModeEmptyDiscover");
      case "deep_cuts":
        return i18n.t("playlists.populationModeEmptyDeepCuts");
      default:
        return i18n.t("playlists.populationModeEmptyAll");
    }
  });

  let isChangingMode = $state(false);

  async function handleChangePopulationMode(newMode: QueuePopulationMode) {
    if (playlistId === undefined || newMode === populationMode || isChangingMode) return;
    isChangingMode = true;
    playerStore.clearExhausted(playlistId);
    try {
      await playlistsStore.setPlaylistPopulationMode(playlistId, newMode);
      songs = await fetchSongs(kind, genre, decade, playlistId);
    } catch (err) {
      console.error("Failed to change queue population mode:", err);
    } finally {
      isChangingMode = false;
    }
  }

  let isRefreshing = $state(false);

  async function handleRefreshAutoPlaylist() {
    if (playlistId === undefined || isRefreshing) return;
    isRefreshing = true;
    playerStore.clearExhausted(playlistId);
    try {
      await playlistsStore.refreshAutoPlaylist(playlistId);
      songs = await fetchSongs(kind, genre, decade, playlistId);
    } catch (err) {
      console.error("Failed to refresh auto-playlist:", err);
    } finally {
      isRefreshing = false;
    }
  }

  function openTagEditor(songId: number) {
    editingSongId = songId;
  }

  function handleTagEditorSaved() {
    collectionStore.refreshLibrary();
    loading = true;
    fetchSongs(kind, genre, decade, playlistId)
      .then((fetchedSongs) => {
        songs = fetchedSongs;
      })
      .catch((err) => console.error(err))
      .finally(() => (loading = false));
  }

  function formatDuration(ns: number | undefined): string {
    if (!ns) return "0:00";
    const sec = Math.floor(ns / 1_000_000_000);
    const m = Math.floor(sec / 60);
    const s = sec % 60;
    return `${m}:${s < 10 ? "0" : ""}${s}`;
  }

  async function rateSong(song: Song, rating: number) {
    song.rating = await invoke<number>("set_song_rating", { songId: song.id, rating });
  }

  // Sync rating/playcount changes from other views and scrobble bumps into
  // this view's locally fetched song list.
  $effect(() => {
    let unlisten: (() => void) | undefined;
    let disposed = false;
    listen<SongStatsPayload>("song-stats-changed", async (event) => {
      const song = songs.find((s) => s.id === event.payload.song_id);
      if (song) {
        applySongStats(song, event.payload);
      }
      if (kind === "history" || kind === "favourites" || kind === "recently_added") {
        try {
          songs = await fetchSongs(kind, genre, decade, playlistId);
        } catch (err) {
          console.error("Failed to re-fetch songs on stats change:", err);
        }
      }
    }).then((fn) => {
      if (disposed) fn();
      else unlisten = fn;
    });
    return () => {
      disposed = true;
      unlisten?.();
    };
  });

  type AutoPlaylistSortField = "default" | keyof Song;
  let sortField = $state<AutoPlaylistSortField>("default");
  let sortAsc = $state(true);

  function toggleSort(field: AutoPlaylistSortField) {
    if (sortField === field) {
      sortAsc = !sortAsc;
    } else {
      sortField = field;
      sortAsc = true;
    }
  }

  let filterQuery = $state("");
  let overflowMenuPos = $state<{ x: number; y: number } | null>(null);

  function toggleOverflowMenu(event: MouseEvent) {
    const btn = event.currentTarget as HTMLElement;
    const rect = btn.getBoundingClientRect();
    if (overflowMenuPos) {
      overflowMenuPos = null;
    } else {
      overflowMenuPos = { x: rect.left, y: rect.bottom + 4 };
    }
  }

  async function handleClearHistory() {
    overflowMenuPos = null;
    try {
      await invoke("clear_play_history");
      songs = [];
      toastStore.show(i18n.t("playlists.historyCleared", {}, "Play history cleared"));
    } catch (err) {
      console.error("Failed to clear play history:", err);
    }
  }

  let filteredSongs = $derived.by(() => {
    if (!filterQuery.trim()) return songs;
    const q = filterQuery.toLowerCase().trim();
    return songs.filter((s) => {
      const titleMatch = s.title?.toLowerCase().includes(q);
      const artistMatch = s.artist?.toLowerCase().includes(q) || s.album_artist?.toLowerCase().includes(q);
      const albumMatch = s.album?.toLowerCase().includes(q);
      const genreMatch = s.genre?.toLowerCase().includes(q);
      return titleMatch || artistMatch || albumMatch || genreMatch;
    });
  });

  let sortedSongs = $derived.by(() => {
    const list = filteredSongs;
    if (sortField === "default") {
      return sortAsc ? list : [...list].reverse();
    }
    const field = sortField as keyof Song;
    return [...list].sort((a, b) => {
      let valA = a[field];
      let valB = b[field];

      if (valA === undefined || valA === null) return sortAsc ? 1 : -1;
      if (valB === undefined || valB === null) return sortAsc ? -1 : 1;

      if (typeof valA === "string" && typeof valB === "string") {
        const cmp = valA.localeCompare(valB);
        return sortAsc ? cmp : -cmp;
      } else {
        const cmp = (valA as number) - (valB as number);
        return sortAsc ? cmp : -cmp;
      }
    });
  });

</script>

<div
  class="flex-1 flex flex-col overflow-y-auto carousel-scroll bg-brand-main text-brand-text-secondary h-full"
  use:rememberScroll={`autoplaylist:${view.kind}:${view.genre ?? view.decade ?? ""}`}
>
  <!-- Auto-Playlist Hero & Summary Banner Header -->
  <div class="relative z-30 w-full border-b border-brand-border/60 bg-brand-main/60 backdrop-blur-md px-6 pt-6 pb-6 shrink-0">
    <div class="flex items-stretch justify-between gap-6 relative z-10">
      <!-- Left Title & Summary Metadata -->
      <div class="flex flex-col justify-end gap-2 min-w-0 flex-1">
        <h1 class="text-3xl sm:text-4xl font-heading font-bold text-brand-text-primary leading-snug truncate py-0.5" title={displayName}>
          {displayName}
        </h1>

        <div class="flex flex-wrap items-center gap-x-2 gap-y-1 text-xs text-brand-text-secondary mt-1 font-medium">
          <span>{songs.length === 1 ? i18n.t('playlists.oneSong') : i18n.t('playlists.songsCount', { count: songs.length })}</span>
          <span>•</span>
          <span>{totalDurationLabel}</span>
          {#if updatedLabel}
            <span>•</span>
            <span>{i18n.t('playlists.updatedOn', { date: updatedLabel })}</span>
          {/if}
        </div>

        <!-- Primary Control Buttons Row -->
        <div class="flex flex-wrap items-center gap-3 mt-3 select-none">
          <PlayShuffleButtons
            onPlayAll={handlePlayAll}
            onShufflePlay={handleShufflePlay}
            disabled={loading || songs.length === 0}
            class="shrink-0"
          />
          <ColumnSelector align="left" iconOnly />
        </div>

        <!-- Secondary Control Buttons Row: Search Filter Bar, Undo, Redo, More -->
        <div class="flex flex-wrap items-center gap-2.5 mt-2.5 select-none relative z-40">
          <!-- Search Filter Bar -->
          <div class="relative w-full max-w-xs">
            <Search class="w-3.5 h-3.5 absolute left-3.5 top-1/2 -translate-y-1/2 text-brand-text-secondary/60 pointer-events-none" />
            <Input
              type="text"
              bind:value={filterQuery}
              placeholder={i18n.t("playlists.filterPlaceholder")}
              size="md"
              pill
              class="w-full"
              style="padding-left: 2.25rem; padding-right: 2rem;"
            />
            {#if filterQuery}
              <button
                onclick={() => { filterQuery = ""; }}
                class="absolute right-3 top-1/2 -translate-y-1/2 text-brand-text-secondary/60 hover:text-brand-text-primary p-0.5 cursor-pointer"
                title={i18n.t("playlists.clearFilter")}
              >
                <X class="w-3.5 h-3.5" />
              </button>
            {/if}
          </div>

          <div class="flex items-center gap-2 shrink-0">
            <IconActionButton onclick={() => playlistsStore.undo()} title={i18n.t("playlists.undoTooltip")}>
              {#snippet icon()}<RotateCcw class="w-4 h-4" />{/snippet}
            </IconActionButton>
            <IconActionButton onclick={() => playlistsStore.redo()} title={i18n.t("playlists.redoTooltip")}>
              {#snippet icon()}<RotateCw class="w-4 h-4" />{/snippet}
            </IconActionButton>
            <button
              onclick={toggleOverflowMenu}
              title={i18n.t("playlists.moreActionsTooltip")}
              class="flex items-center justify-center w-10 h-10 rounded-full border border-brand-border text-brand-text-secondary hover:text-brand-accent-text hover:bg-brand-sidebar transition-colors cursor-pointer shadow-xs"
            >
              <MoreHorizontal class="w-4 h-4" />
            </button>
          </div>
        </div>

        <!-- Additional controls for genre/decade playlists: auto-play and population-mode -->
        {#if (kind === "genre" || kind === "decade") && playlistId !== undefined}
          <div class="flex flex-wrap items-center gap-2.5 mt-2.5 select-none relative z-40">
            <!-- Auto-Play toggle: keep appending next batch as playback approaches end (#26) -->
            <div
              id="auto-play-toggle-{playlistId}"
              title={autoPlay
                ? i18n.t('playlists.autoPlayTooltipOn')
                : i18n.t('playlists.autoPlayTooltipOff')}
              class="flex items-center gap-2 pl-2.5 pr-0.5 h-7 rounded-full border border-brand-border bg-brand-main/40 text-xs font-semibold whitespace-nowrap shrink-0 text-brand-text-primary"
            >
              <RefreshCw class="w-3.5 h-3.5 shrink-0 text-brand-text-secondary {autoPlay ? 'animate-spin [animation-duration:3s] text-brand-accent-text' : ''}" />
              <span class="whitespace-nowrap text-[11px]">{i18n.t('playlists.autoPlayLabel')}</span>
              <Toggle checked={autoPlay} onchange={handleToggleAutoPlay} label={i18n.t('playlists.autoPlayLabel')} showOnOffLabel={false} />
            </div>

            <!-- Queue population mode tabs (#120): what bias to (re)populate this auto-playlist with -->
            <PopulationModeTabs
              mode={populationMode}
              disabled={loading || isChangingMode}
              onChange={handleChangePopulationMode}
            />
          </div>
        {/if}
      </div>

      <!-- Right: Cover Stack -->
      <div class="relative w-40 h-40 hidden sm:block shrink-0">
        {#if (kind === "genre" || kind === "decade") && topCovers.length > 0}
          <div class="w-full h-full bg-brand-main bg-gradient-to-br {kind === 'decade' ? 'from-[#2563EB]/25 to-[#38BDF8]/15 border-[#38BDF8]/30 shadow-[0_0_28px_3px_rgba(56,189,248,0.4)]' : 'from-[#059669]/25 to-[#34D399]/15 border-[#34D399]/30 shadow-[0_0_28px_3px_rgba(52,211,153,0.4)]'} flex items-center justify-center overflow-hidden border relative">
            <CoverStack covers={topCovers} sizeClass="w-[82%] h-[82%]" />
          </div>
        {:else if kind === "favourites"}
          <div class="w-full h-full bg-brand-main bg-gradient-to-br from-[#DB2777]/25 to-[#F43F5E]/15 flex items-center justify-center overflow-hidden border border-[#F43F5E]/30 shadow-[0_0_28px_3px_rgba(244,63,94,0.4)]">
            <Heart class="w-16 h-16 text-[#F43F5E] fill-current" />
          </div>
        {:else if kind === "recently_added"}
          <div class="w-full h-full bg-brand-main bg-gradient-to-br from-[#CA8A04]/25 to-[#FACC15]/15 flex items-center justify-center overflow-hidden border border-[#FACC15]/30 shadow-[0_0_28px_3px_rgba(250,204,21,0.4)]">
            <Clock class="w-16 h-16 text-[#CA8A04]" />
          </div>
        {:else if kind === "history"}
          <div class="w-full h-full bg-brand-main bg-gradient-to-br from-[#8B5CF6]/25 to-[#A78BFA]/15 flex items-center justify-center overflow-hidden border border-[#A78BFA]/30 shadow-[0_0_28px_3px_rgba(167,139,250,0.4)]">
            <Hourglass class="w-16 h-16 text-[#8B5CF6]" />
          </div>
        {:else if kind === "decade"}
          <div class="w-full h-full bg-brand-main bg-gradient-to-br from-[#2563EB]/25 to-[#38BDF8]/15 flex items-center justify-center overflow-hidden border border-[#38BDF8]/30 shadow-[0_0_28px_3px_rgba(56,189,248,0.4)]">
            <Calendar class="w-16 h-16 text-[#38BDF8]" />
          </div>
        {:else}
          <div class="w-full h-full bg-brand-main bg-gradient-to-br from-[#059669]/25 to-[#34D399]/15 flex items-center justify-center overflow-hidden border border-[#34D399]/30 shadow-[0_0_28px_3px_rgba(52,211,153,0.4)]">
            <Music class="w-16 h-16 text-[#34D399]" />
          </div>
        {/if}
      </div>
    </div>
  </div>

  <div class="px-6 py-6 flex flex-col" class:pb-28={!!playerStore.currentSong}>
    <div class="border border-brand-border/60 rounded-xl bg-brand-sidebar/30 backdrop-blur-md relative overflow-hidden">
      <!-- Header -->
      <div class="sticky top-0 z-20 flex flex-col bg-brand-sidebar border-b border-brand-border text-xs text-brand-text-primary uppercase tracking-wider font-semibold select-none">
        <div role="row" class="grid items-center py-3 px-4" style={gridColsStyle}>
          <SortableHeader
            active={sortField === "track" || sortField === "default"}
            {sortAsc}
            onclick={() => toggleSort("track")}
            class="text-center hover:text-brand-text-primary transition-colors flex items-center justify-center gap-1 cursor-pointer font-semibold uppercase tracking-wider min-w-0"
          >
            {#snippet label(arrow)}<span class="truncate max-w-[calc(100%-1rem)]">{i18n.t('playlists.tableHeaderTrack')} {arrow}</span>{/snippet}
          </SortableHeader>
          {#if collectionStore.visibleColumns.title}
            <SortableHeader
              active={sortField === "title"}
              {sortAsc}
              onclick={() => toggleSort("title")}
              class="text-left hover:text-brand-text-primary transition-colors flex items-center gap-1 cursor-pointer font-semibold uppercase tracking-wider min-w-0"
            >
              {#snippet label(arrow)}<span class="truncate max-w-[calc(100%-1rem)]">{i18n.t('playlists.tableHeaderTitle')} {arrow}</span>{/snippet}
            </SortableHeader>
          {/if}
          {#if collectionStore.visibleColumns.artist}
            <SortableHeader
              active={sortField === "artist"}
              {sortAsc}
              onclick={() => toggleSort("artist")}
              class="text-left hover:text-brand-text-primary transition-colors flex items-center gap-1 cursor-pointer font-semibold uppercase tracking-wider min-w-0"
            >
              {#snippet label(arrow)}<span class="truncate max-w-[calc(100%-1rem)]">{i18n.t('playlists.tableHeaderArtist')} {arrow}</span>{/snippet}
            </SortableHeader>
          {/if}
          {#if collectionStore.visibleColumns.album}
            <SortableHeader
              active={sortField === "album"}
              {sortAsc}
              onclick={() => toggleSort("album")}
              class="text-left hover:text-brand-text-primary transition-colors flex items-center gap-1 cursor-pointer font-semibold uppercase tracking-wider min-w-0"
            >
              {#snippet label(arrow)}<span class="truncate max-w-[calc(100%-1rem)]">{i18n.t('collection.tableHeaderAlbum')} {arrow}</span>{/snippet}
            </SortableHeader>
          {/if}

          {#if collectionStore.visibleColumns.composer}
            <SortableHeader
              active={sortField === "composer"}
              {sortAsc}
              onclick={() => toggleSort("composer")}
              class="text-left hover:text-brand-text-primary transition-colors flex items-center gap-1 cursor-pointer font-semibold uppercase tracking-wider min-w-0"
            >
              {#snippet label(arrow)}<span class="truncate max-w-[calc(100%-0.5rem)]">{i18n.t('collection.tableHeaderComposer')} {arrow}</span>{/snippet}
            </SortableHeader>
          {/if}
          {#if collectionStore.visibleColumns.album_artist}
            <SortableHeader
              active={sortField === "album_artist"}
              {sortAsc}
              onclick={() => toggleSort("album_artist")}
              class="text-left hover:text-brand-text-primary transition-colors flex items-center gap-1 cursor-pointer font-semibold uppercase tracking-wider min-w-0"
            >
              {#snippet label(arrow)}<span class="truncate max-w-[calc(100%-0.5rem)]">{i18n.t('collection.tableHeaderAlbumArtist')} {arrow}</span>{/snippet}
            </SortableHeader>
          {/if}
          {#if collectionStore.visibleColumns.format}
            <SortableHeader
              active={sortField === "filetype"}
              {sortAsc}
              onclick={() => toggleSort("filetype")}
              class="text-left hover:text-brand-text-primary transition-colors flex items-center gap-1 cursor-pointer font-semibold uppercase tracking-wider min-w-0"
            >
              {#snippet label(arrow)}<span class="truncate max-w-[calc(100%-0.5rem)]">{i18n.t('collection.tableHeaderFormat')} {arrow}</span>{/snippet}
            </SortableHeader>
          {/if}
          {#if collectionStore.visibleColumns.year}
            <SortableHeader
              active={sortField === "year"}
              {sortAsc}
              onclick={() => toggleSort("year")}
              class="text-left hover:text-brand-text-primary transition-colors flex items-center gap-1 cursor-pointer font-semibold uppercase tracking-wider min-w-0"
            >
              {#snippet label(arrow)}<span class="truncate max-w-[calc(100%-0.5rem)]">{i18n.t('collection.tableHeaderYear')} {arrow}</span>{/snippet}
            </SortableHeader>
          {/if}
          {#if collectionStore.visibleColumns.genre}
            <SortableHeader
              active={sortField === "genre"}
              {sortAsc}
              onclick={() => toggleSort("genre")}
              class="text-left hover:text-brand-text-primary transition-colors flex items-center gap-1 cursor-pointer font-semibold uppercase tracking-wider min-w-0"
            >
              {#snippet label(arrow)}<span class="truncate max-w-[calc(100%-0.5rem)]">{i18n.t('collection.tableHeaderGenre')} {arrow}</span>{/snippet}
            </SortableHeader>
          {/if}
          {#if collectionStore.visibleColumns.grouping}
            <SortableHeader
              active={sortField === "grouping"}
              {sortAsc}
              onclick={() => toggleSort("grouping")}
              class="text-left hover:text-brand-text-primary transition-colors flex items-center gap-1 cursor-pointer font-semibold uppercase tracking-wider min-w-0"
            >
              {#snippet label(arrow)}<span class="truncate max-w-[calc(100%-0.5rem)]">{i18n.t('collection.tableHeaderGrouping')} {arrow}</span>{/snippet}
            </SortableHeader>
          {/if}
          {#if collectionStore.visibleColumns.bpm}
            <SortableHeader
              active={sortField === "bpm"}
              {sortAsc}
              onclick={() => toggleSort("bpm")}
              class="text-left hover:text-brand-text-primary transition-colors flex items-center gap-1 cursor-pointer font-semibold uppercase tracking-wider min-w-0"
            >
              {#snippet label(arrow)}<span class="truncate max-w-[calc(100%-0.5rem)]">{i18n.t('collection.tableHeaderBpm')} {arrow}</span>{/snippet}
            </SortableHeader>
          {/if}
          {#if collectionStore.visibleColumns.initial_key}
            <SortableHeader
              active={sortField === "initial_key"}
              {sortAsc}
              onclick={() => toggleSort("initial_key")}
              class="text-left hover:text-brand-text-primary transition-colors flex items-center gap-1 cursor-pointer font-semibold uppercase tracking-wider min-w-0"
            >
              {#snippet label(arrow)}<span class="truncate max-w-[calc(100%-0.5rem)]">{i18n.t('collection.tableHeaderInitialKey')} {arrow}</span>{/snippet}
            </SortableHeader>
          {/if}
          {#if collectionStore.visibleColumns.bitrate}
            <SortableHeader
              active={sortField === "bitrate"}
              {sortAsc}
              onclick={() => toggleSort("bitrate")}
              class="text-left hover:text-brand-text-primary transition-colors flex items-center gap-1 cursor-pointer font-semibold uppercase tracking-wider min-w-0"
            >
              {#snippet label(arrow)}<span class="truncate max-w-[calc(100%-0.5rem)]">{i18n.t('collection.tableHeaderBitrate')} {arrow}</span>{/snippet}
            </SortableHeader>
          {/if}
          {#if collectionStore.visibleColumns.samplerate}
            <SortableHeader
              active={sortField === "samplerate"}
              {sortAsc}
              onclick={() => toggleSort("samplerate")}
              class="text-left hover:text-brand-text-primary transition-colors flex items-center gap-1 cursor-pointer font-semibold uppercase tracking-wider min-w-0"
            >
              {#snippet label(arrow)}<span class="truncate max-w-[calc(100%-0.5rem)]">{i18n.t('collection.tableHeaderSampleRate')} {arrow}</span>{/snippet}
            </SortableHeader>
          {/if}
          {#if collectionStore.visibleColumns.bitdepth}
            <SortableHeader
              active={sortField === "bitdepth"}
              {sortAsc}
              onclick={() => toggleSort("bitdepth")}
              class="text-left hover:text-brand-text-primary transition-colors flex items-center gap-1 cursor-pointer font-semibold uppercase tracking-wider min-w-0"
            >
              {#snippet label(arrow)}<span class="truncate max-w-[calc(100%-0.5rem)]">{i18n.t('collection.tableHeaderBitDepth')} {arrow}</span>{/snippet}
            </SortableHeader>
          {/if}
          {#if collectionStore.visibleColumns.channels}
            <SortableHeader
              active={sortField === "channels"}
              {sortAsc}
              onclick={() => toggleSort("channels")}
              class="text-left hover:text-brand-text-primary transition-colors flex items-center gap-1 cursor-pointer font-semibold uppercase tracking-wider min-w-0"
            >
              {#snippet label(arrow)}<span class="truncate max-w-[calc(100%-0.5rem)]">{i18n.t('collection.tableHeaderChannels')} {arrow}</span>{/snippet}
            </SortableHeader>
          {/if}
          {#if collectionStore.visibleColumns.filesize}
            <SortableHeader
              active={sortField === "filesize"}
              {sortAsc}
              onclick={() => toggleSort("filesize")}
              class="text-left hover:text-brand-text-primary transition-colors flex items-center gap-1 cursor-pointer font-semibold uppercase tracking-wider min-w-0"
            >
              {#snippet label(arrow)}<span class="truncate max-w-[calc(100%-0.5rem)]">{i18n.t('collection.tableHeaderFileSize')} {arrow}</span>{/snippet}
            </SortableHeader>
          {/if}
          {#if collectionStore.visibleColumns.rating}
            <SortableHeader
              active={sortField === "rating"}
              {sortAsc}
              onclick={() => toggleSort("rating")}
              class="flex items-center justify-center hover:text-brand-text-primary transition-colors cursor-pointer font-semibold uppercase tracking-wider min-w-0"
            >
              {#snippet label(arrow)}<span class="truncate">{i18n.t('collection.tableHeaderRating')} {arrow}</span>{/snippet}
            </SortableHeader>
          {/if}
          {#if collectionStore.visibleColumns.playcount}
            <SortableHeader
              active={sortField === "playcount"}
              {sortAsc}
              onclick={() => toggleSort("playcount")}
              class="text-center hover:text-brand-text-primary transition-colors flex items-center justify-center gap-1 cursor-pointer font-semibold uppercase tracking-wider min-w-0"
            >
              {#snippet label(arrow)}<span class="truncate">{i18n.t('collection.tableHeaderPlays')} {arrow}</span>{/snippet}
            </SortableHeader>
          {/if}
          {#if collectionStore.visibleColumns.skipcount}
            <SortableHeader
              active={sortField === "skipcount"}
              {sortAsc}
              onclick={() => toggleSort("skipcount")}
              class="text-center hover:text-brand-text-primary transition-colors flex items-center justify-center gap-1 cursor-pointer font-semibold uppercase tracking-wider min-w-0"
            >
              {#snippet label(arrow)}<span class="truncate">{i18n.t('collection.tableHeaderSkips')} {arrow}</span>{/snippet}
            </SortableHeader>
          {/if}
          {#if collectionStore.visibleColumns.lastplayed}
            <SortableHeader
              active={sortField === "lastplayed"}
              {sortAsc}
              onclick={() => toggleSort("lastplayed")}
              class="text-center hover:text-brand-text-primary transition-colors flex items-center justify-center gap-1 cursor-pointer font-semibold uppercase tracking-wider min-w-0"
            >
              {#snippet label(arrow)}<span class="truncate">{i18n.t('collection.tableHeaderLastPlayed')} {arrow}</span>{/snippet}
            </SortableHeader>
          {/if}
          {#if collectionStore.visibleColumns.added}
            <SortableHeader
              active={sortField === "added"}
              {sortAsc}
              onclick={() => toggleSort("added")}
              class="text-center hover:text-brand-text-primary transition-colors flex items-center justify-center gap-1 cursor-pointer font-semibold uppercase tracking-wider min-w-0"
            >
              {#snippet label(arrow)}<span class="truncate">{i18n.t('collection.tableHeaderAdded')} {arrow}</span>{/snippet}
            </SortableHeader>
          {/if}
          {#if collectionStore.visibleColumns.duration}
            <SortableHeader
              active={sortField === "length_nanosec"}
              {sortAsc}
              onclick={() => toggleSort("length_nanosec")}
              class="flex items-center justify-center hover:text-brand-text-primary transition-colors cursor-pointer font-semibold uppercase tracking-wider min-w-0"
            >
              {#snippet label(arrow)}<Clock class="w-4 h-4 shrink-0" /> {arrow}{/snippet}
            </SortableHeader>
          {/if}
          {#if collectionStore.visibleColumns.path}
            <SortableHeader
              active={sortField === "path"}
              {sortAsc}
              onclick={() => toggleSort("path")}
              class="text-left hover:text-brand-text-primary transition-colors flex items-center gap-1 cursor-pointer font-semibold uppercase tracking-wider min-w-0"
            >
              {#snippet label(arrow)}<span class="truncate max-w-[calc(100%-0.5rem)]">{i18n.t('collection.tableHeaderPath')} {arrow}</span>{/snippet}
            </SortableHeader>
          {/if}
          {#if collectionStore.visibleColumns.actions}
            <div class="text-center">{i18n.t('collection.tableHeaderActions')}</div>
          {/if}
        </div>
      </div>

      <!-- Rows -->
      <div class="divide-y divide-brand-border/40">
        {#if loading}
          <div class="py-16 text-center text-brand-text-primary text-sm">
            {i18n.t('home.loading')}
          </div>
        {:else if sortedSongs.length === 0}
          <div class="py-16 text-center select-none">
            <EmptyState icon={Music} title={emptyStateMessage} />
          </div>
        {:else}
          {#each sortedSongs as song, index (song.id)}
            {@const disconnected = !song.unavailable && collectionStore.isPathOnDisconnectedDrive(song.path)}
            {@const disabled = song.unavailable || disconnected}
            <div
              role="row"
              tabindex="0"
              onkeydown={(e) => {
                if (e.key === 'Enter' && !disabled) handlePlaySong(song);
              }}
              data-song-row="true"
              onclick={(e) => !disabled && handleSongClick(e, song)}
              ondblclick={() => !disabled && handlePlaySong(song)}
              oncontextmenu={(e) => handleContextMenu(e, song)}
              title={disconnected ? i18n.t('collection.driveDisconnectedTooltip') : undefined}
              class="grid items-center py-2.5 px-4 group transition-all duration-150 select-none text-sm border-b border-brand-border/40
                {disabled ? 'opacity-50 cursor-not-allowed' : 'cursor-pointer'}
                {selectedSongIds.has(song.id) ? 'bg-brand-accent/20 text-brand-accent-text-hover' : (playerStore.currentSong && playerStore.currentSong.id === song.id ? 'bg-brand-accent/10 text-brand-accent-text-hover' : 'hover:bg-brand-sidebar/40')}"
              style={gridColsStyle}
            >
              <div class="text-center text-brand-text-primary font-medium relative min-w-0">
                <div class="relative w-9 h-4 mx-auto flex items-center justify-center">
                  {#if playerStore.currentSong && playerStore.currentSong.id === song.id && playerStore.state === 'playing'}
                    <div class="flex items-center justify-center gap-0.5 h-4 w-4 absolute inset-0 group-hover:opacity-0 transition-opacity">
                      <NowPlayingBars />
                    </div>
                  {:else}
                    <span class="absolute inset-0 flex items-center justify-center group-hover:opacity-0 transition-opacity whitespace-nowrap">
                      {index + 1}
                    </span>
                  {/if}
                  <button
                    onclick={(e) => { e.stopPropagation(); if (!disabled) handlePlaySong(song); }}
                    class="absolute inset-0 flex items-center justify-center opacity-0 group-hover:opacity-100 text-brand-accent-text hover:text-brand-accent-text-hover transition-opacity cursor-pointer disabled:opacity-0 disabled:cursor-not-allowed"
                    disabled={disabled}
                    title={disconnected ? i18n.t('collection.driveDisconnectedTooltip') : i18n.t('collection.playSong')}
                  >
                    <Play class="w-4 h-4 fill-current" />
                  </button>
                </div>
              </div>
              {#if collectionStore.visibleColumns.title}
                <div class="font-medium truncate pr-4 min-w-0 {selectedSongIds.has(song.id) || (playerStore.currentSong && playerStore.currentSong.id === song.id) ? 'text-brand-accent-text-hover' : 'text-brand-text-primary'}">
                  <span class="truncate min-w-0" title={song.title}>{song.title || i18n.t('collection.unknownSong')}</span>
                </div>
              {/if}
              {#if collectionStore.visibleColumns.artist}
                <div class="text-brand-text-primary truncate pr-4 min-w-0">
                  {#if song.artist}
                    <LinkButton
                      onclick={(e) => { e.stopPropagation(); collectionStore.viewArtist(song.album_artist?.trim() || song.artist || ""); }}
                      class="text-brand-text-primary truncate min-w-0"
                      title={i18n.t('collection.filterByArtist', { artist: song.artist })}
                    >
                      {song.artist}
                    </LinkButton>
                  {:else}
                    <span class="text-brand-text-primary truncate min-w-0">{i18n.t('collection.unknownArtist')}</span>
                  {/if}
                </div>
              {/if}
              {#if collectionStore.visibleColumns.album}
                <div class="text-brand-text-primary truncate pr-4 min-w-0">
                  {#if song.album}
                    <LinkButton
                      onclick={(e) => { e.stopPropagation(); collectionStore.viewAlbum(song.album || ""); }}
                      class="text-brand-text-primary truncate min-w-0"
                      title={i18n.t('collection.filterByAlbum', { album: song.album })}
                    >
                      {song.album}
                    </LinkButton>
                  {:else}
                    <span class="text-brand-text-primary truncate min-w-0">{i18n.t('collection.unknownAlbum')}</span>
                  {/if}
                </div>
              {/if}

              {#if collectionStore.visibleColumns.composer}
                <div class="text-brand-text-primary truncate pr-4 min-w-0 text-xs font-medium" title={song.composer}>
                  {song.composer || "—"}
                </div>
              {/if}
              {#if collectionStore.visibleColumns.album_artist}
                <div class="text-brand-text-primary truncate pr-4 min-w-0 text-xs font-medium" title={song.album_artist}>
                  {song.album_artist || "—"}
                </div>
              {/if}
              {#if collectionStore.visibleColumns.format}
                <div class="text-brand-text-primary truncate pr-2 min-w-0 text-xs font-semibold uppercase">
                  {song.filetype ? song.filetype.toUpperCase() : "—"}
                </div>
              {/if}
              {#if collectionStore.visibleColumns.year}
                <div class="text-brand-text-primary truncate pr-2 min-w-0 text-xs font-medium">
                  {song.year || "—"}
                </div>
              {/if}
              {#if collectionStore.visibleColumns.genre}
                <div class="text-brand-text-primary truncate pr-2 min-w-0 text-xs font-medium" title={song.genre}>
                  {song.genre || "—"}
                </div>
              {/if}
              {#if collectionStore.visibleColumns.grouping}
                <div class="text-brand-text-primary truncate pr-2 min-w-0 text-xs font-medium" title={song.grouping}>
                  {song.grouping || "—"}
                </div>
              {/if}
              {#if collectionStore.visibleColumns.bpm}
                <div class="text-brand-text-primary truncate pr-2 min-w-0 text-xs font-mono">
                  {song.bpm || "—"}
                </div>
              {/if}
              {#if collectionStore.visibleColumns.initial_key}
                <div class="text-brand-text-primary truncate pr-2 min-w-0 text-xs font-mono">
                  {song.initial_key || "—"}
                </div>
              {/if}
              {#if collectionStore.visibleColumns.bitrate}
                <div class="text-brand-text-primary truncate pr-2 min-w-0 text-xs font-mono">
                  {song.bitrate ? `${song.bitrate}k` : "—"}
                </div>
              {/if}
              {#if collectionStore.visibleColumns.samplerate}
                <div class="text-brand-text-primary truncate pr-2 min-w-0 text-xs font-mono">
                  {formatSampleRate(song.samplerate)}
                </div>
              {/if}
              {#if collectionStore.visibleColumns.bitdepth}
                <div class="text-brand-text-primary truncate pr-2 min-w-0 text-xs font-mono">
                  {formatBitDepth(song.bitdepth)}
                </div>
              {/if}
              {#if collectionStore.visibleColumns.channels}
                <div class="text-brand-text-primary truncate pr-2 min-w-0 text-xs font-medium">
                  {formatChannels(song.channels)}
                </div>
              {/if}
              {#if collectionStore.visibleColumns.filesize}
                <div class="text-brand-text-primary truncate pr-2 min-w-0 text-xs font-mono">
                  {formatFileSize(song.filesize)}
                </div>
              {/if}
              {#if collectionStore.visibleColumns.rating}
                <div class="flex justify-center min-w-0" onclick={(e) => e.stopPropagation()} role="presentation">
                  <SongRating rating={song.rating} onRate={(r) => rateSong(song, r)} />
                </div>
              {/if}
              {#if collectionStore.visibleColumns.playcount}
                <div class="text-center text-brand-text-primary text-xs min-w-0">
                  {song.playcount || 0}
                </div>
              {/if}
              {#if collectionStore.visibleColumns.skipcount}
                <div class="text-center text-brand-text-primary text-xs min-w-0">
                  {song.skipcount || 0}
                </div>
              {/if}
              {#if collectionStore.visibleColumns.lastplayed}
                <div class="text-center text-brand-text-primary text-xs whitespace-nowrap">
                  {formatDate(song.lastplayed)}
                </div>
              {/if}
              {#if collectionStore.visibleColumns.added}
                <div class="text-center text-brand-text-primary text-xs whitespace-nowrap">
                  {formatDate(song.added)}
                </div>
              {/if}
              {#if collectionStore.visibleColumns.duration}
                <div class="text-center text-brand-text-primary text-xs min-w-0">
                  {formatDuration(song.length_nanosec)}
                </div>
              {/if}
              {#if collectionStore.visibleColumns.path}
                <div class="text-brand-text-primary truncate pr-4 min-w-0 text-xs font-mono" title={song.path}>
                  {song.path || "—"}
                </div>
              {/if}

              {#if collectionStore.visibleColumns.actions}
                <div class="text-center min-w-0">
                  <div class="flex items-center justify-center gap-2.5">
                    <button
                      onclick={(e) => { e.stopPropagation(); handleAddSongToPlaylist(song.id); }}
                      class="text-brand-text-primary hover:text-brand-accent-text transition-colors cursor-pointer"
                      title={playlistsStore.activeCustomPlaylist
                        ? i18n.t('collection.addPlaylistTooltip', { name: playlistsStore.activeCustomPlaylist.name })
                        : i18n.t('collection.addPlaylistTooltipDefault')}
                    >
                      <Plus class="w-4 h-4" />
                    </button>
                    <button
                      onclick={(e) => { e.stopPropagation(); openTagEditor(song.id); }}
                      class="text-brand-text-primary hover:text-brand-accent-text transition-colors cursor-pointer"
                      title={i18n.t('collection.editTagsTooltip')}
                    >
                      <Edit3 class="w-4 h-4" />
                    </button>
                  </div>
                </div>
              {/if}
            </div>
          {/each}
        {/if}
      </div>
    </div>

    {#if autoPlay && (kind === "genre" || kind === "decade") && playlistId !== undefined && playerStore.isAutoPlayExhausted(playlistId)}
      <div class="mt-4 p-3 rounded-lg bg-brand-sidebar/60 border border-brand-border/40 text-center text-xs text-brand-text-primary flex items-center justify-center gap-2 select-none">
        <CheckCircle2 class="w-4 h-4 text-brand-accent shrink-0" />
        <span>{i18n.t('playlists.allMatchingTracksAdded')}</span>
      </div>
    {/if}
  </div>
</div>

{#if editingSongId !== null}
  <TagEditor
    songId={editingSongId}
    onClose={() => { editingSongId = null; }}
    onSave={handleTagEditorSaved}
  />
{/if}

<svelte:window onkeydown={handleKeydown} onmousedown={handleWindowMouseDown} />

{#if contextMenuState}
  {@const song = contextMenuState.song}
  <SongContextMenu
    x={contextMenuState.x}
    y={contextMenuState.y}
    {song}
    selectedCount={selectedSongIds.size}
    selectedSongIds={Array.from(selectedSongIds)}
    onPlay={() => {
      if (selectedSongIds.size > 1) {
        handlePlaySelected();
      } else {
        handlePlaySong(song);
      }
    }}
    onAddToPlaylist={() => {
      if (selectedSongIds.size > 1) {
        handleBulkAddToPlaylist();
      } else {
        handleAddSongToPlaylist(song.id);
      }
    }}
    onGoToArtist={() => collectionStore.viewArtist(song.album_artist?.trim() || song.artist || "")}
    onGoToAlbum={() => collectionStore.viewAlbum(song.album || "")}
    onEditTags={() => openTagEditor(song.id)}
    onClose={() => { contextMenuState = null; }}
  />
{/if}

{#if selectedSongIds.size > 0}
  <SongSelectionToolbar
    count={selectedSongIds.size}
    onPlaySelected={handlePlaySelected}
    onAddToPlaylist={handleBulkAddToPlaylist}
    onClear={() => { selectedSongIds = new Set(); }}
  />
{/if}

{#if overflowMenuPos}
  <ContextMenu
    x={overflowMenuPos.x}
    y={overflowMenuPos.y}
    onClose={() => { overflowMenuPos = null; }}
  >
    <ContextMenuItem
      icon={Plus}
      label={playlistsStore.activeCustomPlaylist
        ? i18n.t('albumDetail.addAllToPlaylistTooltip', { name: playlistsStore.activeCustomPlaylist.name })
        : i18n.t('albumDetail.addAllToPlaylistTooltipDefault')}
      onclick={() => { handleAddAllToPlaylist(); overflowMenuPos = null; }}
      disabled={loading || songs.length === 0}
    />
    <ContextMenuItem
      icon={FolderPlus}
      label={i18n.t("playlists.saveAsCustomTooltip")}
      onclick={() => { handleSaveAsCustomPlaylist(); overflowMenuPos = null; }}
      disabled={loading || songs.length === 0}
    />

    {#if (kind === "genre" || kind === "decade") && playlistId !== undefined}
      <ContextMenuItem
        icon={RefreshCw}
        label={i18n.t("playlists.refreshPlaylistBtn", {}, "Refresh Playlist")}
        onclick={() => { handleRefreshAutoPlaylist(); overflowMenuPos = null; }}
        disabled={loading || isRefreshing}
      />
    {/if}

    {#if kind === "history"}
      <ContextMenuDivider />
      <ContextMenuItem
        icon={Eraser}
        destructive
        label={i18n.t("playlists.clearHistoryBtn", {}, "Clear History")}
        onclick={handleClearHistory}
        disabled={loading || songs.length === 0}
      />
    {/if}
  </ContextMenu>
{/if}

{#if showSaveModal}
  <Modal onClose={() => showSaveModal = false} maxWidth="max-w-sm">
    <div class="h-14 flex items-center justify-between px-6 border-b border-brand-border shrink-0 bg-brand-main">
      <div class="flex items-center gap-2">
        <FolderPlus class="w-4 h-4 text-brand-accent-text" />
        <h3 class="text-sm font-bold text-brand-text-primary">{i18n.t("playlists.saveAsCustomTitle", {}, "Save as Custom Playlist")}</h3>
      </div>
      <button onclick={() => showSaveModal = false} class="text-brand-text-secondary hover:text-brand-text-primary transition-colors cursor-pointer">
        <X class="w-4 h-4" />
      </button>
    </div>

    <form onsubmit={(e) => { e.preventDefault(); confirmSaveAsCustomPlaylist(); }} class="flex flex-col gap-4 p-6 bg-brand-sidebar">
      <div class="flex flex-col gap-1.5">
        <label for="save-playlist-name-input" class="text-xs font-semibold text-brand-text-secondary uppercase tracking-wider">
          {i18n.t("playlists.saveQueueNameLabel", {}, "Playlist Name")}
        </label>
        <Input
          id="save-playlist-name-input"
          type="text"
          bind:value={savePlaylistName}
          placeholder={i18n.t("playlists.saveQueueNamePlaceholder", {}, "My Playlist")}
          class="w-full"
          required
          autofocus
        />
      </div>

      <div class="flex items-center justify-end gap-3 pt-2">
        <Button onclick={() => showSaveModal = false} variant="secondary" size="sm">
          {i18n.t("playlists.cancel", {}, "Cancel")}
        </Button>
        <Button type="submit" variant="primary" size="sm">
          {i18n.t("playlists.saveQueueConfirm", {}, "Save")}
        </Button>
      </div>
    </form>
  </Modal>
{/if}

<style>
  :global(.carousel-scroll) {
    scrollbar-width: none;
    -ms-overflow-style: none;
  }
  :global(.carousel-scroll::-webkit-scrollbar) {
    display: none;
  }
</style>
