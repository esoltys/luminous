<script lang="ts">
  import { onMount } from "svelte";
  import { fade } from "svelte/transition";
  import { playlistsStore } from "../stores/playlists.svelte";
  import { pinnedStore } from "../stores/pinned.svelte";
  import { playerStore } from "../stores/player.svelte";
  import { collectionStore } from "../stores/collection.svelte";
  import { navigationStore } from "../stores/navigation.svelte";
  import { windowLayoutStore } from "../stores/windowLayout.svelte";
  import { shuffleArray } from "../utils/shuffle";
  import {
    Trash2,
    ListMusic,
    RotateCcw,
    RotateCw,
    AlertTriangle,
    Play,
    FolderInput,
    FileOutput,
    Pencil,
    Check,
    X,
    CopyPlus,
    Music,
    Shuffle,
    Search,
    Radio,
    Layers,
    MoreHorizontal,
    Eraser,
    Sparkles,
    FolderPlus,
    Pin,
    PinOff
  } from "lucide-svelte";
  import { resolveArtUrl } from "../types";
  import { i18n } from "../stores/i18n.svelte";
  import type { PlaylistItem, Song } from "../types";
  import { parseSearchRules, isSmartPlaylistSpec } from "../utils/filterParser";
  import { rememberScroll } from "../utils/scrollMemory";
  import { invoke } from "@tauri-apps/api/core";
  import { open, save } from "@tauri-apps/plugin-dialog";
  import TagEditor from "./TagEditor.svelte";
  import { tagsStore } from "../stores/tags.svelte";
  import CoverArt from "./CoverArt.svelte";
  import CoverStack from "./CoverStack.svelte";
  import PlaylistContextMenu from "./PlaylistContextMenu.svelte";
  import ConfirmDialog from "./ConfirmDialog.svelte";
  import Modal from "./Modal.svelte";
  import ColumnSelector from "./ColumnSelector.svelte";
  import Button from "./Button.svelte";
  import Input from "./Input.svelte";
  import IconActionButton from "./IconActionButton.svelte";
  import ContextMenu from "./ContextMenu.svelte";
  import ContextMenuItem from "./ContextMenuItem.svelte";
  import ContextMenuDivider from "./ContextMenuDivider.svelte";
  import { portal } from "../utils/portal";
  import { formatSampleRate, formatBitDepth, formatChannels, formatFileSize, formatDate, formatDuration } from "../utils/formatters";
  import { formatDateAdded } from "../utils/date";
  import { CONTEXT_MENU_WIDTH_PX } from "../constants";
  import { compareSongs } from "../utils/songSort";
  import SongTable, { type SongTableRow } from "./SongTable.svelte";

  // Default column widths (px or fr) — used when no saved width exists for a column.
  const PLAYLIST_COL_DEFAULTS: Partial<Record<keyof typeof collectionStore.visibleColumns, string>> = {
    title: "2fr", artist: "1.5fr", album: "1.5fr",
    composer: "1.5fr", album_artist: "1.5fr", format: "64px", year: "60px", originalyear: "60px",
    genre: "1.2fr", grouping: "1.2fr", bpm: "60px", initial_key: "60px",
    bitrate: "70px", samplerate: "75px", bitdepth: "65px", channels: "70px",
    filesize: "75px", rating: "96px", playcount: "70px", skipcount: "70px",
    lastplayed: "90px", added: "90px", duration: "80px", path: "2fr", actions: "80px",
  };
  import {
    COVER_STACK_OFFSET_X_PX,
    COVER_STACK_OFFSET_Y_PX,
    COVER_STACK_ROTATION_DEG,
    COVER_STACK_SCALE_STEP,
    COVER_STACK_OPACITY_STEP,
  } from "../constants";

  let editingSongId = $state<number | null>(null);

  let isEditingTitle = $state(false);
  let editTitleValue = $state("");

  let filterQuery = $state("");

  let selectedUuids = $state<Set<string>>(new Set());

  let contextMenuState = $state<{ x: number; y: number; item: PlaylistItem } | null>(null);

  // Overflow ("more actions") menu state — houses the lower-frequency playlist
  // actions (import/export/duplicate-cleanup/clear/delete) so the toolbar
  // isn't a wall of always-visible buttons.
  let showOverflowMenu = $state(false);
  let overflowMenuPos = $state<{ x: number; y: number } | null>(null);
  let overflowButtonEl = $state<HTMLButtonElement | undefined>(undefined);

  let showSaveQueueModal = $state(false);
  let saveQueueName = $state("Queue Playlist");

  function toggleOverflowMenu() {
    if (showOverflowMenu) {
      showOverflowMenu = false;
      return;
    }
    if (!overflowButtonEl) return;
    const rect = overflowButtonEl.getBoundingClientRect();
    overflowMenuPos = { x: rect.right - CONTEXT_MENU_WIDTH_PX, y: rect.bottom + 8 };
    showOverflowMenu = true;
  }

  // Focuses and selects an input's text on mount, without the a11y-flagged
  // `autofocus` attribute (the rename input only appears after an explicit
  // user action — double-click or the rename button — so this isn't a
  // page-load autofocus).
  function focusAndSelect(node: HTMLInputElement) {
    node.focus();
    node.select();
  }

  function startRename() {
    if (activePlaylist && !isQueue) {
      editTitleValue = activePlaylist.name;
      isEditingTitle = true;
    }
  }

  async function saveRename() {
    if (!isEditingTitle) return;
    if (activePlaylist && editTitleValue.trim() !== "" && editTitleValue.trim() !== activePlaylist.name) {
      try {
        await playlistsStore.renamePlaylist(activePlaylist.id, editTitleValue.trim());
      } catch (err) {
        console.error("Failed to rename playlist:", err);
        return;
      }
    }
    isEditingTitle = false;
  }

  function cancelRename() {
    isEditingTitle = false;
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
      }
    } catch (err) {
      console.error("Failed to import playlist:", err);
    }
  }

  let showExportOptionsModal = $state(false);
  let exportRelative = $state(true);

  async function triggerExport() {
    if (!activePlaylist) return;
    try {
      const savePath = await save({
        title: i18n.t("playlists.exportPlaylistTooltip"),
        defaultPath: `${activePlaylist.name}.m3u8`,
        filters: [
          { name: "M3U8 Playlist (*.m3u8)", extensions: ["m3u8"] },
          { name: "M3U Playlist (*.m3u)", extensions: ["m3u"] },
          { name: "PLS Playlist (*.pls)", extensions: ["pls"] },
          { name: "XSPF Playlist (*.xspf)", extensions: ["xspf"] },
        ],
      });
      if (savePath && typeof savePath === "string") {
        await playlistsStore.exportPlaylist(activePlaylist.id, savePath, exportRelative);
        showExportOptionsModal = false;
      }
    } catch (err) {
      console.error("Failed to export playlist:", err);
    }
  }

  function openTagEditor(songId: number) {
    editingSongId = songId;
  }

  function handleTagEditorSaved() {
    tagsStore.load();
    if (playlistsStore.activePlaylistId !== null) {
      playlistsStore.selectPlaylist(playlistsStore.activePlaylistId);
    }
  }

  let activePlaylist = $derived(
    playlistsStore.playlists.find((p) => p.id === playlistsStore.activePlaylistId)
  );

  let isActive = $derived(
    activePlaylist !== undefined && playlistsStore.effectivePinnedPlaylistId === activePlaylist.id
  );

  let isQueue = $derived(
    activePlaylist !== undefined && activePlaylist.is_queue
  );

  let isSpecialPlaylist = $derived(
    activePlaylist !== undefined &&
      (isQueue ||
       activePlaylist.is_queue ||
       activePlaylist.name.toLowerCase() === "history" ||
       activePlaylist.name.toLowerCase() === "favourites" ||
       activePlaylist.name.toLowerCase() === "recently added")
  );

  let isSmartPlaylist = $derived(
    activePlaylist !== undefined &&
      activePlaylist.dynamic_enabled &&
      isSmartPlaylistSpec(activePlaylist.dynamic_spec)
  );

  function handleEditSmartPlaylist() {
    if (!activePlaylist) return;
    const rules = parseSearchRules((activePlaylist.dynamic_spec ?? "").replace(/;/g, " "));
    collectionStore.openSmartBuilder(rules, {
      id: activePlaylist.id,
      name: activePlaylist.name,
      populationMode: activePlaylist.population_mode ?? "all",
    });
  }

  type PlaylistSortField = "position" | keyof Song;
  let sortField = $state<PlaylistSortField>("position");
  let sortAsc = $state(true);

  function toggleSort(field: string) {
    const f = field as PlaylistSortField;
    if (sortField === f) {
      sortAsc = !sortAsc;
    } else {
      sortField = f;
      sortAsc = true;
    }
  }

  // Build the searchable text for a song from whichever columns are currently
  // visible, so the filter reflects what's actually shown in the table rather
  // than a fixed title/artist/album set.
  function searchableColumnValues(song: Song | undefined): string[] {
    if (!song) return [];
    const vc = collectionStore.visibleColumns;
    const values: (string | number | undefined | null)[] = [];
    if (vc.title) values.push(song.title);
    if (vc.artist) values.push(song.artist);
    if (vc.album) values.push(song.album);
    if (vc.composer) values.push(song.composer);
    if (vc.album_artist) values.push(song.album_artist);
    if (vc.format) values.push(song.filetype);
    if (vc.year) values.push(song.year);
    if (vc.genre) values.push(song.genre);
    if (vc.grouping) values.push(song.grouping);
    if (vc.bpm) values.push(song.bpm);
    if (vc.initial_key) values.push(song.initial_key);
    if (vc.bitrate) values.push(song.bitrate);
    if (vc.samplerate) values.push(formatSampleRate(song.samplerate));
    if (vc.bitdepth) values.push(formatBitDepth(song.bitdepth));
    if (vc.channels) values.push(formatChannels(song.channels));
    if (vc.filesize) values.push(formatFileSize(song.filesize));
    if (vc.rating) values.push(song.rating);
    if (vc.playcount) values.push(song.playcount);
    if (vc.skipcount) values.push(song.skipcount);
    if (vc.lastplayed) values.push(formatDate(song.lastplayed));
    if (vc.added) values.push(formatDateAdded(song.added));
    if (vc.duration) values.push(formatDuration(song.length_nanosec));
    if (vc.path) values.push(song.path);
    return values
      .filter((v) => v !== undefined && v !== null && v !== "")
      .map((v) => String(v).toLowerCase());
  }

  let filteredTracks = $derived.by(() => {
    const q = filterQuery.trim().toLowerCase();
    let result = playlistsStore.activePlaylistTracks;
    if (q) {
      result = result.filter((item) => searchableColumnValues(item.song).some((v) => v.includes(q)));
    }

    if (sortField === "position") {
      return sortAsc ? result : [...result].reverse();
    }

    const field = sortField as keyof Song;
    return [...result].sort((a, b) => {
      if (!a.song && !b.song) return 0;
      if (!a.song) return sortAsc ? 1 : -1;
      if (!b.song) return sortAsc ? -1 : 1;
      return compareSongs(a.song, b.song, field, sortAsc);
    });
  });

  // Top distinct album covers sampled for hero 3D card stack
  let topAlbums = $derived.by(() => {
    const seen = new Set<string>();
    const list: Array<{ songId?: number; artEmbedded?: boolean; artAutomatic?: string | null; artManual?: string | null }> = [];
    for (const item of playlistsStore.activePlaylistTracks) {
      if (!item.song) continue;
      const s = item.song;
      const key = s.art_manual || s.art_automatic || (s.art_embedded ? `embed-${s.id}` : null);
      if (key && !seen.has(key)) {
        seen.add(key);
        list.push({
          songId: s.id,
          artEmbedded: s.art_embedded,
          artAutomatic: s.art_automatic,
          artManual: s.art_manual,
        });
        if (list.length >= 6) break;
      }
    }
    return list;
  });

  let totalRuntimeLabel = $derived.by(() => {
    const totalNs = playlistsStore.activePlaylistTracks.reduce(
      (sum, item) => sum + (item.song?.length_nanosec ?? 0),
      0
    );
    if (!totalNs) return "0m";
    const totalSec = Math.floor(totalNs / 1_000_000_000);
    const m = Math.floor(totalSec / 60);
    const h = Math.floor(m / 60);
    const remM = m % 60;
    return h > 0 ? `${h}h ${remM}m` : `${m}m`;
  });

  let genreSummaryLabel = $derived.by(() => {
    const counts = new Map<string, number>();
    for (const item of playlistsStore.activePlaylistTracks) {
      const g = (item.song?.genre ?? "").trim();
      if (g !== "") counts.set(g, (counts.get(g) ?? 0) + 1);
    }
    if (counts.size === 0) return "";
    if (counts.size > 2) return i18n.t("playlists.mixedGenre", {}, "Mixed");
    const top = [...counts.entries()]
      .sort((a, b) => b[1] - a[1])
      .map(([g]) => g);
    return top.slice(0, 2).join(" / ");
  });

  let duplicateUuids = $derived.by(() => {
    const seen = new Set<string>();
    const dupes: string[] = [];
    for (const item of playlistsStore.activePlaylistTracks) {
      let key = "";
      if (item.song?.id) {
        key = `song-${item.song.id}`;
      } else if (item.song?.title && item.song?.artist) {
        key = `meta-${item.song.title.toLowerCase().trim()}-${item.song.artist.toLowerCase().trim()}`;
      } else if (item.url) {
        key = `url-${item.url}`;
      } else {
        key = `uuid-${item.uuid}`;
      }

      if (seen.has(key)) {
        dupes.push(item.uuid);
      } else {
        seen.add(key);
      }
    }
    return dupes;
  });

  let duplicateCount = $derived(duplicateUuids.length);

  function itemToRow(item: PlaylistItem): SongTableRow {
    const trueUnavailable = isItemUnavailable(item);
    const disconnected = !trueUnavailable && collectionStore.isPathOnDisconnectedDrive(item.song?.path);
    return {
      key: item.uuid,
      song: item.song,
      disabled: trueUnavailable || disconnected,
      disabledTooltip: trueUnavailable
        ? i18n.t("playlists.fileNotFoundTooltip")
        : disconnected
          ? i18n.t("collection.driveDisconnectedTooltip")
          : undefined,
      disabledVariant: trueUnavailable ? "strikethrough" : "dim",
      isDuplicate: duplicateUuids.includes(item.uuid),
      underlyingIndex: playlistsStore.activePlaylistTracks.findIndex((t) => t.uuid === item.uuid),
    };
  }

  let tableRows = $derived(filteredTracks.map(itemToRow));
  // Range (shift-click) selection and drag-target resolution both operate on
  // the underlying (unfiltered) playlist order — preserving the existing
  // behavior from before this table was consolidated.
  let rangeSelectionRows = $derived(playlistsStore.activePlaylistTracks.map(itemToRow));

  function removeDuplicates() {
    if (activePlaylist && duplicateCount > 0) {
      playlistsStore.deduplicatePlaylist(activePlaylist.id);
    }
  }

  let showDeleteConfirm = $state(false);

  async function handleDeletePlaylist() {
    if (!activePlaylist || isQueue) return;
    showDeleteConfirm = false;
    await playlistsStore.deletePlaylist(activePlaylist.id);
    navigationStore.selectedPlaylistId = null;
  }

  /** Plays the clicked row the same way `playSelected`/`handlePlayAll` do:
   * resolve songIds + a start index entirely from in-memory reactive state
   * and hand the whole batch to `playSongs` in one shot. Earlier this went
   * through a uuid lookup (`playPlaylistItemByUuid`) resolved fresh on the
   * backend, which turned out to still occasionally fail to find a uuid
   * that demonstrably existed in the DB a moment later — an unresolved
   * server-side race. Since there's nothing to race against when the whole
   * request is built synchronously from state already in hand, this path
   * sidesteps it rather than chasing it further. */
  function handlePlayPlaylistItem(item: PlaylistItem) {
    if (!item || isItemUnavailable(item) || !activePlaylist) return;
    playerStore.playPlaylistItemByUuid(activePlaylist.id, item.uuid);
  }

  /** Returns true if the item's song is missing from disk or has no song data. */
  function isItemUnavailable(item: PlaylistItem): boolean {
    return !item.song || item.song.unavailable === true;
  }

  /** Remove all playlist items whose song is unavailable. */
  function removeUnavailableTracks() {
    if (playlistsStore.activePlaylistId === null) return;
    const uuids = playlistsStore.activePlaylistTracks
      .filter((item) => isItemUnavailable(item))
      .map((item) => item.uuid);
    if (uuids.length > 0) {
      playlistsStore.removeItemsFromPlaylist(playlistsStore.activePlaylistId, uuids);
    }
  }

  /** Count of unavailable tracks in the active playlist. */
  let unavailableCount = $derived(
    playlistsStore.activePlaylistTracks.filter((item) => isItemUnavailable(item)).length
  );

  function handleRemoveItem(uuid: string) {
    if (playlistsStore.activePlaylistId !== null) {
      playlistsStore.removeItemsFromPlaylist(playlistsStore.activePlaylistId, [uuid]);
    }
  }

  async function rateSong(song: Song, rating: number) {
    song.rating = await invoke<number>("set_song_rating", { songId: song.id, rating });
  }

  let trackByUuid = $derived(new Map(playlistsStore.activePlaylistTracks.map((t) => [t.uuid, t])));

  function handleRowContextMenu(event: MouseEvent, row: SongTableRow) {
    const item = trackByUuid.get(row.key);
    if (item) contextMenuState = { x: event.clientX, y: event.clientY, item };
  }

  function handleDeleteKey(event: KeyboardEvent) {
    const target = event.target as HTMLElement | null;
    if (target && (target.tagName === "INPUT" || target.tagName === "TEXTAREA")) return;
    if (event.key === "Delete" || event.key === "Backspace") {
      if (selectedUuids.size > 0 && activePlaylist) {
        event.preventDefault();
        playlistsStore.removeItemsFromPlaylist(activePlaylist.id, Array.from(selectedUuids));
        selectedUuids = new Set();
      }
    }
  }

  function handleReorder(fromIndex: number, toIndex: number, selectedRowKeys: string[]) {
    if (!activePlaylist) return;
    const targetTrack = playlistsStore.activePlaylistTracks[toIndex];
    if (!targetTrack) return;

    if (selectedRowKeys.length > 1) {
      const selectedIndices = playlistsStore.activePlaylistTracks
        .map((t, idx) => ({ uuid: t.uuid, idx }))
        .filter((entry) => selectedRowKeys.includes(entry.uuid))
        .map((entry) => entry.idx);

      if (selectedIndices.length > 0) {
        playlistsStore.reorderItemsBatch(activePlaylist.id, selectedIndices, toIndex);
      }
    } else {
      const sourceTrack = playlistsStore.activePlaylistTracks[fromIndex];
      if (sourceTrack && sourceTrack.uuid !== targetTrack.uuid) {
        playlistsStore.reorderItemByUuid(activePlaylist.id, sourceTrack.uuid, targetTrack.uuid);
      }
    }
  }

  function playSelected() {
    if (selectedUuids.size === 0 || !activePlaylist) return;
    const selectedTracks = playlistsStore.activePlaylistTracks.filter(
      (t) => selectedUuids.has(t.uuid) && t.song && !isItemUnavailable(t)
    );
    const songIds = selectedTracks.map((t) => t.song!.id);
    if (songIds.length > 0) {
      playerStore.playSongs(songIds, 0, activePlaylist.id);
    }
  }

  function removeSelected() {
    if (selectedUuids.size > 0 && activePlaylist) {
      playlistsStore.removeItemsFromPlaylist(activePlaylist.id, Array.from(selectedUuids));
      selectedUuids = new Set();
    }
  }

  async function handlePlayAll() {
    if (!activePlaylist || playlistsStore.activePlaylistTracks.length === 0) return;
    const availableTracks = playlistsStore.activePlaylistTracks.filter((t) => t.song && !isItemUnavailable(t));
    if (availableTracks.length === 0) return;
    const songIds = availableTracks.map((t) => t.song!.id);
    const queuePl = await playlistsStore.requireQueue();
    await playerStore.setShuffleMode("off");
    await playerStore.playSongs(songIds, 0, queuePl?.id, undefined, "Queue");
    if (queuePl) {
      playlistsStore.selectPlaylist(queuePl.id);
      navigationStore.viewPlaylist(queuePl.id);
    }
  }

  async function handleShufflePlay() {
    if (!activePlaylist || playlistsStore.activePlaylistTracks.length === 0) return;
    const availableTracks = playlistsStore.activePlaylistTracks.filter((t) => t.song && !isItemUnavailable(t));
    if (availableTracks.length === 0) return;
    const shuffledSongIds = shuffleArray(availableTracks.map((t) => t.song!.id));
    const queuePl = await playlistsStore.requireQueue();
    await playerStore.setShuffleMode("off");
    await playerStore.playSongs(shuffledSongIds, 0, queuePl?.id, undefined, "Queue");
    if (queuePl) {
      playlistsStore.selectPlaylist(queuePl.id);
      navigationStore.viewPlaylist(queuePl.id);
    }
  }

  function handleSaveQueueAsCustomPlaylist() {
    if (playlistsStore.activePlaylistTracks.length === 0) return;
    const songIds = playlistsStore.activePlaylistTracks.filter((t) => t.song).map((t) => t.song!.id);
    if (songIds.length === 0) return;

    saveQueueName = `Queue Playlist`;
    showSaveQueueModal = true;
  }

  async function confirmSaveQueueAsCustomPlaylist() {
    if (!saveQueueName || !saveQueueName.trim()) return;
    const songIds = playlistsStore.activePlaylistTracks.filter((t) => t.song).map((t) => t.song!.id);
    if (songIds.length === 0) return;

    try {
      const created = await playlistsStore.createPlaylist(saveQueueName.trim());
      await playlistsStore.addSongsToPlaylist(created.id, songIds);
      playlistsStore.selectPlaylist(created.id);
      navigationStore.viewPlaylist(created.id);
      showSaveQueueModal = false;
    } catch (err) {
      console.error("Failed to save Queue as custom playlist:", err);
    }
  }

  let currentCoverUrl = $derived.by(() => {
    const song = playerStore.currentSong;
    if (!song) return null;
    if (song.art_manual) {
      return resolveArtUrl(song.art_manual);
    }
    if (song.art_automatic) {
      return resolveArtUrl(song.art_automatic);
    }
    return null;
  });
</script>

{#snippet playlistEmptyState()}
  <div class="py-12 text-center text-brand-text-primary/45 select-none">
    <ListMusic class="w-12 h-12 mx-auto mb-2 text-brand-text-primary/30" />
    {#if filterQuery}
      {i18n.t("playlists.noFilterResults", { query: filterQuery })}
    {:else if isQueue}
      {i18n.t("playlists.emptyQueueText")}
    {:else}
      {i18n.t("playlists.emptyPlaylistTitle")}
    {/if}
  </div>
{/snippet}

<svelte:window onkeydown={handleDeleteKey} />

<div class="flex-1 flex flex-col overflow-hidden bg-brand-main text-brand-text-secondary h-full relative select-none">
  {#if currentCoverUrl && isQueue}
    <div class="absolute inset-0 pointer-events-none select-none z-0 overflow-hidden">
      {#key currentCoverUrl}
        <img
          src={currentCoverUrl}
          alt=""
          class="absolute inset-0 w-full h-full object-cover blur-2xl"
          style="will-change: filter; transform: translateZ(0) scale(1.5);"
          in:fade={{ duration: 400 }}
        />
      {/key}
      <div class="absolute inset-0 bg-gradient-to-b from-transparent via-transparent to-brand-main"></div>
    </div>
  {/if}

  {#if activePlaylist}
    <div
      class="flex-1 flex flex-col min-h-0 relative z-10 overflow-y-auto carousel-scroll"
      use:rememberScroll={`playlist:${playlistsStore.activePlaylistId}`}
    >
    <div class="relative z-30 w-full overflow-hidden border-b border-brand-border/60 bg-brand-main/60 backdrop-blur-md px-6 {windowLayoutStore.isDetailHeaderCollapsed ? 'py-3' : 'pt-6 pb-6'} shrink-0">
      <div class="flex items-stretch justify-between gap-6 relative z-10">
        <div class="flex flex-col justify-end gap-1.5 min-w-0 flex-1">
          {#if !windowLayoutStore.isDetailHeaderCollapsed}
          {#if isEditingTitle}
            <div class="flex items-center gap-2">
              <input
                bind:value={editTitleValue}
                onkeydown={(e) => { if (e.key === "Enter") saveRename(); else if (e.key === "Escape") cancelRename(); }}
                class="bg-brand-sidebar border border-brand-accent text-brand-text-primary px-3 py-1 text-2xl font-bold rounded-lg focus:outline-none"
                use:focusAndSelect
              />
              <button onclick={saveRename} class="p-1.5 text-brand-accent-text hover:text-brand-accent" title={i18n.t('playlists.saveRenameTooltip')}>
                <Check class="w-5 h-5" />
              </button>
              <button onclick={cancelRename} class="p-1.5 text-brand-text-secondary hover:text-brand-text-primary" title={i18n.t('playlists.cancel')}>
                <X class="w-5 h-5" />
              </button>
            </div>
          {:else}
            <div class="flex items-center gap-3 group/title">
              <h1
                ondblclick={isQueue ? undefined : startRename}
                class="text-3xl sm:text-4xl font-heading font-bold text-brand-text-primary transition-colors truncate py-0.5 leading-snug {isQueue ? '' : 'hover:text-brand-accent-text'}"
                title={isQueue ? undefined : i18n.t("playlists.renamePlaylistTooltip")}
              >
                {activePlaylist.name}
              </h1>
              {#if !isQueue}
                <button
                  onclick={startRename}
                  class="opacity-0 group-hover/title:opacity-100 text-brand-text-secondary hover:text-brand-text-primary transition-opacity p-1"
                  title={i18n.t("playlists.renamePlaylistTooltip")}
                >
                  <Pencil class="w-4 h-4" />
                </button>
              {/if}
            </div>
          {/if}

          <div class="flex items-center gap-3 text-xs text-brand-text-secondary font-medium">
            <span>
              {#if isSpecialPlaylist}
                {playlistsStore.activePlaylistTracks.length === 1
                  ? i18n.t("playlists.oneSong")
                  : i18n.t("playlists.songsCount", { count: playlistsStore.activePlaylistTracks.length })}
                • {totalRuntimeLabel}
              {:else}
                {i18n.t("playlists.statsLine", {
                  genre: genreSummaryLabel || i18n.t("playlists.unknownGenre"),
                  songs: playlistsStore.activePlaylistTracks.length === 1
                    ? i18n.t("playlists.oneSong")
                    : i18n.t("playlists.songsCount", { count: playlistsStore.activePlaylistTracks.length }),
                  duration: totalRuntimeLabel,
                })}
              {/if}
            </span>
          </div>
          {/if}

          <div class="flex flex-wrap items-center gap-3 mt-3">
            <Button
              onclick={handlePlayAll}
              disabled={playlistsStore.activePlaylistTracks.length === 0}
              variant="primary"
            >
              <Play class="w-4 h-4 fill-current" /> {i18n.t("artistDetail.playAll")}
            </Button>
            <Button
              onclick={handleShufflePlay}
              disabled={playlistsStore.activePlaylistTracks.length === 0}
              variant="secondary"
            >
              <Shuffle class="w-4 h-4" /> {i18n.t("artistDetail.shuffleAndPlay")}
            </Button>
            {#if isQueue}
              <IconActionButton
                onclick={handleSaveQueueAsCustomPlaylist}
                disabled={playlistsStore.activePlaylistTracks.length === 0}
                title={i18n.t("playlists.saveQueueAsPlaylist", {}, "Save as Custom Playlist")}
                class="shrink-0"
              >
                {#snippet icon()}<FolderPlus class="w-4 h-4" />{/snippet}
              </IconActionButton>
            {/if}
            {#if isSmartPlaylist}
              <Button onclick={handleEditSmartPlaylist} variant="secondary" title={i18n.t("playlists.editSmartPlaylistBtn")}>
                <Pencil class="w-4 h-4" />
                <span>{i18n.t("playlists.editSmartPlaylistBtn")}</span>
              </Button>
            {/if}
            {#if !isActive && activePlaylist && !activePlaylist.dynamic_enabled}
              <Button
                onclick={() => playlistsStore.pinPlaylist(activePlaylist.id)}
                variant="accent-soft"
                title={i18n.t("playlists.makeActiveBtn")}
              >
                <Radio class="w-4 h-4" />
                <span>{i18n.t("playlists.makeActiveBtn")}</span>
              </Button>
            {/if}
            {#if !isQueue && activePlaylist}
              <IconActionButton
                onclick={() => pinnedStore.toggle("playlist", String(activePlaylist!.id))}
                title={pinnedStore.isPinned("playlist", String(activePlaylist.id))
                  ? i18n.t("playlists.contextMenuUnpinHome")
                  : i18n.t("playlists.contextMenuPinHome")}
                class="shrink-0"
              >
                {#snippet icon()}
                  {#if pinnedStore.isPinned("playlist", String(activePlaylist.id))}
                    <PinOff class="w-4 h-4" />
                  {:else}
                    <Pin class="w-4 h-4" />
                  {/if}
                {/snippet}
              </IconActionButton>
            {/if}
            <ColumnSelector align="left" iconOnly />
          </div>

          {#if !windowLayoutStore.isDetailHeaderCollapsed}
          <div class="flex flex-wrap items-center gap-2.5 mt-2.5 select-none">
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
                bind:this={overflowButtonEl}
                onclick={toggleOverflowMenu}
                title={i18n.t("playlists.moreActionsTooltip")}
                class="flex items-center justify-center w-10 h-10 rounded-full border border-brand-border text-brand-text-secondary hover:text-brand-accent-text hover:bg-brand-sidebar transition-colors shadow-xs"
              >
                <MoreHorizontal class="w-4 h-4" />
              </button>
            </div>
          </div>
          {/if}
        </div>

        {#if !windowLayoutStore.isDetailHeaderCollapsed}
        {#if isQueue}
          <div class="w-40 h-40 hidden sm:flex shrink-0 bg-brand-main bg-gradient-to-br from-brand-accent/25 to-brand-accent/15 items-center justify-center overflow-hidden border border-brand-accent/30 shadow-[0_0_28px_3px] shadow-brand-accent/40">
            {#key playerStore.currentSong?.id}
              <div class="w-full h-full" in:fade={{ duration: 200 }}>
                {#if playerStore.currentSong}
                  <CoverArt
                    songId={playerStore.currentSong.id}
                    artEmbedded={playerStore.currentSong.art_embedded}
                    artAutomatic={playerStore.currentSong.art_automatic}
                    artManual={playerStore.currentSong.art_manual}
                    sizeClass="w-full h-full"
                  />
                {:else}
                  <div class="w-full h-full flex items-center justify-center">
                    <Layers class="w-16 h-16 text-brand-accent-text" />
                  </div>
                {/if}
              </div>
            {/key}
          </div>
        {:else if isSmartPlaylist && topAlbums.length > 0}
          <div class="w-40 h-40 hidden sm:flex shrink-0 bg-brand-main bg-gradient-to-br from-[#C2410C]/25 to-[#F59E0B]/15 items-center justify-center overflow-hidden border border-[#F59E0B]/30 shadow-[0_0_28px_3px_rgba(245,158,11,0.4)]">
            <CoverStack covers={topAlbums} sizeClass="w-[82%] h-[82%]" />
          </div>
        {:else if isSmartPlaylist}
          <div class="w-40 h-40 hidden sm:flex shrink-0 bg-brand-main bg-gradient-to-br from-[#C2410C]/25 to-[#F59E0B]/15 items-center justify-center overflow-hidden border border-[#F59E0B]/30 shadow-[0_0_28px_3px_rgba(245,158,11,0.4)]">
            <Sparkles class="w-16 h-16 text-[#F59E0B]" />
          </div>
        {:else if topAlbums.length > 0}
          <div class="relative self-stretch w-48 hidden sm:block shrink-0">
            {#each topAlbums.slice(0, 6) as album, i (i)}
              <div
                class="absolute bottom-0 right-0 w-32 h-32 overflow-hidden border border-brand-border/60 shadow-xl transition-all duration-300"
                style="z-index: {10 - i}; transform: translate({i * COVER_STACK_OFFSET_X_PX}px, {i * COVER_STACK_OFFSET_Y_PX}px) rotate({i * COVER_STACK_ROTATION_DEG}deg) scale({1 - i * COVER_STACK_SCALE_STEP}); opacity: {1 - i * COVER_STACK_OPACITY_STEP};"
              >
                <CoverArt
                  songId={album.songId}
                  artEmbedded={album.artEmbedded}
                  artAutomatic={album.artAutomatic}
                  artManual={album.artManual}
                  sizeClass="w-full h-full"
                />
              </div>
            {/each}
          </div>
        {/if}
        {/if}
      </div>
    </div>

    <div class="p-6 flex flex-col" class:pb-28={!!playerStore.currentSong}>
      <div class="border border-brand-border/60 rounded-xl bg-brand-sidebar/30 backdrop-blur-md relative overflow-hidden table-surface-blur">
        <SongTable
          rows={tableRows}
          rangeSelectionOrder={rangeSelectionRows}
          mode="position"
          leadingColumnWidth="48px"
          colDefaults={PLAYLIST_COL_DEFAULTS}
          {sortField}
          {sortAsc}
          onToggleSort={toggleSort}
          positionSortField="position"
          bind:selectedKeys={selectedUuids}
          emptyState={playlistEmptyState}
          onRowDoubleClick={(row) => activePlaylist && playerStore.playPlaylistItemByUuid(activePlaylist.id, row.key)}
          onRowContextMenu={handleRowContextMenu}
          onRate={rateSong}
          onEditTags={(song) => openTagEditor(song.id)}
          onRemoveFromPlaylist={(row) => handleRemoveItem(row.key)}
          onReorder={handleReorder}
          isRowPlaying={(row) => (!!playerStore.playlistItemUuid && playerStore.playlistItemUuid === row.key) || (!!playerStore.currentSong && !!row.song && playerStore.currentSong.id === row.song.id)}
          interactiveWhenDisabled
          disabledPlaceholder
        />
      </div>
    </div>
  </div>

    {#if selectedUuids.size > 0}
      <div data-floating-toolbar="true" class="absolute left-1/2 -translate-x-1/2 z-40 bg-brand-sidebar/95 border border-brand-border/80 shadow-2xl rounded-full px-5 py-2.5 flex items-center gap-4 text-xs font-semibold backdrop-blur-xl animate-in fade-in slide-in-from-bottom-4 duration-200" class:bottom-6={!playerStore.currentSong} class:bottom-28={!!playerStore.currentSong}>
        <span class="text-brand-accent-text font-bold">
          {i18n.t("playlists.selectedCount", { count: selectedUuids.size })}
        </span>
        <div class="h-4 w-px bg-brand-border/60"></div>
        <button
          onclick={playSelected}
          class="flex items-center gap-1.5 hover:text-brand-accent-text transition-colors"
        >
          <Play class="w-3.5 h-3.5 fill-current text-brand-accent-text" />
          <span>{i18n.t("playlists.playSelected")}</span>
        </button>
        <button
          onclick={removeSelected}
          class="flex items-center gap-1.5 text-red-400 hover:text-red-300 transition-colors"
        >
          <Trash2 class="w-3.5 h-3.5" />
          <span>{i18n.t("playlists.removeSelected")}</span>
        </button>
        <div class="h-4 w-px bg-brand-border/60"></div>
        <button
          onclick={() => { selectedUuids = new Set(); }}
          class="text-brand-text-primary hover:text-brand-text-primary transition-colors"
        >
          {i18n.t("playlists.clearSelection")}
        </button>
      </div>
    {/if}
  {:else}
    <div class="flex-1 flex flex-col items-center justify-center text-brand-text-primary/60">
      <ListMusic class="w-16 h-16 mb-4 text-brand-text-primary/30" />
      <h2 class="text-lg font-bold text-brand-text-primary/80 mb-1">{i18n.t("playlists.noPlaylistsTitle")}</h2>
      <p class="text-sm">{i18n.t("playlists.noPlaylistsText")}</p>
    </div>
  {/if}
</div>

{#if contextMenuState}
  {@const singleItem = contextMenuState.item}
  <PlaylistContextMenu
    x={contextMenuState.x}
    y={contextMenuState.y}
    selectedCount={selectedUuids.size}
    onPlay={playSelected}
    onRemove={removeSelected}
    onGoToArtist={singleItem.song?.artist ? () => navigationStore.viewArtist(singleItem.song?.album_artist?.trim() || singleItem.song?.artist || "") : undefined}
    onGoToAlbum={singleItem.song?.album ? () => navigationStore.viewAlbum(singleItem.song?.album || "") : undefined}
    onEditTags={singleItem.song?.id && !isItemUnavailable(singleItem) ? () => openTagEditor(singleItem.song!.id) : undefined}
    onClose={() => { contextMenuState = null; }}
  />
{/if}

{#if showOverflowMenu && overflowMenuPos && activePlaylist}
  <ContextMenu
    x={overflowMenuPos.x}
    y={overflowMenuPos.y}
    estimatedHeight={280}
    onClose={() => { showOverflowMenu = false; }}
  >
    <ContextMenuItem
      icon={FolderInput}
      label={i18n.t("playlists.importPlaylistBtn")}
      onclick={() => { handleImportPlaylist(); showOverflowMenu = false; }}
    />
    <ContextMenuItem
      icon={FileOutput}
      label={i18n.t("playlists.exportPlaylistBtn")}
      onclick={() => { showExportOptionsModal = true; showOverflowMenu = false; }}
    />

    {#if duplicateCount > 0 || unavailableCount > 0}
      <ContextMenuDivider />
      {#if duplicateCount > 0}
        <ContextMenuItem
          icon={CopyPlus}
          label={i18n.t("playlists.removeDuplicatesBtn", { count: duplicateCount })}
          onclick={() => { removeDuplicates(); showOverflowMenu = false; }}
        />
      {/if}
      {#if unavailableCount > 0}
        <ContextMenuItem
          icon={AlertTriangle}
          label={i18n.t("playlists.removeUnavailableBtn", { count: unavailableCount })}
          onclick={() => { removeUnavailableTracks(); showOverflowMenu = false; }}
        />
      {/if}
    {/if}

    <ContextMenuDivider />
    <ContextMenuItem
      icon={Eraser}
      label={i18n.t("playlists.clearPlaylistBtn")}
      onclick={() => { playlistsStore.clearPlaylist(activePlaylist.id); showOverflowMenu = false; }}
    />
    {#if !isQueue}
      <ContextMenuItem
        icon={Trash2}
        destructive
        label={i18n.t("playlists.deletePlaylistBtn")}
        onclick={() => { showDeleteConfirm = true; showOverflowMenu = false; }}
      />
    {/if}
  </ContextMenu>
{/if}

{#if editingSongId !== null}
  <TagEditor
    songId={editingSongId}
    onClose={() => { editingSongId = null; }}
    onSave={handleTagEditorSaved}
  />
{/if}

{#if showDeleteConfirm && activePlaylist}
  <ConfirmDialog
    title={i18n.t("playlists.confirmDeletePlaylistTitle")}
    message={i18n.t("playlists.confirmDeletePlaylist", { name: activePlaylist.name })}
    confirmLabel={i18n.t("playlists.deletePlaylistBtn")}
    cancelLabel={i18n.t("playlists.cancel")}
    onConfirm={handleDeletePlaylist}
    onCancel={() => { showDeleteConfirm = false; }}
  />
{/if}

{#if showExportOptionsModal}
  <div use:portal class="fixed inset-0 z-50 flex items-center justify-center bg-black/60 backdrop-blur-xs select-none">
    <div class="bg-brand-sidebar border border-brand-border rounded-xl p-5 w-96 shadow-2xl space-y-4">
      <h3 class="text-base font-bold text-brand-text-primary flex items-center gap-2">
        <FileOutput class="w-5 h-5 text-brand-accent-text" />
        {i18n.t("playlists.exportModalTitle")}
      </h3>
      <div class="space-y-2.5 text-xs text-brand-text-secondary">
        <label class="flex items-center gap-2 hover:text-brand-text-primary transition-colors">
          <input type="radio" name="exportPathType" bind:group={exportRelative} value={true} class="accent-brand-accent" />
          <span>{i18n.t("playlists.useRelativePaths")}</span>
        </label>
        <label class="flex items-center gap-2 hover:text-brand-text-primary transition-colors">
          <input type="radio" name="exportPathType" bind:group={exportRelative} value={false} class="accent-brand-accent" />
          <span>{i18n.t("playlists.useAbsolutePaths")}</span>
        </label>
      </div>
      <div class="flex justify-end gap-2 pt-2">
        <button
          onclick={() => { showExportOptionsModal = false; }}
          class="px-3 py-1.5 rounded text-xs font-medium text-brand-text-secondary hover:bg-brand-main transition-colors"
        >
          {i18n.t("playlists.cancelBtn")}
        </button>
        <button
          onclick={triggerExport}
          class="px-3 py-1.5 rounded text-xs font-medium bg-brand-accent hover:bg-brand-accent-hover text-brand-accent-contrast transition-colors"
        >
          {i18n.t("playlists.exportBtn")}
        </button>
      </div>
    </div>
  </div>
{/if}

{#if showSaveQueueModal}
  <Modal onClose={() => showSaveQueueModal = false} maxWidth="max-w-sm">
    <div class="h-14 flex items-center justify-between px-6 border-b border-brand-border shrink-0 bg-brand-main">
      <div class="flex items-center gap-2">
        <FolderPlus class="w-4 h-4 text-brand-accent-text" />
        <h3 class="text-sm font-bold text-brand-text-primary">{i18n.t("playlists.saveQueueAsPlaylist", {}, "Save as Custom Playlist")}</h3>
      </div>
      <button onclick={() => showSaveQueueModal = false} class="text-brand-text-secondary hover:text-brand-text-primary transition-colors">
        <X class="w-4 h-4" />
      </button>
    </div>

    <form onsubmit={(e) => { e.preventDefault(); confirmSaveQueueAsCustomPlaylist(); }} class="flex flex-col gap-4 p-6 bg-brand-sidebar">
      <div class="flex flex-col gap-1.5">
        <label for="save-queue-name-input" class="text-xs font-semibold text-brand-text-secondary uppercase tracking-wider">
          {i18n.t("playlists.saveQueueNameLabel", {}, "Playlist Name")}
        </label>
        <Input
          id="save-queue-name-input"
          type="text"
          bind:value={saveQueueName}
          placeholder={i18n.t("playlists.saveQueueNamePlaceholder", {}, "My Queue Playlist")}
          class="w-full"
          required
          autofocus
        />
      </div>

      <div class="flex items-center justify-end gap-3 pt-2">
        <Button onclick={() => showSaveQueueModal = false} variant="secondary" size="sm">
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

