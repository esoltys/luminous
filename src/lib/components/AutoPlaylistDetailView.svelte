<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { listen } from "@tauri-apps/api/event";
  import { applySongStats, type SongStatsPayload } from "../utils/stats";
  import { collectionStore, type AutoPlaylistRef } from "../stores/collection.svelte";
  import { playerStore } from "../stores/player.svelte";
  import { playlistsStore } from "../stores/playlists.svelte";
  import { getArtistGradient } from "../utils/artist";
  import { songsToCoverStack } from "../utils/covers";
  import CoverStack from "./CoverStack.svelte";
  import SongRating from "./SongRating.svelte";
  import TagEditor from "./TagEditor.svelte";
  import SongContextMenu from "./SongContextMenu.svelte";
  import { Play, Shuffle, Plus, FolderPlus, Edit3, Music, ListMusic, RefreshCw, RotateCw, CheckCircle2 } from "lucide-svelte";
  import type { PlaylistItem, Song } from "../types";
  import { i18n } from "../stores/i18n.svelte";

  let { view }: { view: AutoPlaylistRef } = $props();

  let kind = $derived(view.kind);
  let genre = $derived(view.genre);
  let decade = $derived(view.decade);
  let playlistId = $derived(view.playlistId);
  let updated = $derived(view.updated);

  let songs = $state<Song[]>([]);
  let loading = $state(true);
  let editingSongId = $state<number | null>(null);
  let contextMenuState = $state<{ x: number; y: number; song: Song } | null>(null);

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
      alert(i18n.t("collection.selectPlaylistFirstAlert"));
    }
  }

  function handlePlaySelected() {
    if (selectedSongIds.size === 0) return;
    const selectedList = songs.filter((s) => selectedSongIds.has(s.id));
    if (selectedList.length > 0) {
      playerStore.playSongs(selectedList.map((s) => s.id), 0, playlistId);
    }
  }

  let displayName = $derived.by(() => {
    if (kind === "favourites") return i18n.t("playlists.autoFavourites");
    if (kind === "recently_added") return i18n.t("playlists.autoRecentlyAdded");
    if (kind === "decade") return decade || i18n.t("artistDetail.unknownYear");
    return genre || i18n.t("artistDetail.unknownGenre");
  });

  let topCovers = $derived(songsToCoverStack(songs));

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
        songs = fetchedSongs.filter((s) => !collectionStore.isFormatExcluded(s.filetype));
      })
      .catch((err) => {
        console.error("Failed to load auto-playlist detail:", err);
      })
      .finally(() => {
        if (kind === k && genre === g && decade === d && playlistId === pid) loading = false;
      });
  });

  function handlePlaySong(song: Song) {
    const index = songs.findIndex((s) => s.id === song.id);
    const songIds = songs.map((s) => s.id);
    playerStore.playSongs(songIds, index >= 0 ? index : 0, playlistId);
  }

  async function handlePlayAll() {
    if (songs.length === 0) return;
    await playerStore.setShuffleMode("off");
    await playerStore.playSongs(songs.map((s) => s.id), 0, playlistId);
  }

  async function handleShufflePlay() {
    if (songs.length === 0) return;
    const ids = songs.map((s) => s.id);
    const randomIndex = Math.floor(Math.random() * ids.length);
    await playerStore.setShuffleMode("all");
    await playerStore.playSongs(ids, randomIndex, playlistId);
  }

  async function handleAddSongToPlaylist(songId: number) {
    if (playlistsStore.activeCustomPlaylist) {
      await playlistsStore.addSongsToPlaylist(playlistsStore.activeCustomPlaylist.id, [songId]);
    } else {
      alert(i18n.t("collection.selectPlaylistFirstAlert"));
    }
  }

  async function handleAddAllToPlaylist() {
    if (songs.length === 0) return;
    if (playlistsStore.activeCustomPlaylist) {
      await playlistsStore.addSongsToPlaylist(playlistsStore.activeCustomPlaylist.id, songs.map((s) => s.id));
    } else {
      alert(i18n.t("collection.selectPlaylistFirstAlert"));
    }
  }

  // "Save as Custom Playlist" always duplicates the currently-loaded songs into
  // a brand-new custom playlist, rather than detaching this auto-playlist in
  // place — the auto-playlist keeps existing/refreshing independently.
  async function handleSaveAsCustomPlaylist() {
    if (songs.length === 0) return;
    await playlistsStore.createPlaylist(displayName);
    if (playlistsStore.activePlaylistId !== null) {
      await playlistsStore.addSongsToPlaylist(playlistsStore.activePlaylistId, songs.map((s) => s.id));
      collectionStore.viewPlaylist(playlistsStore.activePlaylistId);
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

  let isRefreshing = $state(false);

  async function handleRefreshAutoPlaylist() {
    if (playlistId === undefined || isRefreshing) return;
    isRefreshing = true;
    playerStore.clearExhausted(playlistId);
    try {
      await playlistsStore.refreshAutoPlaylist(playlistId);
      const fetchedSongs = await fetchSongs(kind, genre, decade, playlistId);
      songs = fetchedSongs.filter((s) => !collectionStore.isFormatExcluded(s.filetype));
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
        songs = fetchedSongs.filter((s) => !collectionStore.isFormatExcluded(s.filetype));
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
    listen<SongStatsPayload>("song-stats-changed", (event) => {
      const song = songs.find((s) => s.id === event.payload.song_id);
      if (song) applySongStats(song, event.payload);
    }).then((fn) => {
      if (disposed) fn();
      else unlisten = fn;
    });
    return () => {
      disposed = true;
      unlisten?.();
    };
  });

  type AutoPlaylistSortField = "default" | "track" | "title" | "artist" | "album" | "rating" | "duration";
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

  let sortedSongs = $derived.by(() => {
    if (sortField === "default") {
      return sortAsc ? songs : [...songs].reverse();
    }
    return [...songs].sort((a, b) => {
      let valA: string | number = "";
      let valB: string | number = "";

      if (sortField === "track") {
        valA = a.track ?? 0;
        valB = b.track ?? 0;
      } else if (sortField === "title") {
        valA = a.title?.toLowerCase() ?? "";
        valB = b.title?.toLowerCase() ?? "";
      } else if (sortField === "artist") {
        valA = a.artist?.toLowerCase() ?? "";
        valB = b.artist?.toLowerCase() ?? "";
      } else if (sortField === "album") {
        valA = a.album?.toLowerCase() ?? "";
        valB = b.album?.toLowerCase() ?? "";
      } else if (sortField === "rating") {
        valA = a.rating ?? 0;
        valB = b.rating ?? 0;
      } else if (sortField === "duration") {
        valA = a.length_nanosec ?? 0;
        valB = b.length_nanosec ?? 0;
      }

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

<div class="flex-1 flex flex-col overflow-y-auto bg-brand-main text-brand-text-secondary h-full">
  <!-- Auto-Playlist Hero & Summary Banner Header -->
  <div class="relative w-full border-b border-brand-border/60 bg-brand-main/60 backdrop-blur-md px-6 pt-6 pb-6">
    <div class="flex items-end justify-between gap-6 relative z-10">
      <!-- Left Title & Summary Metadata -->
      <div class="flex flex-col justify-end gap-2 min-w-0 max-w-xl">
        <h1 class="text-3xl sm:text-4xl font-extrabold text-brand-text-primary leading-snug truncate py-0.5" title={displayName}>
          {displayName}
        </h1>

        <div class="flex flex-wrap items-center gap-x-2 gap-y-1 text-xs text-brand-text-secondary/85 mt-1 font-medium">
          <span>{songs.length === 1 ? i18n.t('playlists.oneSong') : i18n.t('playlists.songsCount', { count: songs.length })}</span>
          <span>•</span>
          <span>{totalDurationLabel}</span>
          {#if updatedLabel}
            <span>•</span>
            <span>{i18n.t('playlists.updatedOn', { date: updatedLabel })}</span>
          {/if}
        </div>

        <!-- Control Buttons -->
        <div class="flex flex-wrap items-center gap-3 mt-3 select-none">
          <button
            onclick={handlePlayAll}
            disabled={loading || songs.length === 0}
            class="flex items-center gap-2 px-5 py-2 rounded-full bg-brand-accent hover:bg-brand-accent-hover text-brand-accent-contrast font-semibold text-sm transition-colors cursor-pointer disabled:opacity-50 disabled:cursor-not-allowed shadow-md shadow-brand-accent/20 shrink-0"
          >
            <Play class="w-4 h-4 fill-current" /> {i18n.t('artistDetail.playAll')}
          </button>
          <button
            onclick={handleShufflePlay}
            disabled={loading || songs.length === 0}
            class="flex items-center gap-2 px-5 py-2 rounded-full border border-brand-border text-brand-text-primary hover:bg-brand-sidebar font-semibold text-sm transition-colors cursor-pointer disabled:opacity-50 disabled:cursor-not-allowed shrink-0"
          >
            <Shuffle class="w-4 h-4" /> {i18n.t('artistDetail.shuffleAndPlay')}
          </button>
          <button
            onclick={handleAddAllToPlaylist}
            disabled={loading || songs.length === 0}
            title={playlistsStore.activeCustomPlaylist
              ? i18n.t('albumDetail.addAllToPlaylistTooltip', { name: playlistsStore.activeCustomPlaylist.name })
              : i18n.t('albumDetail.addAllToPlaylistTooltipDefault')}
            class="flex items-center justify-center w-10 h-10 rounded-full border border-brand-border text-brand-text-secondary hover:text-brand-accent-text hover:bg-brand-sidebar transition-colors cursor-pointer disabled:opacity-50 disabled:cursor-not-allowed shadow-xs shrink-0"
          >
            <Plus class="w-4 h-4" />
          </button>
          <button
            onclick={handleSaveAsCustomPlaylist}
            disabled={loading || songs.length === 0}
            title={i18n.t('playlists.saveAsCustomTooltip')}
            class="flex items-center justify-center w-10 h-10 rounded-full border border-brand-border text-brand-text-secondary hover:text-brand-accent-text hover:bg-brand-sidebar transition-colors cursor-pointer disabled:opacity-50 disabled:cursor-not-allowed shadow-xs shrink-0"
          >
            <FolderPlus class="w-4 h-4" />
          </button>

          {#if (kind === "genre" || kind === "decade") && playlistId !== undefined}
            <!-- Refresh button: force-regenerate playlist with fresh songs from library -->
            <button
              onclick={handleRefreshAutoPlaylist}
              disabled={loading || isRefreshing}
              title={i18n.t('playlists.refreshAutoPlaylistTooltip')}
              class="flex items-center justify-center w-10 h-10 rounded-full border border-brand-border text-brand-text-secondary hover:text-brand-accent-text hover:bg-brand-sidebar transition-colors cursor-pointer disabled:opacity-50 disabled:cursor-not-allowed shadow-xs shrink-0"
            >
              <RotateCw class="w-4 h-4 {isRefreshing ? 'animate-spin' : ''}" />
            </button>
          {/if}

          {#if (kind === "genre" || kind === "decade") && playlistId !== undefined}
            <!-- Auto-Play toggle: keep appending next batch as playback approaches end (#26) -->
            <button
              id="auto-play-toggle-{playlistId}"
              onclick={handleToggleAutoPlay}
              title={autoPlay
                ? i18n.t('playlists.autoPlayTooltipOn')
                : i18n.t('playlists.autoPlayTooltipOff')}
              class="flex items-center gap-2.5 px-4 py-2 rounded-full border text-xs font-semibold whitespace-nowrap shrink-0 transition-all duration-200 cursor-pointer select-none
                {autoPlay
                  ? 'bg-brand-accent/15 border-brand-accent text-brand-accent shadow-[0_0_12px_2px] shadow-brand-accent/25 hover:bg-brand-accent/25'
                  : 'border-brand-border text-brand-text-secondary/70 hover:text-brand-text-primary hover:bg-brand-sidebar'}"
            >
              <RefreshCw class="w-3.5 h-3.5 shrink-0 {autoPlay ? 'animate-spin [animation-duration:3s]' : ''}" />
              <span class="whitespace-nowrap">{i18n.t('playlists.autoPlayLabel')}</span>
              <!-- Toggle pill -->
              <span class="relative inline-flex items-center w-8 h-4 rounded-full shrink-0 transition-colors duration-200 {autoPlay ? 'bg-brand-accent' : 'bg-brand-border'}">
                <span class="absolute w-3 h-3 bg-white rounded-full shadow transition-transform duration-200 {autoPlay ? 'translate-x-4' : 'translate-x-0.5'}"></span>
              </span>
            </button>
          {/if}
        </div>


      </div>

      <!-- Right: Cover Stack -->
      <div class="relative w-40 h-40 hidden sm:block shrink-0">
        {#if topCovers.length > 0}
          <div class="w-full h-full bg-gradient-to-br {kind === 'decade' ? 'from-cyan-600 to-blue-600' : 'from-emerald-600 to-teal-600'} flex items-center justify-center overflow-hidden border border-brand-border/60 rounded-lg relative shadow-2xl">
            <CoverStack covers={topCovers} sizeClass="w-[82%] h-[82%]" />
          </div>
        {:else}
          <div class="absolute inset-0 overflow-hidden border border-brand-border/60 shadow-2xl bg-gradient-to-br {getArtistGradient(displayName)} flex items-center justify-center">
            <ListMusic class="w-16 h-16 text-white/80" />
          </div>
        {/if}
      </div>
    </div>
  </div>

  <div class="px-6 md:px-8 py-6" class:pb-24={!!playerStore.currentSong}>
    <div class="border border-brand-border/60 rounded-xl bg-brand-sidebar/30 backdrop-blur-md relative">
      <table class="w-full text-left text-sm border-collapse min-w-[800px]">
        <thead>
          <tr class="text-xs text-brand-text-secondary uppercase tracking-wider font-semibold">
            <th class="sticky top-0 bg-brand-sidebar border-b border-brand-border py-3 px-4 w-12 text-center z-10">
              <button onclick={() => toggleSort("track")} class="w-full flex items-center justify-center gap-1 hover:text-brand-text-primary transition-colors cursor-pointer uppercase tracking-wider font-semibold">
                {i18n.t('playlists.tableHeaderTrack')} {sortField === "track" || sortField === "default" ? (sortAsc ? "▲" : "▼") : ""}
              </button>
            </th>
            <th class="sticky top-0 bg-brand-sidebar border-b border-brand-border py-3 px-4 z-10">
              <button onclick={() => toggleSort("title")} class="flex items-center gap-1 hover:text-brand-text-primary transition-colors cursor-pointer uppercase tracking-wider font-semibold">
                {i18n.t('playlists.tableHeaderTitle')} {sortField === "title" ? (sortAsc ? "▲" : "▼") : ""}
              </button>
            </th>
            <th class="sticky top-0 bg-brand-sidebar border-b border-brand-border py-3 px-4 z-10">
              <button onclick={() => toggleSort("artist")} class="flex items-center gap-1 hover:text-brand-text-primary transition-colors cursor-pointer uppercase tracking-wider font-semibold">
                {i18n.t('playlists.tableHeaderArtist')} {sortField === "artist" ? (sortAsc ? "▲" : "▼") : ""}
              </button>
            </th>
            <th class="sticky top-0 bg-brand-sidebar border-b border-brand-border py-3 px-4 z-10">
              <button onclick={() => toggleSort("album")} class="flex items-center gap-1 hover:text-brand-text-primary transition-colors cursor-pointer uppercase tracking-wider font-semibold">
                {i18n.t('collection.tableHeaderAlbum')} {sortField === "album" ? (sortAsc ? "▲" : "▼") : ""}
              </button>
            </th>
            <th class="sticky top-0 bg-brand-sidebar border-b border-brand-border py-3 px-4 w-28 text-center z-10">
              <button onclick={() => toggleSort("rating")} class="w-full flex items-center justify-center gap-1 hover:text-brand-text-primary transition-colors cursor-pointer uppercase tracking-wider font-semibold">
                {i18n.t('collection.tableHeaderRating')} {sortField === "rating" ? (sortAsc ? "▲" : "▼") : ""}
              </button>
            </th>
            <th class="sticky top-0 bg-brand-sidebar border-b border-brand-border py-3 px-4 w-24 text-center z-10">
              <button onclick={() => toggleSort("duration")} class="w-full flex items-center justify-center gap-1 hover:text-brand-text-primary transition-colors cursor-pointer uppercase tracking-wider font-semibold">
                {i18n.t('playlists.tableHeaderDuration')} {sortField === "duration" ? (sortAsc ? "▲" : "▼") : ""}
              </button>
            </th>
            <th class="sticky top-0 bg-brand-sidebar border-b border-brand-border py-3 px-4 w-20 text-center z-10">{i18n.t('collection.tableHeaderActions')}</th>
          </tr>
        </thead>
        <tbody>
          {#if loading}
            <tr><td colspan="7" class="py-16 text-center">
              <div class="text-brand-text-secondary text-sm">{i18n.t('home.loading')}</div>
            </td></tr>
          {:else if sortedSongs.length === 0}
            <tr><td colspan="7" class="py-16 text-center select-none">
              <Music class="w-12 h-12 text-brand-accent-text/40 mb-3 mx-auto animate-pulse" />
              <h3 class="text-sm font-semibold text-brand-text-primary mb-1">{i18n.t('collection.noSongsTitle')}</h3>
            </td></tr>
          {:else}
            {#each sortedSongs as song, index (song.id)}
              <tr
                data-song-row="true"
                onclick={(e) => handleSongClick(e, song)}
                ondblclick={() => handlePlaySong(song)}
                oncontextmenu={(e) => handleContextMenu(e, song)}
                class="border-b border-brand-border/40 group transition-all duration-150 select-none cursor-pointer
                  {selectedSongIds.has(song.id) ? 'bg-brand-accent/20 text-brand-accent-text-hover' : (playerStore.currentSong && playerStore.currentSong.id === song.id ? 'bg-brand-accent/10 text-brand-accent-text-hover' : 'hover:bg-brand-sidebar/40')}"
              >
                <td class="py-2.5 px-4 text-center text-brand-text-secondary/50 font-medium w-12 relative">
                  <div class="relative w-5 h-4 mx-auto flex items-center justify-center">
                    {#if playerStore.currentSong && playerStore.currentSong.id === song.id && playerStore.state === 'playing'}
                      <div class="flex items-center justify-center gap-0.5 h-4 w-4 absolute inset-0 group-hover:opacity-0 transition-opacity">
                        <span class="w-0.5 bg-brand-accent animate-bounce h-full" style="animation-delay: 0.1s"></span>
                        <span class="w-0.5 bg-brand-accent animate-bounce h-2/3" style="animation-delay: 0.2s"></span>
                        <span class="w-0.5 bg-brand-accent animate-bounce h-full" style="animation-delay: 0.3s"></span>
                      </div>
                    {:else}
                      <span class="absolute inset-0 flex items-center justify-center group-hover:opacity-0 transition-opacity">
                        {song.track !== undefined && song.track !== null ? song.track : index + 1}
                      </span>
                    {/if}
                    <button
                      onclick={(e) => { e.stopPropagation(); handlePlaySong(song); }}
                      class="absolute inset-0 flex items-center justify-center opacity-0 group-hover:opacity-100 text-brand-accent-text hover:text-brand-accent-text-hover transition-opacity cursor-pointer"
                      title={i18n.t('collection.playSong')}
                    >
                      <Play class="w-4 h-4 fill-current" />
                    </button>
                  </div>
                </td>
                <td class="py-2.5 px-4 font-medium truncate max-w-xs {selectedSongIds.has(song.id) || (playerStore.currentSong && playerStore.currentSong.id === song.id) ? 'text-brand-accent-text-hover' : 'text-brand-text-primary'}">
                  <span class="truncate" title={song.title}>{song.title || i18n.t('collection.unknownSong')}</span>
                </td>
                <td class="py-2.5 px-4 text-brand-text-secondary/90 truncate max-w-xs">
                  {#if song.artist}
                    <span
                      role="button"
                      tabindex="0"
                      onclick={(e) => { e.stopPropagation(); collectionStore.viewArtist(song.album_artist?.trim() || song.artist || ""); }}
                      onkeydown={(e) => { if (e.key === "Enter" || e.key === " ") { e.stopPropagation(); collectionStore.viewArtist(song.album_artist?.trim() || song.artist || ""); } }}
                      class="hover:underline hover:text-brand-accent-text transition-all duration-150 text-left truncate cursor-pointer text-brand-text-secondary/90"
                      title={i18n.t('collection.filterByArtist', { artist: song.artist })}
                    >
                      {song.artist}
                    </span>
                  {:else}
                    <span class="text-brand-text-secondary/50">{i18n.t('collection.unknownArtist')}</span>
                  {/if}
                </td>
                <td class="py-2.5 px-4 text-brand-text-secondary/70 truncate max-w-xs">
                  {#if song.album}
                    <span
                      role="button"
                      tabindex="0"
                      onclick={(e) => { e.stopPropagation(); collectionStore.viewAlbum(song.album || ""); }}
                      onkeydown={(e) => { if (e.key === "Enter" || e.key === " ") { e.stopPropagation(); collectionStore.viewAlbum(song.album || ""); } }}
                      class="hover:underline hover:text-brand-accent-text transition-all duration-150 text-left truncate cursor-pointer text-brand-text-secondary/70"
                      title={i18n.t('collection.filterByAlbum', { album: song.album })}
                    >
                      {song.album}
                    </span>
                  {:else}
                    <span class="text-brand-text-secondary/50">{i18n.t('collection.unknownAlbum')}</span>
                  {/if}
                </td>
                <td class="py-2.5 px-4 text-center">
                  <div class="flex justify-center" onclick={(e) => e.stopPropagation()} role="presentation">
                    <SongRating rating={song.rating} onRate={(r) => rateSong(song, r)} />
                  </div>
                </td>
                <td class="py-2.5 px-4 text-center text-brand-text-secondary/80">{formatDuration(song.length_nanosec)}</td>
                <td class="py-2.5 px-4 text-center">
                  <div class="flex items-center justify-center gap-2.5">
                    <button
                      onclick={(e) => { e.stopPropagation(); handleAddSongToPlaylist(song.id); }}
                      class="text-brand-text-secondary/60 hover:text-brand-accent-text transition-colors cursor-pointer"
                      title={playlistsStore.activeCustomPlaylist
                        ? i18n.t('collection.addPlaylistTooltip', { name: playlistsStore.activeCustomPlaylist.name })
                        : i18n.t('collection.addPlaylistTooltipDefault')}
                    >
                      <Plus class="w-4 h-4" />
                    </button>
                    <button
                      onclick={(e) => { e.stopPropagation(); openTagEditor(song.id); }}
                      class="text-brand-text-secondary/60 hover:text-brand-accent-text transition-colors cursor-pointer"
                      title={i18n.t('collection.editTagsTooltip')}
                    >
                      <Edit3 class="w-4 h-4" />
                    </button>
                  </div>
                </td>
              </tr>
            {/each}
          {/if}
        </tbody>
      </table>
    </div>

    {#if autoPlay && (kind === "genre" || kind === "decade") && playlistId !== undefined && playerStore.isAutoPlayExhausted(playlistId)}
      <div class="mt-4 p-3 rounded-lg bg-brand-sidebar/60 border border-brand-border/40 text-center text-xs text-brand-text-secondary/80 flex items-center justify-center gap-2 select-none">
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
  <div data-floating-toolbar="true" class="absolute left-1/2 -translate-x-1/2 z-40 bg-brand-sidebar/95 border border-brand-border/80 shadow-2xl rounded-full px-5 py-2.5 flex items-center gap-4 text-xs font-semibold backdrop-blur-xl animate-in fade-in slide-in-from-bottom-4 duration-200" class:bottom-6={!playerStore.currentSong} class:bottom-28={!!playerStore.currentSong}>
    <span class="text-brand-accent-text font-bold">
      {i18n.t('playlists.selectedCount', { count: selectedSongIds.size })}
    </span>
    <div class="h-4 w-px bg-brand-border/60"></div>
    <button
      onclick={handlePlaySelected}
      class="flex items-center gap-1.5 hover:text-brand-accent-text transition-colors cursor-pointer"
    >
      <Play class="w-3.5 h-3.5 fill-current text-brand-accent-text" />
      <span>{i18n.t('playlists.playSelected')}</span>
    </button>
    <button
      onclick={handleBulkAddToPlaylist}
      class="flex items-center gap-1.5 hover:text-brand-accent-text transition-colors cursor-pointer"
    >
      <Plus class="w-3.5 h-3.5 text-brand-accent-text" />
      <span>
        {playlistsStore.activeCustomPlaylist
          ? i18n.t('playlists.contextMenuAddToPlaylist', { name: playlistsStore.activeCustomPlaylist.name })
          : i18n.t('playlists.contextMenuAddToPlaylistDefault')}
      </span>
    </button>
    <div class="h-4 w-px bg-brand-border/60"></div>
    <button
      onclick={() => { selectedSongIds = new Set(); }}
      class="text-brand-text-secondary hover:text-brand-text-primary transition-colors cursor-pointer"
    >
      {i18n.t('playlists.clearSelection')}
    </button>
  </div>
{/if}
