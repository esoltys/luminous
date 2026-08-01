<script lang="ts">
  import { onMount } from "svelte";
  import { playlistsStore } from "../stores/playlists.svelte";
  import { playerStore } from "../stores/player.svelte";
  import { collectionStore } from "../stores/collection.svelte";
  import {
    Trash2,
    ListMusic,
    RotateCcw,
    RotateCw,
    Edit3,
    AlertTriangle,
    Play,
    GripVertical,
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
    Eraser
  } from "lucide-svelte";
  import { getCoverArtUrl } from "../types";
  import { i18n } from "../stores/i18n.svelte";
  import type { PlaylistItem, Song } from "../types";
  import { parseSearchRules, isSmartPlaylistSpec } from "../utils/filterParser";
  import { invoke } from "@tauri-apps/api/core";
  import { open, save } from "@tauri-apps/plugin-dialog";
  import SongRating from "./SongRating.svelte";
  import TagEditor from "./TagEditor.svelte";
  import CoverArt from "./CoverArt.svelte";
  import PlaylistContextMenu from "./PlaylistContextMenu.svelte";
  import ConfirmDialog from "./ConfirmDialog.svelte";
  import SortableHeader from "./SortableHeader.svelte";
  import NowPlayingBars from "./NowPlayingBars.svelte";
  import LinkButton from "./LinkButton.svelte";
  import ColumnSelector from "./ColumnSelector.svelte";
  import { Clock } from "lucide-svelte";
  import Button from "./Button.svelte";
  import Input from "./Input.svelte";
  import IconActionButton from "./IconActionButton.svelte";
  import ContextMenu from "./ContextMenu.svelte";
  import ContextMenuItem from "./ContextMenuItem.svelte";
  import ContextMenuDivider from "./ContextMenuDivider.svelte";
  import { portal } from "../utils/portal";
  import { formatDate, formatFileSize, formatSampleRate, formatBitDepth, formatChannels } from "../utils/formatters";
  import { CONTEXT_MENU_WIDTH_PX } from "../constants";

  let gridColsStyle = $derived.by(() => {
    const cols: string[] = ["48px"]; // position column always present
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
  import {
    COVER_STACK_OFFSET_X_PX,
    COVER_STACK_OFFSET_Y_PX,
    COVER_STACK_ROTATION_DEG,
    COVER_STACK_SCALE_STEP,
    COVER_STACK_OPACITY_STEP,
  } from "../constants";

  const DRAG_RESET_DELAY_MS = 100;

  let editingSongId = $state<number | null>(null);

  // Inline title rename state
  let isEditingTitle = $state(false);
  let editTitleValue = $state("");

  // In-playlist real-time search filter state
  let filterQuery = $state("");

  // Multi-selection state
  let selectedUuids = $state<Set<string>>(new Set());
  let lastSelectedIndex = $state<number | null>(null);

  // Right-click context menu state
  let contextMenuState = $state<{ x: number; y: number; item: PlaylistItem } | null>(null);

  // Overflow ("more actions") menu state — houses the lower-frequency playlist
  // actions (import/export/duplicate-cleanup/clear/delete) so the toolbar
  // isn't a wall of always-visible buttons.
  let showOverflowMenu = $state(false);
  let overflowMenuPos = $state<{ x: number; y: number } | null>(null);
  let overflowButtonEl = $state<HTMLButtonElement | undefined>(undefined);

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
    if (activePlaylist) {
      editTitleValue = activePlaylist.name;
      isEditingTitle = true;
    }
  }

  async function saveRename() {
    if (!isEditingTitle) return;
    if (activePlaylist && editTitleValue.trim() !== "" && editTitleValue.trim() !== activePlaylist.name) {
      await playlistsStore.renamePlaylist(activePlaylist.id, editTitleValue.trim());
    }
    isEditingTitle = false;
  }

  function cancelRename() {
    isEditingTitle = false;
  }

  // Import / Export handlers
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
    if (playlistsStore.activePlaylistId !== null) {
      playlistsStore.selectPlaylist(playlistsStore.activePlaylistId);
    }
  }

  // Selected playlist from the store
  let activePlaylist = $derived(
    playlistsStore.playlists.find((p) => p.id === playlistsStore.activePlaylistId)
  );

  let isActive = $derived(
    activePlaylist !== undefined && playlistsStore.effectivePinnedPlaylistId === activePlaylist.id
  );

  let isQueue = $derived(
    activePlaylist !== undefined && !activePlaylist.dynamic_enabled && activePlaylist.name.toLowerCase() === "queue"
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
      autoPlay: activePlaylist.auto_play ?? true,
      populationMode: activePlaylist.population_mode ?? "all",
    });
  }

  type PlaylistSortField = "position" | keyof Song;
  let sortField = $state<PlaylistSortField>("position");
  let sortAsc = $state(true);

  function toggleSort(field: PlaylistSortField) {
    if (sortField === field) {
      sortAsc = !sortAsc;
    } else {
      sortField = field;
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
    if (vc.added) values.push(formatDate(song.added));
    if (vc.duration) values.push(formatDuration(song.length_nanosec));
    if (vc.path) values.push(song.path);
    return values
      .filter((v) => v !== undefined && v !== null && v !== "")
      .map((v) => String(v).toLowerCase());
  }

  // Derived filtered tracks based on filterQuery and sort selection
  let filteredTracks = $derived.by(() => {
    const q = filterQuery.trim().toLowerCase();
    let result = playlistsStore.activePlaylistTracks;
    if (q) {
      result = result.filter((item) => searchableColumnValues(item.song).some((v) => v.includes(q)));
    }

    if (sortField === "position") {
      return sortAsc ? result : [...result].reverse();
    }

    const fieldName = sortField as string;
    return [...result].sort((a, b) => {
      let valA = a.song ? (a.song as Record<string, any>)[fieldName] : undefined;
      let valB = b.song ? (b.song as Record<string, any>)[fieldName] : undefined;

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

  // Summary stats
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
    const top = [...counts.entries()]
      .sort((a, b) => b[1] - a[1])
      .map(([g]) => g);
    return top.slice(0, 2).join(" / ");
  });

  // Duplicate track detection
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
    collectionStore.selectedPlaylistId = null;
  }

  function handlePlayPlaylistItem(index: number) {
    const item = playlistsStore.activePlaylistTracks[index];
    if (!item || isItemUnavailable(item)) return;
    if (playlistsStore.activePlaylistId !== null) {
      playerStore.playPlaylistItem(playlistsStore.activePlaylistId, index);
    }
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

  async function rateItem(item: PlaylistItem, rating: number) {
    if (!item.song) return;
    item.song.rating = await invoke<number>("set_song_rating", {
      songId: item.song.id,
      rating,
    });
  }

  // Row selection handlers
  function handleRowClick(event: MouseEvent, item: PlaylistItem) {
    const actualIndex = playlistsStore.activePlaylistTracks.findIndex((t) => t.uuid === item.uuid);

    if (event.shiftKey && lastSelectedIndex !== null && lastSelectedIndex !== -1) {
      const start = Math.min(lastSelectedIndex, actualIndex);
      const end = Math.max(lastSelectedIndex, actualIndex);
      const nextSet = new Set(selectedUuids);
      for (let i = start; i <= end; i++) {
        const track = playlistsStore.activePlaylistTracks[i];
        if (track) nextSet.add(track.uuid);
      }
      selectedUuids = nextSet;
    } else if (event.ctrlKey || event.metaKey) {
      const nextSet = new Set(selectedUuids);
      if (nextSet.has(item.uuid)) {
        nextSet.delete(item.uuid);
      } else {
        nextSet.add(item.uuid);
      }
      selectedUuids = nextSet;
      lastSelectedIndex = actualIndex;
    } else if (selectedUuids.size === 1 && selectedUuids.has(item.uuid)) {
      selectedUuids = new Set();
      lastSelectedIndex = null;
    } else {
      selectedUuids = new Set([item.uuid]);
      lastSelectedIndex = actualIndex;
    }
  }

  function handleKeydown(event: KeyboardEvent) {
    const target = event.target as HTMLElement | null;
    if (target && (target.tagName === "INPUT" || target.tagName === "TEXTAREA")) return;

    if (event.key === "Delete" || event.key === "Backspace") {
      if (selectedUuids.size > 0 && activePlaylist) {
        event.preventDefault();
        playlistsStore.removeItemsFromPlaylist(activePlaylist.id, Array.from(selectedUuids));
        selectedUuids = new Set();
        lastSelectedIndex = null;
      }
    } else if ((event.ctrlKey || event.metaKey) && event.key.toLowerCase() === "a") {
      event.preventDefault();
      selectedUuids = new Set(filteredTracks.map((t) => t.uuid));
    } else if (event.key === "Escape") {
      selectedUuids = new Set();
      lastSelectedIndex = null;
    }
  }

  function handleWindowMouseDown(e: MouseEvent) {
    if (selectedUuids.size === 0) return;
    const target = e.target as HTMLElement;
    if (!target) return;
    if (
      target.closest("[data-playlist-row]") ||
      target.closest("[role='menu']") ||
      target.closest("[data-floating-toolbar]") ||
      target.closest("button") ||
      target.closest("input")
    ) {
      return;
    }
    selectedUuids = new Set();
    lastSelectedIndex = null;
  }

  function handleContextMenu(event: MouseEvent, item: PlaylistItem) {
    event.preventDefault();
    if (!selectedUuids.has(item.uuid)) {
      selectedUuids = new Set([item.uuid]);
      const actualIndex = playlistsStore.activePlaylistTracks.findIndex((t) => t.uuid === item.uuid);
      lastSelectedIndex = actualIndex;
    }
    contextMenuState = { x: event.clientX, y: event.clientY, item };
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
      lastSelectedIndex = null;
    }
  }

  // Drag and drop state and handlers
  let draggedIndex = $state<number | null>(null);
  let dragOverIndex = $state<number | null>(null);

  function handleDragStart(event: DragEvent, index: number, item: PlaylistItem) {
    if (isItemUnavailable(item)) return;
    draggedIndex = index;

    if (!selectedUuids.has(item.uuid)) {
      selectedUuids = new Set([item.uuid]);
      lastSelectedIndex = index;
    }

    if (event.dataTransfer) {
      event.dataTransfer.effectAllowed = "move";
      event.dataTransfer.setData("text/plain", index.toString());
      event.dataTransfer.setData("text", index.toString());
      event.dataTransfer.setData("application/x-playlist-index", index.toString());
    }
  }

  function handleDragOver(event: DragEvent, index: number) {
    event.preventDefault();
    if (event.dataTransfer) {
      event.dataTransfer.dropEffect = "move";
    }
  }

  function handleDragEnter(event: DragEvent, index: number) {
    event.preventDefault();
    if (event.dataTransfer) {
      event.dataTransfer.dropEffect = "move";
    }
    dragOverIndex = index;
  }

  function handleDragLeave(event: DragEvent, index: number) {
    const currentTarget = event.currentTarget as HTMLElement | null;
    const relatedTarget = event.relatedTarget as Node | null;
    if (currentTarget && relatedTarget && currentTarget.contains(relatedTarget)) {
      return;
    }
    if (dragOverIndex === index) {
      dragOverIndex = null;
    }
  }

  function handleDragEnd() {
    dragOverIndex = null;
    setTimeout(() => {
      draggedIndex = null;
    }, DRAG_RESET_DELAY_MS);
  }

  function handleDrop(event: DragEvent, targetIndex: number) {
    event.preventDefault();
    if (!activePlaylist) return;

    if (selectedUuids.size > 1) {
      const selectedIndices = playlistsStore.activePlaylistTracks
        .map((t, idx) => ({ uuid: t.uuid, idx }))
        .filter((entry) => selectedUuids.has(entry.uuid))
        .map((entry) => entry.idx);

      if (selectedIndices.length > 0) {
        playlistsStore.reorderItemsBatch(activePlaylist.id, selectedIndices, targetIndex);
      }
    } else {
      let sourceIndex = draggedIndex;
      if (sourceIndex === null && event.dataTransfer) {
        const data = event.dataTransfer.getData("text/plain");
        if (data) {
          const parsed = parseInt(data, 10);
          if (!isNaN(parsed)) {
            sourceIndex = parsed;
          }
        }
      }

      if (sourceIndex !== null && sourceIndex !== targetIndex) {
        playlistsStore.reorderItem(activePlaylist.id, sourceIndex, targetIndex);
      }
    }

    draggedIndex = null;
    dragOverIndex = null;
  }

  async function handlePlayAll() {
    if (!activePlaylist || playlistsStore.activePlaylistTracks.length === 0) return;
    await playerStore.setShuffleMode("off");
    await playerStore.playPlaylistItem(activePlaylist.id, 0);
  }

  async function handleShufflePlay() {
    if (!activePlaylist || playlistsStore.activePlaylistTracks.length === 0) return;
    const randomIndex = Math.floor(Math.random() * playlistsStore.activePlaylistTracks.length);
    await playerStore.setShuffleMode("all");
    await playerStore.playPlaylistItem(activePlaylist.id, randomIndex);
  }

  function formatDuration(ns: number | undefined): string {
    if (!ns) return "0:00";
    const sec = Math.floor(ns / 1_000_000_000);
    const m = Math.floor(sec / 60);
    const s = sec % 60;
    return `${m}:${s < 10 ? "0" : ""}${s}`;
  }

  let currentCoverUrl = $derived.by(() => {
    const song = playerStore.currentSong;
    if (!song) return null;
    if (song.art_manual) {
      return getCoverArtUrl(`luminous-art://${song.art_manual}`);
    }
    if (song.art_automatic) {
      if (song.art_automatic.startsWith("album-")) {
        return getCoverArtUrl(`luminous-art://${song.art_automatic}`);
      } else {
        return getCoverArtUrl(`luminous-art://local/${song.art_automatic}`);
      }
    }
    return null;
  });
</script>

<svelte:window onkeydown={handleKeydown} onmousedown={handleWindowMouseDown} />

<div class="flex-1 flex flex-col overflow-hidden bg-brand-main text-brand-text-secondary h-full relative select-none">
  {#if currentCoverUrl}
    <div class="absolute inset-0 pointer-events-none select-none z-0 overflow-hidden">
      <div
        class="absolute inset-0 bg-cover bg-center opacity-[0.12] scale-105 blur-[60px] saturate-[180%] transition-all duration-1000"
        style="background-image: url('{currentCoverUrl}');"
      ></div>
      <div class="absolute inset-0 bg-gradient-to-t from-brand-main via-transparent to-brand-main/20"></div>
    </div>
  {/if}

  {#if activePlaylist}
    <div class="flex-1 flex flex-col min-h-0 relative z-10">
    <!-- Stacked Cover Art Hero & Summary Banner Header -->
    <div class="relative z-30 w-full overflow-hidden border-b border-brand-border/60 bg-brand-main/60 backdrop-blur-md px-6 pt-6 pb-6 shrink-0">
      <div class="flex items-stretch justify-between gap-6 relative z-10">
        <!-- Left Title & Summary Metadata -->
        <div class="flex flex-col justify-end gap-2 min-w-0 flex-1">
          {#if isEditingTitle}
            <div class="flex items-center gap-2">
              <input
                bind:value={editTitleValue}
                onkeydown={(e) => { if (e.key === "Enter") saveRename(); else if (e.key === "Escape") cancelRename(); }}
                class="bg-brand-sidebar border border-brand-accent text-brand-text-primary px-3 py-1 text-2xl font-bold rounded-lg focus:outline-none"
                use:focusAndSelect
              />
              <button onclick={saveRename} class="p-1.5 text-brand-accent-text hover:text-brand-accent cursor-pointer" title={i18n.t('playlists.saveRenameTooltip')}>
                <Check class="w-5 h-5" />
              </button>
              <button onclick={cancelRename} class="p-1.5 text-brand-text-secondary hover:text-brand-text-primary cursor-pointer" title={i18n.t('playlists.cancel')}>
                <X class="w-5 h-5" />
              </button>
            </div>
          {:else}
            <div class="flex items-center gap-3 group/title">
              <h1
                ondblclick={startRename}
                class="text-3xl sm:text-4xl font-heading font-bold text-brand-text-primary cursor-pointer hover:text-brand-accent-text transition-colors truncate py-0.5 leading-snug"
                title={i18n.t("playlists.renamePlaylistTooltip")}
              >
                {activePlaylist.name}
              </h1>
              <button
                onclick={startRename}
                class="opacity-0 group-hover/title:opacity-100 text-brand-text-secondary hover:text-brand-text-primary transition-opacity p-1 cursor-pointer"
                title={i18n.t("playlists.renamePlaylistTooltip")}
              >
                <Pencil class="w-4 h-4" />
              </button>
            </div>
          {/if}

          <!-- Summary Metadata Line -->
          <div class="flex items-center gap-3 text-xs text-brand-text-secondary font-medium mt-1">
            <span>
              {i18n.t("playlists.statsLine", {
                genre: genreSummaryLabel || i18n.t("playlists.unknownGenre"),
                songs: activePlaylist.track_count === 1
                  ? i18n.t("playlists.oneSong")
                  : i18n.t("playlists.songsCount", { count: activePlaylist.track_count }),
                duration: totalRuntimeLabel,
              })}
            </span>
          </div>

          <!-- Action Buttons: Play All & Shuffle Play -->
          <div class="flex items-center gap-3 mt-3">
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
            <ColumnSelector align="left" iconOnly />
          </div>

          <!-- Secondary Control Buttons Row -->
          <div class="flex flex-wrap items-center gap-2.5 mt-2.5 select-none">
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
                bind:this={overflowButtonEl}
                onclick={toggleOverflowMenu}
                title={i18n.t("playlists.moreActionsTooltip")}
                class="flex items-center justify-center w-10 h-10 rounded-full border border-brand-border text-brand-text-secondary hover:text-brand-accent-text hover:bg-brand-sidebar transition-colors cursor-pointer shadow-xs"
              >
                <MoreHorizontal class="w-4 h-4" />
              </button>
            </div>
          </div>
        </div>

        <!-- Right: 3D Stacked Album Cover Preview Header or Special Queue Banner -->
        {#if isQueue}
          <div class="w-32 h-32 hidden sm:flex shrink-0 bg-gradient-to-br from-[#4338CA] to-[#7C3AED] items-center justify-center overflow-hidden border border-brand-border/60 shadow-xl">
            <Layers class="w-14 h-14 text-white/90" />
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
      </div>
    </div>

    <!-- Tracks List Container -->
    <div class="flex-1 min-h-0 p-6 flex flex-col" class:pb-28={!!playerStore.currentSong}>
      <div class="flex-1 min-h-0 border border-brand-border/60 rounded-xl bg-brand-sidebar/30 backdrop-blur-md relative overflow-auto">
      <!-- Header -->
      <div class="sticky top-0 z-20 flex flex-col bg-brand-sidebar border-b border-brand-border text-xs text-brand-text-secondary uppercase tracking-wider font-semibold select-none">
        <div role="row" class="grid items-center py-3 px-4" style={gridColsStyle}>
          <SortableHeader
            active={sortField === "position"}
            {sortAsc}
            onclick={() => toggleSort("position")}
            class="text-center hover:text-brand-text-primary transition-colors flex items-center justify-center gap-1 cursor-pointer font-semibold uppercase tracking-wider min-w-0"
          >
            {#snippet label(arrow)}<span class="truncate max-w-[calc(100%-1rem)]">{i18n.t("playlists.tableHeaderTrack")} {arrow}</span>{/snippet}
          </SortableHeader>
          {#if collectionStore.visibleColumns.title}
            <SortableHeader
              active={sortField === "title"}
              {sortAsc}
              onclick={() => toggleSort("title")}
              class="text-left hover:text-brand-text-primary transition-colors flex items-center gap-1 cursor-pointer font-semibold uppercase tracking-wider min-w-0"
            >
              {#snippet label(arrow)}<span class="truncate max-w-[calc(100%-1rem)]">{i18n.t("playlists.tableHeaderTitle")} {arrow}</span>{/snippet}
            </SortableHeader>
          {/if}
          {#if collectionStore.visibleColumns.artist}
            <SortableHeader
              active={sortField === "artist"}
              {sortAsc}
              onclick={() => toggleSort("artist")}
              class="text-left hover:text-brand-text-primary transition-colors flex items-center gap-1 cursor-pointer font-semibold uppercase tracking-wider min-w-0"
            >
              {#snippet label(arrow)}<span class="truncate max-w-[calc(100%-1rem)]">{i18n.t("playlists.tableHeaderArtist")} {arrow}</span>{/snippet}
            </SortableHeader>
          {/if}
          {#if collectionStore.visibleColumns.album}
            <SortableHeader
              active={sortField === "album"}
              {sortAsc}
              onclick={() => toggleSort("album")}
              class="text-left hover:text-brand-text-primary transition-colors flex items-center gap-1 cursor-pointer font-semibold uppercase tracking-wider min-w-0"
            >
              {#snippet label(arrow)}<span class="truncate max-w-[calc(100%-1rem)]">{i18n.t("collection.tableHeaderAlbum")} {arrow}</span>{/snippet}
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
            <div class="text-center">{i18n.t("collection.tableHeaderActions")}</div>
          {/if}
        </div>
      </div>

      <!-- Rows -->
      <div class="divide-y divide-brand-border/40">
        {#each filteredTracks as item, index}
          {@const trueUnavailable = isItemUnavailable(item)}
          {@const disconnected = !trueUnavailable && collectionStore.isPathOnDisconnectedDrive(item.song?.path)}
          {@const unavailable = trueUnavailable || disconnected}
          {@const isDuplicate = duplicateUuids.includes(item.uuid)}
          {@const isSelected = selectedUuids.has(item.uuid)}
          {@const actualIndex = playlistsStore.activePlaylistTracks.findIndex(t => t.uuid === item.uuid)}
          <div
            role="row"
            tabindex="0"
            onkeydown={(e) => {
              if (e.key === 'Enter' && !unavailable) handlePlayPlaylistItem(actualIndex);
            }}
            data-playlist-row="true"
            draggable={!unavailable ? "true" : "false"}
            ondragstart={(e) => !unavailable && handleDragStart(e, actualIndex, item)}
            ondragover={(e) => handleDragOver(e, actualIndex)}
            ondragenter={(e) => handleDragEnter(e, actualIndex)}
            ondragleave={(e) => handleDragLeave(e, actualIndex)}
            ondragend={handleDragEnd}
            ondrop={(e) => handleDrop(e, actualIndex)}
            onclick={(e) => handleRowClick(e, item)}
            oncontextmenu={(e) => handleContextMenu(e, item)}
            ondblclick={() => !unavailable && handlePlayPlaylistItem(actualIndex)}
            class="grid items-center py-2.5 px-4 group transition-all duration-150 select-none text-sm border-b border-brand-border/40
              {unavailable
                ? 'opacity-50 cursor-not-allowed'
                : 'cursor-grab active:cursor-grabbing'}
              {isSelected ? 'bg-brand-accent/20 text-brand-accent-text-hover' : 'hover:bg-brand-sidebar/40'}
              {!unavailable && !isSelected && playerStore.playlistItemUuid === item.uuid ? 'bg-brand-accent/10 text-brand-accent-text-hover' : ''}
              {dragOverIndex === actualIndex && draggedIndex !== null && draggedIndex !== actualIndex
                ? (actualIndex < draggedIndex ? 'border-t-2! border-t-brand-accent bg-brand-accent/5' : 'border-b-2! border-b-brand-accent bg-brand-accent/5')
                : ''
              }"
            style={gridColsStyle}
          >
            <div class="text-center text-brand-text-secondary font-medium relative min-w-0 cursor-grab active:cursor-grabbing">
              <div class="relative w-5 h-4 mx-auto flex items-center justify-center">
                <GripVertical class="w-3.5 h-3.5 opacity-0 group-hover:opacity-60 text-brand-text-secondary transition-opacity shrink-0 absolute -left-3 top-0.5 pointer-events-none" />
                {#if playerStore.playlistItemUuid === item.uuid && playerStore.state === "playing"}
                  <div class="flex items-center justify-center gap-0.5 h-4 w-4 absolute inset-0 group-hover:opacity-0 transition-opacity">
                    <NowPlayingBars />
                  </div>
                {:else}
                  <span class="absolute inset-0 flex items-center justify-center group-hover:opacity-0 transition-opacity">
                    {actualIndex + 1}
                  </span>
                {/if}
                <button
                  onclick={(e) => { e.stopPropagation(); if (!unavailable) handlePlayPlaylistItem(actualIndex); }}
                  class="absolute inset-0 flex items-center justify-center opacity-0 group-hover:opacity-100 text-brand-accent-text hover:text-brand-accent-text-hover transition-opacity cursor-pointer disabled:opacity-0 disabled:cursor-not-allowed"
                  disabled={unavailable}
                  title={i18n.t("playlists.playTrack")}
                >
                  <Play class="w-4 h-4 fill-current" />
                </button>
              </div>
            </div>

            {#if collectionStore.visibleColumns.title}
              <div class="font-medium truncate pr-4 min-w-0 {isSelected || (!unavailable && playerStore.playlistItemUuid === item.uuid) ? 'text-brand-accent-text-hover' : unavailable ? 'text-brand-text-secondary' : 'text-brand-text-primary'}">
                <div class="flex items-center gap-2 max-w-full">
                  {#if trueUnavailable}
                    <span title={i18n.t("playlists.fileNotFoundTooltip")}>
                      <AlertTriangle class="w-3.5 h-3.5 shrink-0 text-amber-400/80" />
                    </span>
                    <span class="truncate line-through decoration-brand-text-secondary/40">
                      {item.song?.title ?? i18n.t("collection.unknownSong")}
                    </span>
                  {:else if disconnected}
                    <span title={i18n.t("collection.driveDisconnectedTooltip")}>
                      <AlertTriangle class="w-3.5 h-3.5 shrink-0 text-amber-400/80" />
                    </span>
                    <span class="truncate">
                      {item.song?.title ?? i18n.t("collection.unknownSong")}
                    </span>
                  {:else if item.song?.title}
                    {#if isDuplicate}
                      <span
                        class="px-1.5 py-0.5 text-[10px] font-bold rounded bg-purple-500/20 text-purple-300 border border-purple-500/40 shrink-0"
                        title={i18n.t("playlists.duplicateTrackFlag")}
                      >
                        {i18n.t("playlists.duplicateTrackFlag")}
                      </span>
                    {/if}
                    <span
                      class="truncate min-w-0 font-medium {playerStore.playlistItemUuid === item.uuid ? 'text-brand-accent-text-hover' : 'text-brand-text-primary'}"
                      title={item.song.title}
                    >
                      {item.song.title}
                    </span>
                  {:else}
                    <span class="truncate min-w-0">{i18n.t("collection.unknownSong")}</span>
                  {/if}
                </div>
              </div>
            {/if}

            {#if collectionStore.visibleColumns.artist}
              <div class="text-brand-text-secondary truncate pr-4 min-w-0">
                {#if trueUnavailable}
                  <span class="text-brand-text-secondary italic text-xs">{i18n.t("playlists.fileNotFoundText")}</span>
                {:else if disconnected}
                  <span class="text-brand-text-secondary italic text-xs">{i18n.t("collection.driveDisconnectedText")}</span>
                {:else if item.song?.artist}
                  <LinkButton
                    onclick={(e) => { e.stopPropagation(); collectionStore.viewArtist(item.song?.album_artist?.trim() || item.song?.artist || ""); }}
                    class="text-brand-text-secondary truncate min-w-0"
                    title={i18n.t("collection.filterByArtist", { artist: item.song.artist })}
                  >
                    {item.song.artist}
                  </LinkButton>
                {:else}
                  <span class="text-brand-text-secondary truncate min-w-0">{i18n.t("collection.unknownArtist")}</span>
                {/if}
              </div>
            {/if}

            {#if collectionStore.visibleColumns.album}
              <div class="text-brand-text-secondary truncate pr-4 min-w-0">
                {#if unavailable}
                  <span class="text-brand-text-secondary italic text-xs truncate min-w-0">{item.song?.album ?? ""}</span>
                {:else if item.song?.album}
                  <LinkButton
                    onclick={(e) => { e.stopPropagation(); collectionStore.viewAlbum(item.song?.album || ""); }}
                    class="text-brand-text-secondary truncate min-w-0"
                    title={i18n.t("collection.filterByAlbum", { album: item.song.album })}
                  >
                    {item.song.album}
                  </LinkButton>
                {:else}
                  <span class="text-brand-text-secondary truncate min-w-0">{i18n.t("collection.unknownAlbum")}</span>
                {/if}
              </div>
            {/if}

            {#if collectionStore.visibleColumns.composer}
              <div class="text-brand-text-secondary truncate pr-4 min-w-0 text-xs font-medium" title={item.song?.composer}>
                {item.song?.composer || "—"}
              </div>
            {/if}
            {#if collectionStore.visibleColumns.album_artist}
              <div class="text-brand-text-secondary truncate pr-4 min-w-0 text-xs font-medium" title={item.song?.album_artist}>
                {item.song?.album_artist || "—"}
              </div>
            {/if}
            {#if collectionStore.visibleColumns.format}
              <div class="text-brand-text-secondary truncate pr-2 min-w-0 text-xs font-semibold uppercase">
                {item.song?.filetype ? item.song.filetype.toUpperCase() : "—"}
              </div>
            {/if}
            {#if collectionStore.visibleColumns.year}
              <div class="text-brand-text-secondary truncate pr-2 min-w-0 text-xs font-medium">
                {item.song?.year || "—"}
              </div>
            {/if}
            {#if collectionStore.visibleColumns.genre}
              <div class="text-brand-text-secondary truncate pr-2 min-w-0 text-xs font-medium" title={item.song?.genre}>
                {item.song?.genre || "—"}
              </div>
            {/if}
            {#if collectionStore.visibleColumns.grouping}
              <div class="text-brand-text-secondary truncate pr-2 min-w-0 text-xs font-medium" title={item.song?.grouping}>
                {item.song?.grouping || "—"}
              </div>
            {/if}
            {#if collectionStore.visibleColumns.bpm}
              <div class="text-brand-text-secondary truncate pr-2 min-w-0 text-xs font-mono">
                {item.song?.bpm || "—"}
              </div>
            {/if}
            {#if collectionStore.visibleColumns.initial_key}
              <div class="text-brand-text-secondary truncate pr-2 min-w-0 text-xs font-mono">
                {item.song?.initial_key || "—"}
              </div>
            {/if}
            {#if collectionStore.visibleColumns.bitrate}
              <div class="text-brand-text-secondary truncate pr-2 min-w-0 text-xs font-mono">
                {item.song?.bitrate ? `${item.song.bitrate}k` : "—"}
              </div>
            {/if}
            {#if collectionStore.visibleColumns.samplerate}
              <div class="text-brand-text-secondary truncate pr-2 min-w-0 text-xs font-mono">
                {formatSampleRate(item.song?.samplerate)}
              </div>
            {/if}
            {#if collectionStore.visibleColumns.bitdepth}
              <div class="text-brand-text-secondary truncate pr-2 min-w-0 text-xs font-mono">
                {formatBitDepth(item.song?.bitdepth)}
              </div>
            {/if}
            {#if collectionStore.visibleColumns.channels}
              <div class="text-brand-text-secondary truncate pr-2 min-w-0 text-xs font-medium">
                {formatChannels(item.song?.channels)}
              </div>
            {/if}
            {#if collectionStore.visibleColumns.filesize}
              <div class="text-brand-text-secondary truncate pr-2 min-w-0 text-xs font-mono">
                {formatFileSize(item.song?.filesize)}
              </div>
            {/if}
            {#if collectionStore.visibleColumns.rating}
              <div class="flex justify-center min-w-0" onclick={(e) => e.stopPropagation()} role="presentation">
                {#if item.song && !unavailable}
                  <SongRating rating={item.song.rating} onRate={(r) => rateItem(item, r)} />
                {/if}
              </div>
            {/if}
            {#if collectionStore.visibleColumns.playcount}
              <div class="text-center text-brand-text-secondary text-xs min-w-0">
                {item.song?.playcount || 0}
              </div>
            {/if}
            {#if collectionStore.visibleColumns.skipcount}
              <div class="text-center text-brand-text-secondary text-xs min-w-0">
                {item.song?.skipcount || 0}
              </div>
            {/if}
            {#if collectionStore.visibleColumns.lastplayed}
              <div class="text-center text-brand-text-secondary font-mono text-xs whitespace-nowrap">
                {formatDate(item.song?.lastplayed)}
              </div>
            {/if}
            {#if collectionStore.visibleColumns.added}
              <div class="text-center text-brand-text-secondary font-mono text-xs whitespace-nowrap">
                {formatDate(item.song?.added)}
              </div>
            {/if}
            {#if collectionStore.visibleColumns.duration}
              <div class="text-center text-brand-text-secondary text-xs min-w-0">
                {formatDuration(item.song?.length_nanosec)}
              </div>
            {/if}
            {#if collectionStore.visibleColumns.path}
              <div class="text-brand-text-secondary truncate pr-4 min-w-0 text-xs font-mono" title={item.song?.path}>
                {item.song?.path || "—"}
              </div>
            {/if}

            {#if collectionStore.visibleColumns.actions}
              <div class="text-center min-w-0">
                <div class="flex items-center justify-center gap-2.5">
                  <button
                    onclick={(e) => { e.stopPropagation(); item.song?.id && !unavailable && openTagEditor(item.song.id); }}
                    class="text-brand-text-secondary/60 hover:text-brand-accent-text transition-colors disabled:opacity-30 cursor-pointer"
                    title={i18n.t("collection.editTagsTooltip")}
                    disabled={!item.song || unavailable}
                  >
                    <Edit3 class="w-4 h-4" />
                  </button>
                  <button
                    onclick={(e) => { e.stopPropagation(); handleRemoveItem(item.uuid); }}
                    class="text-brand-text-secondary/60 hover:text-red-400 transition-colors cursor-pointer"
                    title={i18n.t("playlists.removeFromPlaylist")}
                  >
                    <Trash2 class="w-4 h-4" />
                  </button>
                </div>
              </div>
            {/if}
          </div>
        {/each}

        {#if filteredTracks.length === 0}
          <div class="py-12 text-center text-brand-text-secondary/45 select-none">
            <ListMusic class="w-12 h-12 mx-auto mb-2 text-brand-text-secondary/30" />
            {#if filterQuery}
              {i18n.t("playlists.noFilterResults", { query: filterQuery })}
            {:else}
              {i18n.t("playlists.emptyPlaylistTitle")}. {i18n.t("playlists.emptyPlaylistText")}
            {/if}
          </div>
        {/if}
      </div>
    </div>
  </div>
  </div>

    <!-- Floating Multi-Select Batch Toolbar -->
    {#if selectedUuids.size > 0}
      <div data-floating-toolbar="true" class="absolute left-1/2 -translate-x-1/2 z-40 bg-brand-sidebar/95 border border-brand-border/80 shadow-2xl rounded-full px-5 py-2.5 flex items-center gap-4 text-xs font-semibold backdrop-blur-xl animate-in fade-in slide-in-from-bottom-4 duration-200" class:bottom-6={!playerStore.currentSong} class:bottom-28={!!playerStore.currentSong}>
        <span class="text-brand-accent-text font-bold">
          {i18n.t("playlists.selectedCount", { count: selectedUuids.size })}
        </span>
        <div class="h-4 w-px bg-brand-border/60"></div>
        <button
          onclick={playSelected}
          class="flex items-center gap-1.5 hover:text-brand-accent-text transition-colors cursor-pointer"
        >
          <Play class="w-3.5 h-3.5 fill-current text-brand-accent-text" />
          <span>{i18n.t("playlists.playSelected")}</span>
        </button>
        <button
          onclick={removeSelected}
          class="flex items-center gap-1.5 text-red-400 hover:text-red-300 transition-colors cursor-pointer"
        >
          <Trash2 class="w-3.5 h-3.5" />
          <span>{i18n.t("playlists.removeSelected")}</span>
        </button>
        <div class="h-4 w-px bg-brand-border/60"></div>
        <button
          onclick={() => { selectedUuids = new Set(); lastSelectedIndex = null; }}
          class="text-brand-text-secondary hover:text-brand-text-primary transition-colors cursor-pointer"
        >
          {i18n.t("playlists.clearSelection")}
        </button>
      </div>
    {/if}
  {:else}
    <!-- No playlist selected -->
    <div class="flex-1 flex flex-col items-center justify-center text-brand-text-secondary/60">
      <ListMusic class="w-16 h-16 mb-4 text-brand-text-secondary/30" />
      <h2 class="text-lg font-bold text-brand-text-secondary/80 mb-1">{i18n.t("playlists.noPlaylistsTitle")}</h2>
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
    onGoToArtist={singleItem.song?.artist ? () => collectionStore.viewArtist(singleItem.song?.album_artist?.trim() || singleItem.song?.artist || "") : undefined}
    onGoToAlbum={singleItem.song?.album ? () => collectionStore.viewAlbum(singleItem.song?.album || "") : undefined}
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
        <label class="flex items-center gap-2 cursor-pointer hover:text-brand-text-primary transition-colors">
          <input type="radio" name="exportPathType" bind:group={exportRelative} value={true} class="accent-brand-accent" />
          <span>{i18n.t("playlists.useRelativePaths")}</span>
        </label>
        <label class="flex items-center gap-2 cursor-pointer hover:text-brand-text-primary transition-colors">
          <input type="radio" name="exportPathType" bind:group={exportRelative} value={false} class="accent-brand-accent" />
          <span>{i18n.t("playlists.useAbsolutePaths")}</span>
        </label>
      </div>
      <div class="flex justify-end gap-2 pt-2">
        <button
          onclick={() => { showExportOptionsModal = false; }}
          class="px-3 py-1.5 rounded text-xs font-medium text-brand-text-secondary hover:bg-brand-main transition-colors cursor-pointer"
        >
          {i18n.t("playlists.cancelBtn")}
        </button>
        <button
          onclick={triggerExport}
          class="px-3 py-1.5 rounded text-xs font-medium bg-brand-accent hover:bg-brand-accent-hover text-brand-accent-contrast transition-colors cursor-pointer"
        >
          {i18n.t("playlists.exportBtn")}
        </button>
      </div>
    </div>
  </div>
{/if}
