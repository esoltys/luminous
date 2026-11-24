<script lang="ts">
  import { isRemoteSource } from "../utils/remoteSource";
  import { onMount } from "svelte";
  import { fade } from "../utils/motion";
  import { playlistsStore } from "../stores/playlists.svelte";
  import { pinnedStore } from "../stores/pinned.svelte";
  import { playerStore } from "../stores/player.svelte";
  import { collectionStore } from "../stores/collection.svelte";
  import { navigationStore } from "../stores/navigation.svelte";
  import { windowLayoutStore } from "../stores/windowLayout.svelte";
  import {
    TrashIcon as Trash2,
    PlaylistIcon as ListMusic,
    ArrowCounterClockwiseIcon as RotateCcw,
    ArrowClockwiseIcon as RotateCw,
    WarningIcon as AlertTriangle,
    PlayIcon as Play,
    FolderOpenIcon as FolderInput,
    ExportIcon as FileOutput,
    PencilSimpleIcon as Pencil,
    CheckIcon as Check,
    XIcon as X,
    CopyIcon as CopyPlus,
    MusicNotesIcon as Music,
    ArrowsClockwiseIcon as RefreshCw,
    UsbIcon as Usb,
    BroadcastIcon as Radio,
    StackIcon as Layers,
    EraserIcon as Eraser,
    SparkleIcon as Sparkles,
    FolderPlusIcon as FolderPlus,
    PushPinIcon as Pin,
    PushPinSlashIcon as PinOff,
    ShareNetworkIcon as Share,
    InfinityIcon as Infinity,
    LinkIcon as Link,
  } from "phosphor-svelte";
  import MoreIcon from "./MoreIcon.svelte";
  import InlineMessage from "./InlineMessage.svelte";
  import { toastStore } from "../stores/toast.svelte";
  import { isLiveSong, resolveArtUrl } from "../types";
  import { i18n } from "../stores/i18n.svelte";
  import type { PlaylistItem, QueuePopulationMode, Song } from "../types";
  import {
    autoContinueRunStarts,
    formatPlaylistItemCount,
    getPlaylistCapabilities,
    getPlaylistDisplayName,
  } from "../utils/playlist";
  import { formatUpdatedAgo, radioStore } from "../stores/radio.svelte";
  import StationListModal from "./radio/StationListModal.svelte";
  import { parseSearchRules } from "../utils/filterParser";
  import { rememberScroll } from "../utils/scrollMemory";
  import { openInPicard } from "../utils/picard";
  import { invoke } from "@tauri-apps/api/core";
  import { open, save } from "@tauri-apps/plugin-dialog";
  import TagEditor from "./TagEditor.svelte";
  import AlbumTagEditor from "./AlbumTagEditor.svelte";
  import BlurredCover from "./BlurredCover.svelte";
  import GenreChips from "./GenreChips.svelte";
  import { parseMultiValue, joinMultiValue } from "../utils/multiValue";
  import { tagsStore } from "../stores/tags.svelte";
  import { songsToCoverStack } from "../utils/covers";
  import CoverArt from "./CoverArt.svelte";
  import CoverStack from "./CoverStack.svelte";
  import CoverMosaic from "./CoverMosaic.svelte";
  import PlaylistContextMenu from "./PlaylistContextMenu.svelte";
  import ShareModal from "./ShareModal.svelte";
  import ConfirmDialog from "./ConfirmDialog.svelte";
  import Modal from "./Modal.svelte";
  import Button from "./Button.svelte";
  import Input from "./Input.svelte";
  import IconActionButton from "./IconActionButton.svelte";
  import ContextMenu from "./ContextMenu.svelte";
  import ContextMenuItem from "./ContextMenuItem.svelte";
  import ContextMenuDivider from "./ContextMenuDivider.svelte";
  import SearchEmptyState from "./SearchEmptyState.svelte";
  import { matchesSongSearch } from "../utils/songSearch";
  import { portal } from "../utils/portal";
  import {
    formatSampleRate,
    formatBitDepth,
    formatChannels,
    formatFileSize,
    formatDuration,
    formatHoursMinutes,
    formatStorageSize,
  } from "../utils/formatters";
  import { formatDateAdded } from "../utils/date";
  import { CONTEXT_MENU_WIDTH_PX } from "../constants";
  import { compareSongs } from "../utils/songSort";
  import SongTable, { type SongTableRow } from "./SongTable.svelte";
  import PlaylistSizeLimitPopover from "./PlaylistSizeLimitPopover.svelte";
  import PopulationModeTabs from "./PopulationModeTabs.svelte";
  import { PlaylistSizeLimit } from "../utils/sizeFit.svelte";

  // Default column widths (px or fr) — used when no saved width exists for a column.
  const PLAYLIST_COL_DEFAULTS: Partial<
    Record<keyof typeof collectionStore.visibleColumns, string>
  > = {
    title: "2fr",
    artist: "1.5fr",
    album: "1.5fr",
    composer: "1.5fr",
    album_artist: "1.5fr",
    format: "64px",
    year: "60px",
    originalyear: "60px",
    genre: "1.2fr",
    grouping: "1.2fr",
    bpm: "60px",
    initial_key: "60px",
    bitrate: "70px",
    samplerate: "75px",
    bitdepth: "65px",
    channels: "70px",
    filesize: "75px",
    rating: "96px",
    playcount: "70px",
    skipcount: "70px",
    lastplayed: "90px",
    added: "90px",
    duration: "80px",
    path: "2fr",
    library: "130px",
    actions: "80px",
  };

  let editingSongId = $state<number | null>(null);
  /** A multi-song selection open in the batch tag editor. */
  let batchEditSongs = $state<Song[] | null>(null);

  let isEditingTitle = $state(false);
  let editTitleValue = $state("");

  let selectedUuids = $state<Set<string>>(new Set());

  let contextMenuState = $state<{ x: number; y: number; item: PlaylistItem } | null>(null);

  // Overflow ("more actions") menu state — houses the lower-frequency playlist
  // actions (import/export/duplicate-cleanup/clear/delete) so the toolbar
  // isn't a wall of always-visible buttons.
  let showOverflowMenu = $state(false);
  let overflowMenuPos = $state<{ x: number; y: number } | null>(null);
  let overflowButtonEl = $state<HTMLButtonElement | undefined>(undefined);

  let showSaveQueueModal = $state(false);
  let saveQueueName = $state(i18n.t("playlists.queuePlaylistDefaultName"));

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
    if (activePlaylist && capabilities.renamable) {
      editTitleValue = activePlaylist.name;
      isEditingTitle = true;
    }
  }

  async function saveRename() {
    if (!isEditingTitle) return;
    if (
      activePlaylist &&
      editTitleValue.trim() !== "" &&
      editTitleValue.trim() !== activePlaylist.name
    ) {
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
        filters: [
          {
            name: "Playlists (*.m3u, *.m3u8, *.pls, *.xspf)",
            extensions: ["m3u", "m3u8", "pls", "xspf"],
          },
        ],
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

  $effect(() => {
    if (
      navigationStore.selectedPlaylistId !== null &&
      navigationStore.selectedPlaylistId !== playlistsStore.activePlaylistId
    ) {
      playlistsStore.selectPlaylist(navigationStore.selectedPlaylistId);
    }
  });

  let activePlaylist = $derived(
    playlistsStore.playlists.find(
      (p) => p.id === (navigationStore.selectedPlaylistId ?? playlistsStore.activePlaylistId),
    ),
  );

  let isActive = $derived(
    activePlaylist !== undefined && playlistsStore.effectivePinnedPlaylistId === activePlaylist.id,
  );

  let capabilities = $derived(getPlaylistCapabilities(activePlaylist?.kind));

  let showShareModal = $state(false);
  let shareSongs = $derived(
    playlistsStore.activePlaylistTracks.filter((t) => !!t.song).map((t) => t.song!),
  );

  let showStationListEditModal = $state(false);
  let stationListUpdatedLabel = $derived(
    capabilities.showsStationUpdated
      ? formatUpdatedAgo(radioStore.status?.lastSyncedAt ?? null)
      : "",
  );

  // Title shown everywhere (header, sidebar, Home): a smart playlist's carries its
  // population mode ("Canadian Deep Cuts"). Display only: rename edits the bare name.
  let displayTitle = $derived(getPlaylistDisplayName(activePlaylist));

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
  // A playlist's sort is part of the playlist: it's persisted, and the
  // backend returns the tracks already in that order, so playback, export,
  // the size limit and device sync follow what's on screen (#344). The
  // Queue's order is the player's playback order, so its view sorts here
  // only and forgets the sort on leaving.
  let queueSortField = $state<PlaylistSortField>("position");
  let queueSortAsc = $state(true);
  let sortField = $derived<PlaylistSortField>(
    capabilities.usesLocalSort
      ? queueSortField
      : ((activePlaylist?.sort_field as keyof Song | null | undefined) ?? "position"),
  );
  let sortAsc = $derived(
    capabilities.usesLocalSort ? queueSortAsc : (activePlaylist?.sort_asc ?? true),
  );

  function toggleSort(field: string) {
    const f = field as PlaylistSortField;
    const asc = sortField === f ? !sortAsc : true;
    if (capabilities.usesLocalSort) {
      queueSortField = f;
      queueSortAsc = asc;
    } else if (activePlaylist) {
      playlistsStore.setSort(activePlaylist.id, f === "position" ? null : f, asc);
    }
  }

  let isSearching = $derived(!!collectionStore.searchQuery.trim());

  let sortedTracks = $derived.by(() => {
    let result = playlistsStore.activePlaylistTracks;
    const q = collectionStore.searchQuery.trim();
    if (q) {
      result = result.filter((item) => matchesSongSearch(item.song, q));
    }
    if (!capabilities.usesLocalSort) return result;

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

  // Sample up to 24 covers (beyond the 5 mosaic slots) so failed remote
  // station logos can fall back to the next working logo without placeholders.
  let topAlbums = $derived(
    songsToCoverStack(
      playlistsStore.activePlaylistTracks.filter((t) => !!t.song).map((t) => t.song!),
      24,
    ),
  );

  let allLiveStreams = $derived(
    playlistsStore.activePlaylistTracks.length > 0 &&
      playlistsStore.activePlaylistTracks.every((item) => !!item.song && isLiveSong(item.song)),
  );

  let totalRuntimeLabel = $derived.by(() => {
    const totalNs = playlistsStore.activePlaylistTracks.reduce(
      (sum, item) => sum + (item.song?.length_nanosec ?? 0),
      0,
    );
    return formatHoursMinutes(Math.floor(totalNs / 1_000_000_000 / 60));
  });

  let rawGenre = $derived.by(() => {
    const counts = new Map<string, number>();
    for (const item of playlistsStore.activePlaylistTracks) {
      if (!item.song?.genre || isLiveSong(item.song)) continue;
      for (const g of parseMultiValue(item.song.genre)) {
        const trimmed = g.trim();
        if (trimmed) {
          counts.set(trimmed, (counts.get(trimmed) ?? 0) + 1);
        }
      }
    }
    if (counts.size === 0) return "";
    const sorted = [...counts.entries()]
      .sort((a, b) => b[1] - a[1] || a[0].localeCompare(b[0]))
      .map(([g]) => g);
    return joinMultiValue(sorted);
  });

  let genreLabel = $derived(rawGenre ? undefined : i18n.t("playlists.unknownGenre"));

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
  // Per-row lookups for `itemToRow`, built once per list rather than searched per row (#1898).
  let duplicateUuidSet = $derived(new Set(duplicateUuids));
  let indexByUuid = $derived(
    new Map(playlistsStore.activePlaylistTracks.map((t, i) => [t.uuid, i])),
  );

  // The divider only means something in playlist order: sorted or filtered,
  // Auto Continue's songs are no longer a contiguous run.
  let autoContinueStarts = $derived(
    capabilities.showsAutoContinue &&
      sortField === "position" &&
      sortAsc &&
      !collectionStore.searchQuery.trim()
      ? autoContinueRunStarts(playlistsStore.activePlaylistTracks)
      : new Set<string>(),
  );

  function itemToRow(item: PlaylistItem): SongTableRow {
    const trueUnavailable = isItemUnavailable(item);
    const disconnected =
      !trueUnavailable && collectionStore.isPathOnDisconnectedDrive(item.song?.path);
    return {
      key: item.uuid,
      song: item.song,
      disabled: trueUnavailable || disconnected,
      disabledTooltip: trueUnavailable
        ? i18n.t(
            isItemMissingFromLibrary(item)
              ? "playlists.notInLibraryTooltip"
              : "playlists.fileNotFoundTooltip",
          )
        : disconnected
          ? i18n.t("collection.driveDisconnectedTooltip")
          : undefined,
      disabledVariant: trueUnavailable ? "strikethrough" : "dim",
      isDuplicate: duplicateUuidSet.has(item.uuid),
      underlyingIndex: indexByUuid.get(item.uuid) ?? -1,
      ...(autoContinueStarts.has(item.uuid)
        ? { dividerBefore: i18n.t("playlists.autoContinue") }
        : item.uuid === breakBefore
          ? { dividerBefore: sizeLimit.breakLabel() ?? undefined, dividerVariant: "break" as const }
          : {}),
    };
  }

  let tableRows = $derived(sortedTracks.map(itemToRow));
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
    if (!activePlaylist || !capabilities.deletable) return;
    showDeleteConfirm = false;
    await playlistsStore.deletePlaylist(activePlaylist.id);
    navigationStore.selectedPlaylistId = null;
  }

  /** Plays the playlist in place, starting at the clicked row (looked up by
   * item uuid on the backend). The Queue is left as it is. */
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
    playlistsStore.activePlaylistTracks.filter((item) => isItemUnavailable(item)).length,
  );

  /** True for a song item whose library song is gone (deleted, or never
   * matched on import): the backend renders it from a metadata snapshot
   * with no song id, and only these can be relinked. */
  function isItemMissingFromLibrary(item: PlaylistItem): boolean {
    return item.item_type === "song" && !item.song?.id;
  }

  let missingFromLibraryCount = $derived(
    playlistsStore.activePlaylistTracks.filter((item) => isItemMissingFromLibrary(item)).length,
  );

  async function relinkMissingTracks() {
    if (playlistsStore.activePlaylistId === null) return;
    try {
      const relinked = await playlistsStore.relinkMissingItems(playlistsStore.activePlaylistId);
      if (relinked > 0) {
        toastStore.show(i18n.plural("playlists.relinkMissingToast", relinked), "success");
      } else {
        toastStore.show(i18n.t("playlists.relinkMissingNone"), "info");
      }
    } catch (e) {
      toastStore.show(String(e), "error");
    }
  }

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

  /** Dropping on a row below the dragged song puts it after that row; on a
   * row above, before it. In a sorted playlist the backend keeps the sorted
   * order as the new manual order. */
  function handleReorder(fromIndex: number, toIndex: number, selectedRowKeys: string[]) {
    if (!activePlaylist) return;
    const tracks = playlistsStore.activePlaylistTracks;
    const target = tracks[toIndex];
    const moving = new Set(selectedRowKeys.filter(Boolean));
    if (!target || moving.size === 0 || moving.has(target.uuid)) return;
    const beforeUuid =
      fromIndex < toIndex
        ? (tracks.slice(toIndex + 1).find((t) => !moving.has(t.uuid))?.uuid ?? null)
        : target.uuid;
    playlistsStore.moveItems(activePlaylist.id, [...moving], beforeUuid);
  }

  const sizeLimit = new PlaylistSizeLimit(() => ({
    playlist: capabilities.hasSizeLimit ? activePlaylist : undefined,
    items: playlistsStore.activePlaylistTracks,
  }));
  let breakBefore = $derived(sizeLimit.breakBefore(sortedTracks.map((t) => t.uuid)));

  function playSelected() {
    if (selectedUuids.size === 0 || !activePlaylist) return;
    const selectedTracks = playlistsStore.activePlaylistTracks.filter(
      (t) => selectedUuids.has(t.uuid) && t.song && !isItemUnavailable(t),
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

  function openSelectedInPicard(fallbackSongId?: number) {
    const selectedTracks = playlistsStore.activePlaylistTracks.filter(
      (t) => selectedUuids.has(t.uuid) && t.song,
    );
    const songIds =
      selectedTracks.length > 0
        ? selectedTracks.map((t) => t.song!.id)
        : fallbackSongId !== undefined
          ? [fallbackSongId]
          : [];
    openInPicard(songIds);
  }

  async function handlePlayAll() {
    if (!activePlaylist || playlistsStore.activePlaylistTracks.length === 0) return;
    const availableTracks = playlistsStore.activePlaylistTracks.filter(
      (t) => t.song && !isItemUnavailable(t),
    );
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

  // Population-mode tabs and Refresh apply to dynamic (smart) playlists only:
  // a hand-curated playlist has no population mode to re-roll. The backend's
  // set-mode command repopulates; Refresh re-rolls under the current mode.
  let isChangingMode = $state(false);
  let isRefreshing = $state(false);
  let showSizeLimitPopover = $state(false);

  async function handleChangePopulationMode(mode: QueuePopulationMode) {
    if (!activePlaylist || mode === activePlaylist.population_mode || isChangingMode) return;
    isChangingMode = true;
    try {
      await playlistsStore.setPlaylistPopulationMode(activePlaylist.id, mode);
    } catch (err) {
      console.error("Failed to change population mode:", err);
    } finally {
      isChangingMode = false;
    }
  }

  async function handleRefreshSmartPlaylist() {
    if (!activePlaylist || isRefreshing) return;
    isRefreshing = true;
    try {
      await playlistsStore.refreshAutoPlaylist(activePlaylist.id);
    } catch (err) {
      console.error("Failed to refresh smart playlist:", err);
    } finally {
      isRefreshing = false;
    }
  }

  function handleSaveQueueAsCustomPlaylist() {
    if (playlistsStore.activePlaylistTracks.length === 0) return;
    const songIds = playlistsStore.activePlaylistTracks
      .filter((t) => t.song)
      .map((t) => t.song!.id);
    if (songIds.length === 0) return;

    saveQueueName = i18n.t("playlists.queuePlaylistDefaultName");
    showSaveQueueModal = true;
  }

  async function confirmSaveQueueAsCustomPlaylist() {
    if (!saveQueueName || !saveQueueName.trim()) return;
    const songIds = playlistsStore.activePlaylistTracks
      .filter((t) => t.song)
      .map((t) => t.song!.id);
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

  let songTable = $state<ReturnType<typeof SongTable> | undefined>(undefined);

  // Consumes navigationStore.pendingScrollToCurrentSong (set by the playbar's
  // Queue button) once this view is showing the queue with its tracks loaded,
  // scrolling the now-playing row into view instead of leaving the user at
  // whatever position rememberScroll restored.
  $effect(() => {
    if (!navigationStore.pendingScrollToCurrentSong) return;
    if (!capabilities.isQueue || !songTable) return;
    const uuid = playerStore.playlistItemUuid;
    const tracks = playlistsStore.activePlaylistTracks;
    if (tracks.length === 0) return;
    const current = uuid
      ? tracks.find((t) => t.uuid === uuid)
      : tracks.find(
          (t) => t.song && playerStore.currentSong && t.song.id === playerStore.currentSong.id,
        );
    navigationStore.pendingScrollToCurrentSong = false;
    if (current) songTable.scrollToKey(current.uuid);
  });

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
  {#if collectionStore.searchQuery.trim()}
    <div class="py-12">
      <SearchEmptyState
        icon={Music}
        title={i18n.t("collection.noSongsTitle")}
        matchQueryText={i18n.t("collection.noTracksMatchQuery")}
        query={collectionStore.searchQuery}
        onReset={() => {
          collectionStore.searchQuery = "";
          collectionStore.search("");
        }}
      />
    </div>
  {:else}
    <div class="py-12 text-center text-brand-text-primary/45 select-none">
      <ListMusic class="w-12 h-12 mx-auto mb-2 text-brand-text-primary/30" />
      {#if capabilities.isQueue}
        {i18n.t("playlists.emptyQueueText")}
      {:else}
        {i18n.t("playlists.emptyPlaylistTitle")}
      {/if}
    </div>
  {/if}
{/snippet}

<svelte:window onkeydown={handleDeleteKey} />

<div
  class="flex-1 flex flex-col overflow-hidden bg-brand-main text-brand-text-secondary h-full relative select-none"
>
  {#if currentCoverUrl && capabilities.isQueue}
    <div class="absolute inset-0 pointer-events-none select-none z-0 overflow-hidden">
      {#key currentCoverUrl}
        <div class="absolute inset-0" in:fade={{ duration: 300 }}>
          <BlurredCover
            src={currentCoverUrl}
            class="w-full h-full"
            style="transform: scale(1.5);"
          />
        </div>
      {/key}
      <div
        class="absolute inset-0 bg-gradient-to-b from-transparent via-transparent to-brand-main"
      ></div>
    </div>
  {/if}

  {#if activePlaylist}
    <div
      class="flex-1 flex flex-col min-h-0 relative z-10 overflow-y-auto"
      use:rememberScroll={`playlist:${playlistsStore.activePlaylistId}`}
    >
      <div
        class="relative z-30 w-full overflow-hidden border-b border-brand-border/60 acrylic-chrome-tint [--acrylic-tint:var(--color-brand-main)] px-6 {windowLayoutStore.isDetailHeaderCollapsed
          ? 'py-3'
          : 'pt-6 pb-6'} shrink-0"
      >
        <div class="flex items-start justify-between gap-6 relative z-10">
          <div class="flex flex-col justify-end gap-1.5 min-w-0 flex-1">
            {#if !windowLayoutStore.isDetailHeaderCollapsed}
              {#if isEditingTitle}
                <div class="flex items-center gap-2">
                  <input
                    bind:value={editTitleValue}
                    onkeydown={(e) => {
                      if (e.key === "Enter") saveRename();
                      else if (e.key === "Escape") cancelRename();
                    }}
                    class="bg-brand-sidebar border border-brand-accent text-brand-text-primary px-3 py-1 text-2xl font-bold rounded-lg focus:outline-none"
                    use:focusAndSelect
                  />
                  <button
                    onclick={saveRename}
                    class="p-1.5 text-brand-accent-text hover:text-brand-accent"
                    title={i18n.t("playlists.saveRenameTooltip")}
                  >
                    <Check class="w-5 h-5" />
                  </button>
                  <button
                    onclick={cancelRename}
                    class="p-1.5 text-brand-text-secondary hover:text-brand-text-primary"
                    title={i18n.t("playlists.cancel")}
                  >
                    <X class="w-5 h-5" />
                  </button>
                </div>
              {:else}
                <div class="flex items-center gap-3 group/title">
                  <h1
                    ondblclick={capabilities.renamable ? startRename : undefined}
                    class="text-3xl @xl:text-4xl font-heading font-bold text-brand-text-primary transition-colors truncate py-0.5 leading-snug {capabilities.renamable
                      ? 'hover:text-brand-accent-text'
                      : ''}"
                    title={capabilities.renamable
                      ? i18n.t("playlists.renamePlaylistTooltip")
                      : undefined}
                  >
                    {displayTitle}
                  </h1>
                  {#if capabilities.renamable}
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

              <div
                class="flex flex-wrap items-center gap-x-2 gap-y-1 text-xs text-brand-text-primary font-medium"
              >
                <span>
                  {formatPlaylistItemCount(
                    activePlaylist,
                    playlistsStore.activePlaylistTracks.length,
                  )}
                </span>
                {#if !allLiveStreams}
                  <span>•</span>
                  <span>{totalRuntimeLabel}</span>
                {/if}
                {#if sizeLimit.status}
                  <span>•</span>
                  <span>{sizeLimit.status}</span>
                {/if}
              </div>
            {/if}

            <div
              class="flex flex-wrap items-center gap-3 {windowLayoutStore.isDetailHeaderCollapsed
                ? ''
                : 'mt-3'}"
            >
              <Button
                onclick={handlePlayAll}
                disabled={playlistsStore.activePlaylistTracks.length === 0}
                variant="primary"
              >
                <Play class="w-4 h-4 fill-current" />
                {i18n.t("artistDetail.playAll")}
              </Button>
              {#if capabilities.showsAutoContinue}
                <Button
                  onclick={() => playerStore.setAutoContinue(!playerStore.autoContinue)}
                  variant={playerStore.autoContinue ? "accent-soft" : "secondary"}
                  pressed={playerStore.autoContinue}
                  title={`${i18n.t("playlists.autoContinue")}: ${playerStore.autoContinue ? i18n.t("common.on") : i18n.t("common.off")}`}
                  aria-label={i18n.t("playlists.autoContinue")}
                  class="transition-all {playerStore.autoContinue
                    ? ''
                    : 'text-brand-text-secondary hover:text-brand-text-primary'}"
                >
                  <Infinity class="w-4 h-4 shrink-0" />
                  <span>{i18n.t("playlists.autoContinue")}</span>
                  <span
                    class="relative inline-flex h-4 w-7 shrink-0 items-center rounded-full p-0.5 transition-colors duration-200 ease-out {playerStore.autoContinue
                      ? 'bg-brand-accent'
                      : 'bg-white/20'}"
                    aria-hidden="true"
                  >
                    <span
                      class="pointer-events-none inline-block h-3 w-3 rounded-full bg-white shadow-xs transition-transform duration-200 ease-out {playerStore.autoContinue
                        ? 'translate-x-3'
                        : 'translate-x-0'}"
                    ></span>
                  </span>
                </Button>
              {/if}
              {#if capabilities.canSaveAsCustom}
                <IconActionButton
                  onclick={handleSaveQueueAsCustomPlaylist}
                  disabled={playlistsStore.activePlaylistTracks.length === 0}
                  title={i18n.t("playlists.saveQueueAsPlaylist", {}, "Save as Custom Playlist")}
                  class="shrink-0"
                >
                  {#snippet icon()}<FolderPlus class="w-4 h-4" />{/snippet}
                </IconActionButton>
              {/if}
              {#if capabilities.pinnableToHome && activePlaylist}
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
              {#if capabilities.shareable}
                <IconActionButton
                  onclick={() => {
                    showShareModal = true;
                  }}
                  title={i18n.t("shareModal.menuItem")}
                  class="shrink-0"
                >
                  {#snippet icon()}<Share class="w-4 h-4" />{/snippet}
                </IconActionButton>
              {/if}
              <button
                bind:this={overflowButtonEl}
                onclick={toggleOverflowMenu}
                title={i18n.t("playlists.moreActionsTooltip")}
                class="flex items-center justify-center w-10 h-10 rounded-full border border-brand-border text-brand-text-secondary hover:text-brand-accent-text hover:bg-brand-sidebar transition-colors shadow-xs shrink-0 cursor-pointer"
              >
                <MoreIcon class="w-4 h-4" />
              </button>
            </div>

            {#if capabilities.showsPopulationMode && activePlaylist && !windowLayoutStore.isDetailHeaderCollapsed}
              <div class="flex flex-wrap items-center gap-2.5 mt-2.5 select-none relative z-40">
                <PopulationModeTabs
                  mode={activePlaylist.population_mode ?? "all"}
                  disabled={isChangingMode}
                  onChange={handleChangePopulationMode}
                />
              </div>
            {:else if capabilities.showsStationUpdated && stationListUpdatedLabel && !windowLayoutStore.isDetailHeaderCollapsed}
              <div
                class="text-xs text-brand-text-secondary mt-2"
                data-testid="station-list-detail-updated"
              >
                {stationListUpdatedLabel}
              </div>
            {/if}
          </div>

          {#if !windowLayoutStore.isDetailHeaderCollapsed}
            {#if capabilities.artworkStyle === "queue"}
              <div
                class="w-40 h-40 hidden @xl:flex shrink-0 bg-brand-main bg-gradient-to-br from-brand-accent/25 to-brand-accent/15 items-center justify-center overflow-hidden border border-brand-accent/30 shadow-[0_0_28px_3px] shadow-brand-accent/40"
              >
                {#key playerStore.currentSong?.id}
                  <div class="w-full h-full" in:fade={{ duration: 200 }}>
                    {#if playerStore.currentSong}
                      <CoverArt
                        songId={playerStore.currentSong.id}
                        artEmbedded={playerStore.currentSong.art_embedded}
                        artAutomatic={playerStore.currentSong.art_automatic}
                        artManual={playerStore.currentSong.art_manual}
                        sizeClass="w-full h-full"
                        contain={isLiveSong(playerStore.currentSong)}
                      />
                    {:else}
                      <div class="w-full h-full flex items-center justify-center">
                        <Layers class="w-16 h-16 text-brand-accent-text" />
                      </div>
                    {/if}
                  </div>
                {/key}
              </div>
            {:else if capabilities.artworkStyle === "smart" && topAlbums.length > 0}
              <div
                class="w-40 h-40 hidden @xl:flex shrink-0 bg-brand-main bg-gradient-to-br from-[#C2410C]/25 to-[#F59E0B]/15 items-center justify-center overflow-hidden border border-[#F59E0B]/30 shadow-[0_0_28px_3px_rgba(245,158,11,0.4)]"
              >
                <CoverStack covers={topAlbums} sizeClass="w-[82%] h-[82%]" />
              </div>
            {:else if capabilities.artworkStyle === "smart"}
              <div
                class="w-40 h-40 hidden @xl:flex shrink-0 bg-brand-main bg-gradient-to-br from-[#C2410C]/25 to-[#F59E0B]/15 items-center justify-center overflow-hidden border border-[#F59E0B]/30 shadow-[0_0_28px_3px_rgba(245,158,11,0.4)]"
              >
                <Sparkles class="w-16 h-16 text-[#F59E0B]" />
              </div>
            {:else if topAlbums.length > 0}
              <div class="hidden @xl:flex items-start shrink-0 shadow-xl">
                <CoverMosaic covers={topAlbums} sizeClass="h-36" />
              </div>
            {/if}
          {/if}
        </div>
      </div>

      <div class="p-6 flex flex-col gap-6 pb-dock">
        {#if capabilities.showsGenreHeader && !windowLayoutStore.isDetailHeaderCollapsed}
          <div class="flex flex-wrap items-center gap-3">
            {#if rawGenre}
              <GenreChips genre={rawGenre} variant="full" limit={4} />
            {:else}
              <div
                class="inline-flex items-center px-3 py-1 rounded-full border border-brand-border bg-brand-sidebar text-xs text-brand-text-secondary italic"
              >
                <span>{genreLabel}</span>
              </div>
            {/if}
          </div>
        {/if}
        {#if activePlaylist?.source_conflict}
          <!-- Both the playlist and its file changed (#2280): nothing is written
               until the user keeps one side. -->
          <div
            class="mb-3 px-4 py-3 rounded-xl border border-brand-border/60 acrylic-chrome flex flex-wrap items-center gap-3"
            data-testid="playlist-file-conflict"
          >
            <InlineMessage class="flex-1 min-w-60">
              <span title={activePlaylist.source_path}>
                {i18n.t("playlists.fileConflictBanner", {
                  path: (activePlaylist.source_path ?? "").split(/[\\/]/).pop() ?? "",
                })}
              </span>
            </InlineMessage>
            <div class="flex gap-2">
              <Button
                size="sm"
                onclick={() =>
                  activePlaylist &&
                  playlistsStore.resolveFileConflict(activePlaylist.id, "luminous")}
              >
                {i18n.t("playlists.fileConflictKeepLuminous")}
              </Button>
              <Button
                size="sm"
                onclick={() =>
                  activePlaylist && playlistsStore.resolveFileConflict(activePlaylist.id, "file")}
              >
                {i18n.t("playlists.fileConflictKeepFile")}
              </Button>
            </div>
          </div>
        {/if}
        <div
          class="border border-brand-border/60 rounded-xl acrylic-chrome relative overflow-hidden"
        >
          <SongTable
            bind:this={songTable}
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
            onRowDoubleClick={(row) =>
              activePlaylist && playerStore.playPlaylistItemByUuid(activePlaylist.id, row.key)}
            onRowContextMenu={handleRowContextMenu}
            onRate={rateSong}
            onEditTags={(song) => openTagEditor(song.id)}
            onRemoveFromPlaylist={(row) => handleRemoveItem(row.key)}
            onReorder={isSearching ? undefined : handleReorder}
            dragSourcePlaylistId={activePlaylist?.id ?? null}
            isRowPlaying={(row) =>
              (!!playerStore.playlistItemUuid && playerStore.playlistItemUuid === row.key) ||
              (!!playerStore.currentSong &&
                !!row.song &&
                playerStore.currentSong.id === row.song.id)}
            interactiveWhenDisabled
            disabledPlaceholder
          />
        </div>
      </div>
    </div>

    {#if selectedUuids.size > 0}
      <div
        data-floating-toolbar="true"
        class="absolute left-1/2 -translate-x-1/2 bottom-dock z-40 acrylic-float border border-brand-border/80 shadow-2xl rounded-full px-5 py-2.5 flex items-center gap-4 text-xs font-semibold"
      >
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
          class="flex items-center gap-1.5 text-brand-text-primary hover:text-brand-text-secondary transition-colors"
        >
          <Trash2 class="w-3.5 h-3.5" />
          <span>{i18n.t("playlists.removeSelected")}</span>
        </button>
        <div class="h-4 w-px bg-brand-border/60"></div>
        <button
          onclick={() => {
            selectedUuids = new Set();
          }}
          class="text-brand-text-primary hover:text-brand-text-primary transition-colors"
        >
          {i18n.t("playlists.clearSelection")}
        </button>
      </div>
    {/if}
  {:else}
    <div class="flex-1 flex flex-col items-center justify-center text-brand-text-primary/60">
      <ListMusic class="w-16 h-16 mb-4 text-brand-text-primary/30" />
      <h2 class="text-lg font-bold text-brand-text-primary/80 mb-1">
        {i18n.t("playlists.noPlaylistsTitle")}
      </h2>
      <p class="text-sm">{i18n.t("playlists.noPlaylistsText")}</p>
    </div>
  {/if}
</div>

{#if contextMenuState}
  {@const singleItem = contextMenuState.item}
  {@const selectedTracks = playlistsStore.activePlaylistTracks.filter(
    (t) => selectedUuids.has(t.uuid) && t.song,
  )}
  {@const batchEditable = [
    ...new Map(
      selectedTracks.filter((t) => !isItemUnavailable(t)).map((t) => [t.song!.id, t.song!]),
    ).values(),
  ]}
  {@const picardTargets =
    selectedTracks.length > 0
      ? selectedTracks.map((t) => t.song!)
      : singleItem.song
        ? [singleItem.song]
        : []}
  <PlaylistContextMenu
    x={contextMenuState.x}
    y={contextMenuState.y}
    selectedCount={selectedUuids.size}
    songId={singleItem.song?.id}
    onPlay={playSelected}
    onRemove={removeSelected}
    onGoToArtist={singleItem.song?.artist
      ? () =>
          navigationStore.viewArtist(
            singleItem.song?.album_artist?.trim() || singleItem.song?.artist || "",
          )
      : undefined}
    onGoToAlbum={singleItem.song?.album
      ? () => navigationStore.viewAlbum(singleItem.song?.album || "")
      : undefined}
    onEditTags={singleItem.song?.id && !isItemUnavailable(singleItem)
      ? () => openTagEditor(singleItem.song!.id)
      : undefined}
    onEditBatchTags={batchEditable.length > 1
      ? () => {
          batchEditSongs = batchEditable;
        }
      : undefined}
    onOpenInPicard={singleItem.song?.id
      ? () => openSelectedInPicard(singleItem.song!.id)
      : undefined}
    allSelectedRemote={picardTargets.length > 0 && picardTargets.every(isRemoteSource)}
    onClose={() => {
      contextMenuState = null;
    }}
  />
{/if}

{#if showOverflowMenu && overflowMenuPos && activePlaylist}
  <ContextMenu
    x={overflowMenuPos.x}
    y={overflowMenuPos.y}
    onClose={() => {
      showOverflowMenu = false;
    }}
  >
    <ContextMenuItem
      icon={RotateCcw}
      label={i18n.t("playlists.undoBtn")}
      onclick={() => {
        playlistsStore.undo();
        showOverflowMenu = false;
      }}
    />
    <ContextMenuItem
      icon={RotateCw}
      label={i18n.t("playlists.redoBtn")}
      onclick={() => {
        playlistsStore.redo();
        showOverflowMenu = false;
      }}
    />
    {#if capabilities.refreshable}
      <ContextMenuDivider />
      <ContextMenuItem
        icon={RefreshCw}
        label={i18n.t("playlists.refreshPlaylistBtn")}
        disabled={isRefreshing}
        onclick={() => {
          handleRefreshSmartPlaylist();
          showOverflowMenu = false;
        }}
      />
      {#if capabilities.editableDefinition === "smart"}
        <ContextMenuItem
          icon={Pencil}
          label={i18n.t("playlists.editSmartPlaylistBtn")}
          onclick={() => {
            handleEditSmartPlaylist();
            showOverflowMenu = false;
          }}
        />
      {:else if capabilities.editableDefinition === "station_list"}
        <ContextMenuItem
          icon={Pencil}
          label={i18n.t("radio.editStationList")}
          onclick={() => {
            showStationListEditModal = true;
            showOverflowMenu = false;
          }}
        />
      {/if}
    {/if}
    {#if capabilities.hasSizeLimit}
      {#if !capabilities.refreshable}<ContextMenuDivider />{/if}
      <ContextMenuItem
        icon={Usb}
        label={i18n.t("playlists.sizeLimitMenuItem", {
          value: activePlaylist.size_limit_bytes
            ? formatStorageSize(activePlaylist.size_limit_bytes)
            : i18n.t("playlists.sizeLimitNotSet"),
        })}
        onclick={() => {
          showSizeLimitPopover = true;
          showOverflowMenu = false;
        }}
      />
      {#if !isActive && capabilities.canMakeActive}
        <ContextMenuItem
          icon={Radio}
          label={i18n.t("playlists.makeActiveBtn")}
          onclick={() => {
            playlistsStore.pinPlaylist(activePlaylist.id);
            showOverflowMenu = false;
          }}
        />
      {/if}
    {/if}
    <ContextMenuDivider />
    <ContextMenuItem
      icon={FolderInput}
      label={i18n.t("playlists.importPlaylistBtn")}
      onclick={() => {
        handleImportPlaylist();
        showOverflowMenu = false;
      }}
    />
    <ContextMenuItem
      icon={FileOutput}
      label={i18n.t("playlists.exportPlaylistBtn")}
      onclick={() => {
        showExportOptionsModal = true;
        showOverflowMenu = false;
      }}
    />

    {#if duplicateCount > 0 || unavailableCount > 0}
      <ContextMenuDivider />
      {#if duplicateCount > 0}
        <ContextMenuItem
          icon={CopyPlus}
          label={i18n.plural("playlists.removeDuplicatesBtn", duplicateCount)}
          onclick={() => {
            removeDuplicates();
            showOverflowMenu = false;
          }}
        />
      {/if}
      {#if missingFromLibraryCount > 0}
        <ContextMenuItem
          icon={Link}
          label={i18n.t("playlists.relinkMissingBtn", { count: missingFromLibraryCount })}
          onclick={() => {
            relinkMissingTracks();
            showOverflowMenu = false;
          }}
        />
      {/if}
      {#if unavailableCount > 0}
        <ContextMenuItem
          icon={AlertTriangle}
          label={i18n.t("playlists.removeUnavailableBtn", { count: unavailableCount })}
          onclick={() => {
            removeUnavailableTracks();
            showOverflowMenu = false;
          }}
        />
      {/if}
    {/if}

    <ContextMenuDivider />
    <ContextMenuItem
      icon={Eraser}
      label={i18n.t("playlists.clearPlaylistBtn")}
      onclick={() => {
        playlistsStore.clearPlaylist(activePlaylist.id);
        showOverflowMenu = false;
      }}
    />
    {#if capabilities.deletable}
      <ContextMenuItem
        icon={Trash2}
        destructive
        label={i18n.t("playlists.deletePlaylistBtn")}
        onclick={() => {
          showDeleteConfirm = true;
          showOverflowMenu = false;
        }}
      />
    {/if}
  </ContextMenu>
{/if}

{#if activePlaylist && capabilities.hasSizeLimit}
  <PlaylistSizeLimitPopover
    isOpen={showSizeLimitPopover}
    anchorEl={overflowButtonEl ?? null}
    limitBytes={activePlaylist.size_limit_bytes ?? null}
    deviceId={activePlaylist.size_limit_device ?? null}
    deviceLabel={sizeLimit.deviceName}
    refreshReorders={activePlaylist.dynamic_enabled && !activePlaylist.sort_field}
    onApply={(bytes, deviceId) => playlistsStore.setSizeLimit(activePlaylist.id, bytes, deviceId)}
    onClose={() => (showSizeLimitPopover = false)}
  />
{/if}

{#if showShareModal && activePlaylist}
  <ShareModal
    entity={{ kind: "playlist", title: displayTitle, songs: shareSongs }}
    onClose={() => {
      showShareModal = false;
    }}
  />
{/if}

{#if editingSongId !== null}
  <TagEditor
    songId={editingSongId}
    onClose={() => {
      editingSongId = null;
    }}
    onSave={handleTagEditorSaved}
  />
{/if}

{#if batchEditSongs !== null}
  <AlbumTagEditor
    title={i18n.t("collection.editTagsTooltip")}
    songIds={batchEditSongs.map((s) => s.id)}
    initialTracks={batchEditSongs}
    hasEmbeddedArt={batchEditSongs.some((s) => s.art_embedded)}
    onClose={() => {
      batchEditSongs = null;
    }}
    onSave={handleTagEditorSaved}
  />
{/if}

{#if showDeleteConfirm && activePlaylist}
  <ConfirmDialog
    title={i18n.t("playlists.confirmDeletePlaylistTitle")}
    message={activePlaylist.source_path
      ? i18n.t("playlists.confirmDeleteFilePlaylist", {
          name: activePlaylist.name,
          path: activePlaylist.source_path,
        })
      : i18n.t("playlists.confirmDeletePlaylist", { name: activePlaylist.name })}
    confirmLabel={i18n.t("playlists.deletePlaylistBtn")}
    cancelLabel={i18n.t("playlists.cancel")}
    onConfirm={handleDeletePlaylist}
    onCancel={() => {
      showDeleteConfirm = false;
    }}
  />
{/if}

{#if showExportOptionsModal}
  <div
    use:portal
    class="fixed inset-0 z-50 flex items-center justify-center acrylic-scrim select-none"
  >
    <div
      class="bg-brand-sidebar border border-brand-border rounded-xl p-5 w-96 shadow-2xl space-y-4"
    >
      <h3 class="text-base font-bold text-brand-text-primary flex items-center gap-2">
        <FileOutput class="w-5 h-5 text-brand-accent-text" />
        {i18n.t("playlists.exportModalTitle")}
      </h3>
      <div class="space-y-2.5 text-xs text-brand-text-secondary">
        <label class="flex items-center gap-2 hover:text-brand-text-primary transition-colors">
          <input
            type="radio"
            name="exportPathType"
            bind:group={exportRelative}
            value={true}
            class="accent-brand-accent"
          />
          <span>{i18n.t("playlists.useRelativePaths")}</span>
        </label>
        <label class="flex items-center gap-2 hover:text-brand-text-primary transition-colors">
          <input
            type="radio"
            name="exportPathType"
            bind:group={exportRelative}
            value={false}
            class="accent-brand-accent"
          />
          <span>{i18n.t("playlists.useAbsolutePaths")}</span>
        </label>
      </div>
      <div class="flex justify-end gap-3 pt-2">
        <Button
          onclick={() => {
            showExportOptionsModal = false;
          }}
          variant="secondary"
          size="sm"
        >
          {i18n.t("playlists.cancelBtn")}
        </Button>
        <Button onclick={triggerExport} variant="primary" size="sm">
          {i18n.t("playlists.exportBtn")}
        </Button>
      </div>
    </div>
  </div>
{/if}

{#if showSaveQueueModal}
  <Modal onClose={() => (showSaveQueueModal = false)} maxWidth="max-w-sm">
    <div
      class="h-14 flex items-center justify-between px-6 border-b border-brand-border shrink-0 bg-brand-main"
    >
      <div class="flex items-center gap-2">
        <FolderPlus class="w-4 h-4 text-brand-accent-text" />
        <h3 class="text-sm font-bold text-brand-text-primary">
          {i18n.t("playlists.saveQueueAsPlaylist", {}, "Save as Custom Playlist")}
        </h3>
      </div>
      <button
        onclick={() => (showSaveQueueModal = false)}
        class="text-brand-text-secondary hover:text-brand-text-primary transition-colors"
      >
        <X class="w-4 h-4" />
      </button>
    </div>

    <form
      onsubmit={(e) => {
        e.preventDefault();
        confirmSaveQueueAsCustomPlaylist();
      }}
      class="flex flex-col gap-4 p-6 bg-brand-sidebar"
    >
      <div class="flex flex-col gap-1.5">
        <label for="save-queue-name-input" class="label-field text-brand-text-secondary">
          {i18n.t("playlists.saveQueueNameLabel")}
        </label>
        <Input
          id="save-queue-name-input"
          type="text"
          bind:value={saveQueueName}
          placeholder={i18n.t("playlists.saveQueueNamePlaceholder")}
          class="w-full"
          required
          autofocus
        />
      </div>

      <div class="flex items-center justify-end gap-3 pt-2">
        <Button onclick={() => (showSaveQueueModal = false)} variant="secondary" size="sm">
          {i18n.t("playlists.cancel", {}, "Cancel")}
        </Button>
        <Button type="submit" variant="primary" size="sm">
          {i18n.t("common.save")}
        </Button>
      </div>
    </form>
  </Modal>
{/if}

{#if showStationListEditModal && activePlaylist?.station_list_spec}
  <StationListModal
    editing={{
      id: activePlaylist.id,
      name: activePlaylist.name,
      spec: activePlaylist.station_list_spec,
    }}
    onClose={() => {
      showStationListEditModal = false;
    }}
  />
{/if}
