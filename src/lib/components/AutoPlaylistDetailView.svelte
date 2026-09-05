<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { listen } from "@tauri-apps/api/event";
  import { applySongStats, type SongStatsPayload } from "../utils/stats";
  import { collectionStore } from "../stores/collection.svelte";
  import { navigationStore, type AutoPlaylistRef } from "../stores/navigation.svelte";
  import { playerStore } from "../stores/player.svelte";
  import { playlistsStore } from "../stores/playlists.svelte";
  import { pinnedStore } from "../stores/pinned.svelte";
  import { autoPlaylistRefKeyFor } from "../types";
  import { songsToCoverStack } from "../utils/covers";
  import CoverStack from "./CoverStack.svelte";
  import TagEditor from "./TagEditor.svelte";
  import SongContextMenu from "./SongContextMenu.svelte";
  import EmptyState from "./EmptyState.svelte";
  import { tagsStore } from "../stores/tags.svelte";
  import PopulationModeTabs from "./PopulationModeTabs.svelte";
  import SongSelectionToolbar from "./SongSelectionToolbar.svelte";
  import PlayShuffleButtons from "./PlayShuffleButtons.svelte";
  import IconActionButton from "./IconActionButton.svelte";
  import ColumnSelector from "./ColumnSelector.svelte";
  import Input from "./Input.svelte";
  import ContextMenu from "./ContextMenu.svelte";
  import ContextMenuItem from "./ContextMenuItem.svelte";
  import ContextMenuDivider from "./ContextMenuDivider.svelte";
  import SongTable, { type SongTableRow } from "./SongTable.svelte";
  import {
    ClockIcon as Clock,
    PlusIcon as Plus,
    FolderPlusIcon as FolderPlus,
    MusicNotesIcon as Music,
    GaugeIcon as Gauge,
    ArrowsClockwiseIcon as RefreshCw,
    HeartIcon as Heart,
    CalendarIcon as Calendar,
    HourglassIcon as Hourglass,
    MagnifyingGlassIcon as Search,
    ArrowCounterClockwiseIcon as RotateCcw,
    ArrowClockwiseIcon as RotateCw,
    DotsThreeIcon as MoreHorizontal,
    XIcon as X,
    EraserIcon as Eraser,
    TagIcon as Tag,
    TrendUpIcon as TrendingUp,
    PushPinIcon as Pin,
    PushPinSlashIcon as PinOff,
    WarningIcon as AlertTriangle,
    ArrowSquareOutIcon as OpenInPicard,
    SunHorizonIcon as SunHorizon
  } from "phosphor-svelte";
  import { shuffleArray } from "../utils/shuffle";
  import type { PlaylistItem, QueuePopulationMode, Song } from "../types";
  import { i18n } from "../stores/i18n.svelte";
  import { toastStore } from "../stores/toast.svelte";
  import { getPopulationModeSuffix, getBpmBucketLabel } from "../utils/playlist";
  import { genreColorHsl, resolveGenreColorIndex } from "../utils/genrePalette";
  import { rememberScroll } from "../utils/scrollMemory";
  import { openInPicard } from "../utils/picard";
  import { picardStore } from "../stores/picard.svelte";
  import { compareSongs } from "../utils/songSort";
  import { toTitleCase } from "../utils/formatters";
  import Modal from "./Modal.svelte";
  import Button from "./Button.svelte";
  import AlbumTagEditor from "./AlbumTagEditor.svelte";

  let { view }: { view: AutoPlaylistRef } = $props();

  let kind = $derived(view.kind);

  // Default column widths (px or fr) — used when no saved width exists for a column.
  const AUTOPLAYLIST_COL_DEFAULTS: Partial<Record<keyof typeof collectionStore.visibleColumns, string>> = {
    title: "2fr", artist: "1.5fr", album: "1.5fr",
    composer: "1.5fr", album_artist: "1.5fr", format: "64px", year: "60px", originalyear: "60px",
    genre: "1.2fr", grouping: "1.2fr", bpm: "60px", initial_key: "60px",
    bitrate: "70px", samplerate: "75px", bitdepth: "65px", channels: "70px",
    filesize: "75px", rating: "96px", playcount: "70px", skipcount: "70px",
    lastplayed: "90px", added: "90px", duration: "80px", path: "2fr", library: "130px", actions: "80px",
  };

  let genre = $derived(view.genre);
  let artistTag = $derived(view.artistTag);
  let decade = $derived(view.decade);
  let bpm = $derived(view.bpm);
  let playlistId = $derived(view.playlistId);
  let updated = $derived(view.updated);
  let pinRefKey = $derived(autoPlaylistRefKeyFor(view));

  let songs = $state<Song[]>([]);
  let loading = $state(true);
  let editingSongId = $state<number | null>(null);
  let editingAlbumSongs = $state<Song[] | null>(null);
  let contextMenuState = $state<{ x: number; y: number; song: Song } | null>(null);

  let showSaveModal = $state(false);
  let savePlaylistName = $state("");

  let selectedKeys = $state<Set<string>>(new Set());

  /** Hover quick-action, ported from the old GenreBrowseView drill-down
   * (#548) — opens the full-album tag editor for whichever album the
   * hovered row belongs to. Applies to every auto-playlist kind, not just
   * genre, matching the parity the port was meant to preserve. */
  async function openAlbumEditor(song: Song) {
    if (!song.album) return;
    editingAlbumSongs = await invoke<Song[]>("get_songs_by_album", { album: song.album });
  }

  function handleRowContextMenu(event: MouseEvent, row: SongTableRow) {
    if (row.song) contextMenuState = { x: event.clientX, y: event.clientY, song: row.song };
  }

  async function handleBulkAddToPlaylist() {
    if (selectedKeys.size === 0) return;
    if (playlistsStore.activeCustomPlaylist) {
      await playlistsStore.addSongsToPlaylist(playlistsStore.activeCustomPlaylist.id, Array.from(selectedKeys, Number));
    } else {
      toastStore.show(i18n.t("collection.selectPlaylistFirstAlert"));
    }
  }

  function handlePlaySelected() {
    if (selectedKeys.size === 0) return;
    const selectedList = songs.filter((s) => selectedKeys.has(String(s.id)));
    if (selectedList.length > 0) {
      playerStore.playSongs(selectedList.map((s) => s.id), 0, playlistId, undefined, displayName);
    }
  }

  let displayName = $derived.by(() => {
    if (kind === "favourites") return i18n.t("playlists.autoFavourites");
    if (kind === "recently_added") return i18n.t("playlists.autoRecentlyAdded");
    if (kind === "most_played") return i18n.t("playlists.autoMostPlayed");
    if (kind === "history") return i18n.t("playlists.autoHistory");
    // Missing Metadata (#367) is a diagnostic singleton — always bare, no
    // population-mode suffix (population mode has no meaning here: the
    // point is to surface every affected song, not bias toward favourites).
    if (kind === "missing_metadata") return i18n.t("playlists.autoMissingMetadata");
    const base = kind === "decade"
      ? (decade || i18n.t("artistDetail.unknownYear"))
      : kind === "bpm"
        ? (() => {
            const pl = playlistsStore.playlists.find((p) => p.id === playlistId);
            return getBpmBucketLabel(pl?.dynamic_spec, pl?.name ?? bpm ?? "");
          })()
        : kind === "artist_tag"
          ? (() => {
              const pl = playlistsStore.playlists.find((p) => p.id === playlistId);
              return pl?.name || toTitleCase(artistTag || "") || i18n.t("playlists.artistTagAutoPlaylist");
            })()
          : kind === "daypart"
            ? (() => {
                // The row's own `name` IS the current bucket's mix name
                // (e.g. "Afternoon Mix") — updated in place by the backend
                // every time the daypart boundary crosses (#223).
                const pl = playlistsStore.playlists.find((p) => p.id === playlistId);
                return pl?.name || i18n.t("playlists.daypartAutoPlaylist");
              })()
            : kind === "no_genre"
            ? i18n.t("songTags.noGenre", {}, "No Genre")
            : genre || i18n.t("artistDetail.unknownGenre");
    const suffix = getPopulationModeSuffix(populationMode);
    return suffix ? i18n.t("playlists.populationModeTitleFormat", { base, suffix }) : base;
  });

  let topCovers = $derived(
    (kind === "genre" || kind === "decade" || kind === "bpm" || kind === "no_genre" || kind === "artist_tag" || kind === "daypart")
      ? songsToCoverStack(songs)
      : []
  );

  /** Genre auto-playlist header color (#548): follows the curated tag's own
   * color — a chip's playlist uses its parent card's color — instead of the
   * fixed teal gradient every other genre playlist used to share. Falls back
   * to `undefined` (the fixed teal gradient) when the tag isn't found in the
   * hierarchy at all (e.g. a fresh sub-threshold tag the hierarchy hasn't
   * caught up to yet). */
  let genreColorIndex = $derived.by(() => {
    if (kind !== "genre" || !genre) return undefined;
    return resolveGenreColorIndex(tagsStore.hierarchy, genre);
  });

  $effect(() => {
    if (kind === "genre" && tagsStore.hierarchy.length === 0) {
      tagsStore.loadHierarchy().catch((e) => console.error("Failed to load tag hierarchy:", e));
    }
  });

  let updatedLabel = $derived.by(() => {
    if (
      (kind !== "genre" && kind !== "decade" && kind !== "bpm" && kind !== "artist_tag" && kind !== "missing_metadata" && kind !== "daypart") ||
      updated === undefined
    )
      return null;
    return new Date(updated * 1000).toLocaleDateString();
  });

  let totalDurationLabel = $derived.by(() => {
    const totalNs = songs.reduce((sum, s) => sum + (s.length_nanosec ?? 0), 0);
    const totalMinutes = Math.round(totalNs / 1_000_000_000 / 60);
    const h = Math.floor(totalMinutes / 60);
    const m = totalMinutes % 60;
    return h > 0 ? `${h}h ${m}m` : `${m}m`;
  });

  async function fetchSongs(k: typeof kind, g: typeof genre, at: typeof artistTag, d: typeof decade, b: typeof bpm, pid: typeof playlistId): Promise<Song[]> {
    if ((k === "genre" || k === "decade" || k === "bpm" || k === "artist_tag" || k === "missing_metadata" || k === "daypart") && pid !== undefined) {
      const items = await invoke<PlaylistItem[]>("get_playlist_tracks", { playlistId: pid });
      return items.filter((item) => !!item.song).map((item) => item.song as Song);
    }
    if (k === "favourites") return invoke<Song[]>("get_favourite_songs");
    if (k === "recently_added") return invoke<Song[]>("get_recently_added_songs", { limit: 50 });
    if (k === "most_played") return invoke<Song[]>("get_most_played_songs", { limit: 50 });
    if (k === "history") return invoke<Song[]>("get_recently_played_songs", { limit: 100 });
    if (k === "decade") return invoke<Song[]>("get_songs_by_decade", { decade: d ?? "", limit: 50 });
    if (k === "bpm") return invoke<Song[]>("get_songs_by_bpm", { spec: b ?? "", limit: 50 });
    if (k === "artist_tag") return invoke<Song[]>("get_songs_by_artist_tag", { tag: at ?? "", limit: 500 });
    if (k === "no_genre") return invoke<Song[]>("get_songs_without_genre", { limit: 500 });
    // Sub-threshold fallback (no backing playlist row yet) — direct
    // curated-hierarchy query (#548), same matching a materialized genre
    // auto-playlist's own population uses.
    return invoke<Song[]>("get_songs_by_curated_tag", { tagName: g ?? "", limit: 500 });
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
    const at = artistTag;
    const d = decade;
    const b = bpm;
    const pid = playlistId;
    const count = backingTrackCount;
    loading = true;
    fetchSongs(k, g, at, d, b, pid)
      .then((fetchedSongs) => {
        if (kind !== k || genre !== g || artistTag !== at || decade !== d || bpm !== b || playlistId !== pid) return;
        songs = fetchedSongs;
      })
      .catch((err) => {
        console.error("Failed to load auto-playlist detail:", err);
      })
      .finally(() => {
        if (kind === k && genre === g && artistTag === at && decade === d && bpm === b && playlistId === pid) loading = false;
      });
  });

  async function handlePlaySong(song: Song) {
    const index = songs.findIndex((s) => s.id === song.id);
    const songIds = songs.map((s) => s.id);
    const queuePl = await playlistsStore.requireQueue();
    await playerStore.playSongs(songIds, index >= 0 ? index : 0, queuePl?.id, undefined, "Queue");
    if (queuePl) {
      playlistsStore.selectPlaylist(queuePl.id);
      navigationStore.viewPlaylist(queuePl.id);
    }
  }

  async function handlePlayAll() {
    if (songs.length === 0) return;
    const queuePl = await playlistsStore.requireQueue();
    await playerStore.setShuffleMode("off");
    await playerStore.playSongs(songs.map((s) => s.id), 0, queuePl?.id, undefined, "Queue");
    if (queuePl) {
      playlistsStore.selectPlaylist(queuePl.id);
      navigationStore.viewPlaylist(queuePl.id);
    }
  }

  async function handleShufflePlay() {
    if (songs.length === 0) return;
    const queuePl = await playlistsStore.requireQueue();
    const shuffledIds = shuffleArray(songs.map((s) => s.id));
    await playerStore.setShuffleMode("off");
    await playerStore.playSongs(shuffledIds, 0, queuePl?.id, undefined, "Queue");
    if (queuePl) {
      playlistsStore.selectPlaylist(queuePl.id);
      navigationStore.viewPlaylist(queuePl.id);
    }
  }

  async function handleAddSongToPlaylist(songId: number) {
    const songObj = songs.find((s) => s.id === songId);
    await playlistsStore.addSongsToActiveTarget([songId], songObj?.title || "Song");
  }

  /** Bulk "Open in Picard" from the Missing Metadata playlist's overflow menu
   * (#367) — the primary entry point for handing everything in the list off
   * to Picard at once. Opens just the current selection if any songs are
   * selected, otherwise every song in the list. */
  function handleOpenAllInPicard() {
    const ids = selectedKeys.size > 0 ? Array.from(selectedKeys, Number) : songs.map((s) => s.id);
    openInPicard(ids);
  }

  async function handleAddAllToPlaylist() {
    if (songs.length === 0) return;
    await playlistsStore.addSongsToActiveTarget(
      songs.map((s) => s.id),
      displayName || "Playlist"
    );
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
      navigationStore.viewPlaylist(created.id);
      showSaveModal = false;
    } catch (err) {
      console.error("Failed to save auto-playlist as custom playlist:", err);
    }
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
    try {
      await playlistsStore.setPlaylistPopulationMode(playlistId, newMode);
      songs = await fetchSongs(kind, genre, artistTag, decade, bpm, playlistId);
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
    try {
      await playlistsStore.refreshAutoPlaylist(playlistId);
      songs = await fetchSongs(kind, genre, artistTag, decade, bpm, playlistId);
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
    tagsStore.load();
    loading = true;
    fetchSongs(kind, genre, artistTag, decade, bpm, playlistId)
      .then((fetchedSongs) => {
        songs = fetchedSongs;
      })
      .catch((err) => console.error(err))
      .finally(() => (loading = false));
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
      if (kind === "history" || kind === "favourites" || kind === "recently_added" || kind === "most_played") {
        try {
          songs = await fetchSongs(kind, genre, artistTag, decade, bpm, playlistId);
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

  function toggleSort(field: string) {
    const f = field as AutoPlaylistSortField;
    if (sortField === f) {
      sortAsc = !sortAsc;
    } else {
      sortField = f;
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
    return [...list].sort((a, b) => compareSongs(a, b, field, sortAsc));
  });

  function songToRow(song: Song): SongTableRow {
    const disconnected = !song.unavailable && collectionStore.isPathOnDisconnectedDrive(song.path);
    return {
      key: String(song.id),
      song,
      disabled: song.unavailable || disconnected,
      disabledTooltip: disconnected ? i18n.t("collection.driveDisconnectedTooltip") : undefined,
    };
  }

  let tableRows = $derived(sortedSongs.map(songToRow));

</script>

{#snippet autoPlaylistEmptyState()}
  <EmptyState icon={Music} title={emptyStateMessage} />
{/snippet}

<div
  class="flex-1 flex flex-col overflow-y-auto carousel-scroll bg-brand-main text-brand-text-secondary h-full"
  use:rememberScroll={`autoplaylist:${view.kind}:${view.genre ?? view.decade ?? view.bpm ?? ""}`}
>
  <div class="relative z-30 w-full border-b border-brand-border/60 bg-brand-main/60 backdrop-blur-md px-6 pt-6 pb-6 shrink-0">
    <div class="flex items-stretch justify-between gap-6 relative z-10">
      <div class="flex flex-col justify-end gap-1.5 min-w-0 flex-1">
        <h1 class="text-3xl sm:text-4xl font-heading font-bold text-brand-text-primary leading-snug truncate py-0.5" title={displayName}>
          {displayName}
        </h1>

        <div class="flex flex-wrap items-center gap-x-2 gap-y-1 text-xs text-brand-text-secondary font-medium">
          <span>{songs.length === 1 ? i18n.t('playlists.oneSong') : i18n.t('playlists.songsCount', { count: songs.length })}</span>
          <span>•</span>
          <span>{totalDurationLabel}</span>
          {#if updatedLabel}
            <span>•</span>
            <span>{i18n.t('playlists.updatedOn', { date: updatedLabel })}</span>
          {/if}
        </div>

        <div class="flex flex-wrap items-center gap-3 mt-3 select-none">
          <PlayShuffleButtons
            onPlayAll={handlePlayAll}
            onShufflePlay={handleShufflePlay}
            disabled={loading || songs.length === 0}
            class="shrink-0"
          />
          <IconActionButton
            onclick={() => pinnedStore.toggle("auto_playlist", pinRefKey)}
            title={pinnedStore.isPinned("auto_playlist", pinRefKey)
              ? i18n.t("playlists.contextMenuUnpinHome")
              : i18n.t("playlists.contextMenuPinHome")}
            class="shrink-0"
          >
            {#snippet icon()}
              {#if pinnedStore.isPinned("auto_playlist", pinRefKey)}
                <PinOff class="w-4 h-4" />
              {:else}
                <Pin class="w-4 h-4" />
              {/if}
            {/snippet}
          </IconActionButton>
          <ColumnSelector align="left" iconOnly />
        </div>

        <div class="flex flex-wrap items-center gap-2.5 mt-2.5 select-none relative z-40">
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
                class="absolute right-3 top-1/2 -translate-y-1/2 text-brand-text-secondary/60 hover:text-brand-text-primary p-0.5"
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
              class="flex items-center justify-center w-10 h-10 rounded-full border border-brand-border text-brand-text-secondary hover:text-brand-accent-text hover:bg-brand-sidebar transition-colors shadow-xs"
            >
              <MoreHorizontal class="w-4 h-4" />
            </button>
          </div>
        </div>

        {#if (kind === "genre" || kind === "decade" || kind === "bpm" || kind === "artist_tag" || kind === "daypart") && playlistId !== undefined}
          <div class="flex flex-wrap items-center gap-2.5 mt-2.5 select-none relative z-40">
            <!-- Queue population mode tabs (#120): what bias to (re)populate this auto-playlist with -->
            <PopulationModeTabs
              mode={populationMode}
              disabled={loading || isChangingMode}
              onChange={handleChangePopulationMode}
            />
          </div>
        {/if}
      </div>

      <div class="relative w-40 h-40 hidden sm:block shrink-0">
        {#if kind === "genre" && topCovers.length > 0}
          <div
            class="w-full h-full bg-brand-main flex items-center justify-center overflow-hidden border relative"
            style={genreColorIndex !== undefined
              ? `background-image: linear-gradient(to bottom right, color-mix(in srgb, ${genreColorHsl(genreColorIndex)} 25%, transparent), color-mix(in srgb, ${genreColorHsl(genreColorIndex)} 15%, transparent)); border-color: color-mix(in srgb, ${genreColorHsl(genreColorIndex)} 30%, transparent); box-shadow: 0 0 28px 3px color-mix(in srgb, ${genreColorHsl(genreColorIndex)} 40%, transparent);`
              : "background-image: linear-gradient(to bottom right, rgb(5 150 105 / 0.25), rgb(52 211 153 / 0.15)); border-color: rgb(52 211 153 / 0.3); box-shadow: 0 0 28px 3px rgb(52 211 153 / 0.4);"}
          >
            <CoverStack covers={topCovers} sizeClass="w-[82%] h-[82%]" />
          </div>
        {:else if kind === "artist_tag" && topCovers.length > 0}
          <div class="w-full h-full bg-brand-main bg-gradient-to-br from-[#EA580C]/25 to-[#FB923C]/15 border-[#FB923C]/30 shadow-[0_0_28px_3px_rgba(251,146,60,0.4)] flex items-center justify-center overflow-hidden border relative">
            <CoverStack covers={topCovers} sizeClass="w-[82%] h-[82%]" />
          </div>
        {:else if (kind === "decade" || kind === "bpm") && topCovers.length > 0}
          <div class="w-full h-full bg-brand-main bg-gradient-to-br {kind === 'decade' ? 'from-[#2563EB]/25 to-[#38BDF8]/15 border-[#38BDF8]/30 shadow-[0_0_28px_3px_rgba(56,189,248,0.4)]' : 'from-[#C026D3]/25 to-[#E879F9]/15 border-[#E879F9]/30 shadow-[0_0_28px_3px_rgba(232,121,249,0.4)]'} flex items-center justify-center overflow-hidden border relative">
            <CoverStack covers={topCovers} sizeClass="w-[82%] h-[82%]" />
          </div>
        {:else if kind === "daypart" && topCovers.length > 0}
          <div class="w-full h-full bg-brand-main bg-gradient-to-br from-[#0D9488]/25 to-[#2DD4BF]/15 border-[#2DD4BF]/30 shadow-[0_0_28px_3px_rgba(45,212,191,0.4)] flex items-center justify-center overflow-hidden border relative">
            <CoverStack covers={topCovers} sizeClass="w-[82%] h-[82%]" />
          </div>
        {:else if kind === "no_genre" && topCovers.length > 0}
          <div class="w-full h-full bg-brand-main bg-gradient-to-br from-slate-700/40 to-slate-900/30 flex items-center justify-center overflow-hidden border border-slate-400/20 shadow-[0_0_28px_3px_rgba(100,116,139,0.3)] relative">
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
        {:else if kind === "most_played"}
          <div class="w-full h-full bg-brand-main bg-gradient-to-br from-[#DC2626]/25 to-[#F87171]/15 flex items-center justify-center overflow-hidden border border-[#F87171]/30 shadow-[0_0_28px_3px_rgba(248,113,113,0.4)]">
            <TrendingUp class="w-16 h-16 text-[#DC2626]" />
          </div>
        {:else if kind === "history"}
          <div class="w-full h-full bg-brand-main bg-gradient-to-br from-[#8B5CF6]/25 to-[#A78BFA]/15 flex items-center justify-center overflow-hidden border border-[#A78BFA]/30 shadow-[0_0_28px_3px_rgba(167,139,250,0.4)]">
            <Hourglass class="w-16 h-16 text-[#8B5CF6]" />
          </div>
        {:else if kind === "decade"}
          <div class="w-full h-full bg-brand-main bg-gradient-to-br from-[#2563EB]/25 to-[#38BDF8]/15 flex items-center justify-center overflow-hidden border border-[#38BDF8]/30 shadow-[0_0_28px_3px_rgba(56,189,248,0.4)]">
            <Calendar class="w-16 h-16 text-[#38BDF8]" />
          </div>
        {:else if kind === "artist_tag"}
          <div class="w-full h-full bg-brand-main bg-gradient-to-br from-[#EA580C]/25 to-[#FB923C]/15 flex items-center justify-center overflow-hidden border border-[#FB923C]/30 shadow-[0_0_28px_3px_rgba(251,146,60,0.4)]">
            <Tag class="w-16 h-16 text-[#FB923C]" />
          </div>
        {:else if kind === "bpm"}
          <div class="w-full h-full bg-brand-main bg-gradient-to-br from-[#C026D3]/25 to-[#E879F9]/15 flex items-center justify-center overflow-hidden border border-[#E879F9]/30 shadow-[0_0_28px_3px_rgba(232,121,249,0.4)]">
            <Gauge class="w-16 h-16 text-[#E879F9]" />
          </div>
        {:else if kind === "no_genre"}
          <div class="w-full h-full bg-brand-main bg-gradient-to-br from-slate-700/40 to-slate-900/30 flex items-center justify-center overflow-hidden border border-slate-400/20 shadow-[0_0_28px_3px_rgba(100,116,139,0.3)]">
            <Music class="w-16 h-16 text-slate-300" />
          </div>
        {:else if kind === "missing_metadata"}
          <div class="w-full h-full bg-brand-main bg-gradient-to-br from-amber-600/25 to-amber-400/15 flex items-center justify-center overflow-hidden border border-amber-400/30 shadow-[0_0_28px_3px_rgba(245,158,11,0.4)]">
            <AlertTriangle class="w-16 h-16 text-amber-500" />
          </div>
        {:else if kind === "daypart"}
          <div class="w-full h-full bg-brand-main bg-gradient-to-br from-[#0D9488]/25 to-[#2DD4BF]/15 flex items-center justify-center overflow-hidden border border-[#2DD4BF]/30 shadow-[0_0_28px_3px_rgba(45,212,191,0.4)]">
            <SunHorizon class="w-16 h-16 text-[#2DD4BF]" />
          </div>
        {:else}
          <div
            class="w-full h-full bg-brand-main flex items-center justify-center overflow-hidden border"
            style={genreColorIndex !== undefined
              ? `background-image: linear-gradient(to bottom right, color-mix(in srgb, ${genreColorHsl(genreColorIndex)} 25%, transparent), color-mix(in srgb, ${genreColorHsl(genreColorIndex)} 15%, transparent)); border-color: color-mix(in srgb, ${genreColorHsl(genreColorIndex)} 30%, transparent); box-shadow: 0 0 28px 3px color-mix(in srgb, ${genreColorHsl(genreColorIndex)} 40%, transparent);`
              : "background-image: linear-gradient(to bottom right, rgb(5 150 105 / 0.25), rgb(52 211 153 / 0.15)); border-color: rgb(52 211 153 / 0.3); box-shadow: 0 0 28px 3px rgb(52 211 153 / 0.4);"}
          >
            <Music class="w-16 h-16" style={genreColorIndex !== undefined ? `color: ${genreColorHsl(genreColorIndex)}` : "color: #34D399"} />
          </div>
        {/if}
      </div>
    </div>
  </div>

  <div class="px-6 py-6 flex flex-col" class:pb-28={!!playerStore.currentSong}>
    <div class="border border-brand-border/60 rounded-xl bg-brand-sidebar/30 backdrop-blur-md relative overflow-hidden table-surface-blur">
      <SongTable
        rows={tableRows}
        mode="position"
        leadingColumnWidth="56px"
        colDefaults={AUTOPLAYLIST_COL_DEFAULTS}
        {sortField}
        {sortAsc}
        onToggleSort={toggleSort}
        positionSortField="default"
        bind:selectedKeys
        {loading}
        emptyState={autoPlaylistEmptyState}
        onRowDoubleClick={(row) => row.song && handlePlaySong(row.song)}
        onRowContextMenu={handleRowContextMenu}
        onRate={rateSong}
        onAddToPlaylist={(song) => handleAddSongToPlaylist(song.id)}
        onEditTags={(song) => openTagEditor(song.id)}
        onEditAlbum={openAlbumEditor}
        onOpenInPicard={(song) => openInPicard([song.id])}
      />
    </div>

  </div>
</div>

{#if editingSongId !== null}
  <TagEditor
    songId={editingSongId}
    onClose={() => { editingSongId = null; }}
    onSave={handleTagEditorSaved}
  />
{/if}

{#if editingAlbumSongs !== null && editingAlbumSongs.length > 0}
  <AlbumTagEditor
    songIds={editingAlbumSongs.map((s) => s.id)}
    initialAlbum={editingAlbumSongs[0].album}
    initialAlbumSort={editingAlbumSongs[0].albumsort}
    initialAlbumArtist={editingAlbumSongs[0].album_artist || editingAlbumSongs[0].artist}
    initialAlbumArtistSort={editingAlbumSongs[0].album_artist_sort || editingAlbumSongs[0].artistsort}
    initialGenre={editingAlbumSongs[0].genre}
    initialGenreSort={editingAlbumSongs[0].genresort}
    initialYear={editingAlbumSongs[0].year}
    initialDisc={editingAlbumSongs[0].disc}
    initialCompilation={editingAlbumSongs[0].compilation}
    hasEmbeddedArt={editingAlbumSongs.some((s) => s.art_embedded)}
    onClose={() => { editingAlbumSongs = null; }}
    onSave={handleTagEditorSaved}
  />
{/if}

{#if contextMenuState}
  {@const song = contextMenuState.song}
  <SongContextMenu
    x={contextMenuState.x}
    y={contextMenuState.y}
    {song}
    selectedCount={selectedKeys.size}
    selectedSongIds={Array.from(selectedKeys, Number)}
    onPlay={() => {
      if (selectedKeys.size > 1) {
        handlePlaySelected();
      } else {
        handlePlaySong(song);
      }
    }}
    onAddToPlaylist={() => {
      if (selectedKeys.size > 1) {
        handleBulkAddToPlaylist();
      } else {
        handleAddSongToPlaylist(song.id);
      }
    }}
    onGoToArtist={() => navigationStore.viewArtist(song.album_artist?.trim() || song.artist || "")}
    onGoToAlbum={() => navigationStore.viewAlbum(song.album || "")}
    onEditTags={() => openTagEditor(song.id)}
    onOpenInPicard={() => openInPicard(selectedKeys.size > 1 ? Array.from(selectedKeys, Number) : [song.id])}
    onClose={() => { contextMenuState = null; }}
  />
{/if}

{#if selectedKeys.size > 0}
  <SongSelectionToolbar
    count={selectedKeys.size}
    onPlaySelected={handlePlaySelected}
    onAddToPlaylist={handleBulkAddToPlaylist}
    onClear={() => { selectedKeys = new Set(); }}
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

    {#if (kind === "genre" || kind === "decade" || kind === "bpm" || kind === "missing_metadata" || kind === "daypart") && playlistId !== undefined}
      <ContextMenuItem
        icon={RefreshCw}
        label={i18n.t("playlists.refreshPlaylistBtn", {}, "Refresh Playlist")}
        onclick={() => { handleRefreshAutoPlaylist(); overflowMenuPos = null; }}
        disabled={loading || isRefreshing}
      />
    {/if}

    {#if kind === "missing_metadata"}
      <ContextMenuDivider />
      <ContextMenuItem
        icon={OpenInPicard}
        label={selectedKeys.size > 0
          ? i18n.t("picard.openSelectedInPicard", { count: selectedKeys.size })
          : i18n.t("picard.openAllInPicard")}
        onclick={() => { handleOpenAllInPicard(); overflowMenuPos = null; }}
        disabled={loading || songs.length === 0 || !picardStore.available}
        title={picardStore.available ? undefined : i18n.t("picard.notFoundTooltip")}
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
      <button onclick={() => showSaveModal = false} class="text-brand-text-secondary hover:text-brand-text-primary transition-colors">
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

