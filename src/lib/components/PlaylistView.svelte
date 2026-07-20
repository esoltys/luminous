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
    XCircle,
    Music,
    Shuffle,
    Search
  } from "lucide-svelte";
  import { getCoverArtUrl } from "../types";
  import { i18n } from "../stores/i18n.svelte";
  import type { PlaylistItem } from "../types";
  import { invoke } from "@tauri-apps/api/core";
  import { open, save } from "@tauri-apps/plugin-dialog";
  import SongRating from "./SongRating.svelte";
  import TagEditor from "./TagEditor.svelte";
  import CoverArt from "./CoverArt.svelte";
  import PlaylistContextMenu from "./PlaylistContextMenu.svelte";
  import { portal } from "../utils/portal";

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

  // Derived filtered tracks based on filterQuery
  let filteredTracks = $derived.by(() => {
    const q = filterQuery.trim().toLowerCase();
    if (!q) return playlistsStore.activePlaylistTracks;
    return playlistsStore.activePlaylistTracks.filter((item) => {
      const title = item.song?.title?.toLowerCase() ?? "";
      const artist = item.song?.artist?.toLowerCase() ?? "";
      const album = item.song?.album?.toLowerCase() ?? "";
      return title.includes(q) || artist.includes(q) || album.includes(q);
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

  async function handleDeletePlaylist() {
    if (!activePlaylist) return;
    if (!confirm(i18n.t("playlists.confirmDeletePlaylist", { name: activePlaylist.name }))) return;
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
      playerStore.playSongs(songIds, 0);
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
    }, 100);
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
    <div class="flex-1 overflow-y-auto relative z-10">
    <!-- Stacked Cover Art Hero & Summary Banner Header -->
    <div class="relative w-full overflow-hidden border-b border-brand-border/60 bg-brand-main/60 backdrop-blur-md px-6 pt-6 pb-6">
      <div class="flex items-end justify-between gap-6 relative z-10">
        <!-- Left Title & Summary Metadata -->
        <div class="flex flex-col justify-end gap-2 max-w-xl">
          {#if isEditingTitle}
            <div class="flex items-center gap-2">
              <input
                bind:value={editTitleValue}
                onkeydown={(e) => { if (e.key === "Enter") saveRename(); else if (e.key === "Escape") cancelRename(); }}
                class="bg-brand-sidebar border border-brand-accent text-brand-text-primary px-3 py-1 text-2xl font-bold rounded-lg focus:outline-none"
                use:focusAndSelect
              />
              <button onclick={saveRename} class="p-1.5 text-emerald-400 hover:text-emerald-300 cursor-pointer" title="Save">
                <Check class="w-5 h-5" />
              </button>
              <button onclick={cancelRename} class="p-1.5 text-brand-text-secondary hover:text-brand-text-primary cursor-pointer" title="Cancel">
                <X class="w-5 h-5" />
              </button>
            </div>
          {:else}
            <div class="flex items-center gap-3 group/title">
              <h1
                ondblclick={startRename}
                class="text-3xl sm:text-4xl font-extrabold text-brand-text-primary cursor-pointer hover:text-brand-accent-text transition-colors truncate py-0.5 leading-snug"
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
            <button
              onclick={handlePlayAll}
              disabled={playlistsStore.activePlaylistTracks.length === 0}
              class="flex items-center gap-2 px-5 py-2 rounded-full bg-brand-accent hover:bg-brand-accent-hover text-brand-accent-contrast font-semibold text-sm transition-colors cursor-pointer disabled:opacity-50 disabled:cursor-not-allowed shadow-md shadow-brand-accent/20"
            >
              <Play class="w-4 h-4 fill-current" /> {i18n.t("artistDetail.playAll")}
            </button>
            <button
              onclick={handleShufflePlay}
              disabled={playlistsStore.activePlaylistTracks.length === 0}
              class="flex items-center gap-2 px-5 py-2 rounded-full border border-brand-border text-brand-text-primary hover:bg-brand-sidebar font-semibold text-sm transition-colors cursor-pointer disabled:opacity-50 disabled:cursor-not-allowed"
            >
              <Shuffle class="w-4 h-4" /> {i18n.t("artistDetail.shuffleAndPlay")}
            </button>
          </div>
        </div>

        <!-- Right: 3D Stacked Album Cover Preview Header -->
        {#if topAlbums.length > 0}
          <div class="relative w-48 h-36 hidden sm:block shrink-0">
            {#each topAlbums.slice(0, 6) as album, i (i)}
              <div
                class="absolute bottom-0 right-0 w-28 h-28 rounded-xl overflow-hidden border border-brand-border/60 shadow-xl transition-all duration-300"
                style="z-index: {10 - i}; transform: translate({i * -18}px, {i * -10}px) rotate({i * -5}deg) scale({1 - i * 0.05}); opacity: {1 - i * 0.07};"
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

    <!-- Toolbar: Search filter, actions, import/export, undo/redo -->
    <div class="h-14 px-6 border-b border-brand-border/60 flex items-center justify-between relative z-10 bg-brand-main/40 backdrop-blur-md gap-4">
      <!-- Search Filter Bar -->
      <div class="relative flex-1 max-w-xs">
        <Search class="w-3.5 h-3.5 absolute left-3 top-1/2 -translate-y-1/2 text-brand-text-secondary/60 pointer-events-none" />
        <input
          type="text"
          bind:value={filterQuery}
          placeholder={i18n.t("playlists.filterPlaceholder")}
          class="w-full pl-8 pr-7 py-1 text-xs bg-brand-sidebar/60 border border-brand-border/60 rounded-md text-brand-text-primary placeholder:text-brand-text-secondary/50 focus:outline-none focus:border-brand-accent transition-colors"
        />
        {#if filterQuery}
          <button
            onclick={() => { filterQuery = ""; }}
            class="absolute right-2 top-1/2 -translate-y-1/2 text-brand-text-secondary/60 hover:text-brand-text-primary p-0.5 cursor-pointer"
            title={i18n.t("playlists.clearFilter")}
          >
            <X class="w-3 h-3" />
          </button>
        {/if}
      </div>

      <!-- Action buttons -->
      <div class="flex items-center gap-2">
        <!-- Import / Export buttons -->
        <button
          onclick={handleImportPlaylist}
          class="flex items-center gap-1.5 bg-brand-sidebar hover:bg-brand-main border border-brand-border/60 text-brand-text-primary px-2.5 py-1 text-xs font-semibold rounded-md transition-colors cursor-pointer"
          title={i18n.t("playlists.importPlaylistTooltip")}
        >
          <FolderInput class="w-3.5 h-3.5 text-brand-accent-text" />
          <span>{i18n.t("playlists.importPlaylistBtn")}</span>
        </button>

        <button
          onclick={() => { showExportOptionsModal = true; }}
          class="flex items-center gap-1.5 bg-brand-sidebar hover:bg-brand-main border border-brand-border/60 text-brand-text-primary px-2.5 py-1 text-xs font-semibold rounded-md transition-colors cursor-pointer"
          title={i18n.t("playlists.exportPlaylistTooltip")}
        >
          <FileOutput class="w-3.5 h-3.5 text-brand-accent-text" />
          <span>{i18n.t("playlists.exportPlaylistBtn")}</span>
        </button>

        <!-- Undo/Redo controls -->
        <button
          onclick={() => playlistsStore.undo()}
          class="p-1.5 rounded-md hover:bg-brand-sidebar text-brand-text-secondary hover:text-brand-text-primary transition-colors cursor-pointer"
          title={i18n.t("playlists.undoTooltip")}
        >
          <RotateCcw class="w-4 h-4" />
        </button>
        <button
          onclick={() => playlistsStore.redo()}
          class="p-1.5 rounded-md hover:bg-brand-sidebar text-brand-text-secondary hover:text-brand-text-primary transition-colors cursor-pointer"
          title={i18n.t("playlists.redoTooltip")}
        >
          <RotateCw class="w-4 h-4" />
        </button>

        <!-- Deduplicate Button (matches Remove Unavailable button style) -->
        {#if duplicateCount > 0}
          <button
            onclick={removeDuplicates}
            class="flex items-center gap-1.5 bg-purple-500/10 hover:bg-purple-500/20 border border-purple-500/30 text-purple-400 hover:text-purple-300 px-2.5 py-1 text-xs font-semibold rounded-md transition-colors cursor-pointer"
            title={i18n.t("playlists.removeDuplicatesTooltip", { count: duplicateCount })}
          >
            <CopyPlus class="w-3.5 h-3.5" />
            <span>{i18n.t("playlists.removeDuplicatesBtn", { count: duplicateCount })}</span>
          </button>
        {/if}

        <!-- Remove Unavailable Button -->
        {#if unavailableCount > 0}
          <button
            onclick={removeUnavailableTracks}
            class="flex items-center gap-1.5 bg-amber-500/10 hover:bg-amber-500/20 border border-amber-500/30 text-amber-400 hover:text-amber-300 px-2.5 py-1 text-xs font-semibold rounded-md transition-colors cursor-pointer"
            title={i18n.t("playlists.removeUnavailableTooltip", { count: unavailableCount })}
          >
            <AlertTriangle class="w-3.5 h-3.5" />
            <span>{i18n.t("playlists.removeUnavailableBtn", { count: unavailableCount })}</span>
          </button>
        {/if}

        <button
          onclick={() => playlistsStore.clearPlaylist(activePlaylist.id)}
          class="bg-brand-sidebar hover:bg-brand-main border border-brand-border/60 text-brand-text-primary px-2.5 py-1 text-xs font-semibold rounded-md transition-colors cursor-pointer"
          title={i18n.t("playlists.clearPlaylistTooltip")}
        >
          {i18n.t("playlists.clearPlaylistBtn")}
        </button>

        <button
          onclick={handleDeletePlaylist}
          class="flex items-center gap-1.5 bg-red-500/10 hover:bg-red-500/20 border border-red-500/30 text-red-400 hover:text-red-300 px-2.5 py-1 text-xs font-semibold rounded-md transition-colors cursor-pointer"
          title={i18n.t("playlists.deletePlaylistTooltip")}
        >
          <Trash2 class="w-3.5 h-3.5" />
          <span>{i18n.t("playlists.deletePlaylistBtn")}</span>
        </button>
      </div>
    </div>

    <!-- Tracks List Container -->
    <div class="p-6" class:pb-24={playerStore.hasEverPlayed}>
      <div class="border border-brand-border/60 rounded-xl bg-brand-sidebar/30 backdrop-blur-md relative">
        <table class="w-full text-left text-sm border-collapse min-w-[800px]">
          <thead>
            <tr class="text-xs text-brand-text-secondary uppercase tracking-wider font-semibold">
              <th class="sticky top-0 bg-brand-sidebar border-b border-brand-border py-3 px-4 w-12 text-center z-10">{i18n.t("playlists.tableHeaderTrack")}</th>
              <th class="sticky top-0 bg-brand-sidebar border-b border-brand-border py-3 px-4 z-10">{i18n.t("playlists.tableHeaderTitle")}</th>
              <th class="sticky top-0 bg-brand-sidebar border-b border-brand-border py-3 px-4 z-10">{i18n.t("playlists.tableHeaderArtist")}</th>
              <th class="sticky top-0 bg-brand-sidebar border-b border-brand-border py-3 px-4 z-10">{i18n.t("collection.tableHeaderAlbum")}</th>
              <th class="sticky top-0 bg-brand-sidebar border-b border-brand-border py-3 px-4 w-28 text-center z-10">{i18n.t("collection.tableHeaderRating")}</th>
              <th class="sticky top-0 bg-brand-sidebar border-b border-brand-border py-3 px-4 w-24 text-center z-10">{i18n.t("playlists.tableHeaderDuration")}</th>
              <th class="sticky top-0 bg-brand-sidebar border-b border-brand-border py-3 px-4 w-20 text-center z-10">{i18n.t("collection.tableHeaderActions")}</th>
            </tr>
          </thead>
          <tbody>
            {#each filteredTracks as item, index}
              {@const unavailable = isItemUnavailable(item)}
              {@const isDuplicate = duplicateUuids.includes(item.uuid)}
              {@const isSelected = selectedUuids.has(item.uuid)}
              {@const actualIndex = playlistsStore.activePlaylistTracks.findIndex(t => t.uuid === item.uuid)}
              <tr
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
                class="border-b border-brand-border/40 group transition-all duration-150 select-none
                  {unavailable
                    ? 'opacity-50 cursor-not-allowed'
                    : 'cursor-grab active:cursor-grabbing'}
                  {isSelected ? 'bg-brand-accent/20 text-brand-accent-text-hover' : 'hover:bg-brand-sidebar/40'}
                  {!unavailable && !isSelected && playerStore.playlistItemUuid === item.uuid ? 'bg-brand-accent/10 text-brand-accent-text-hover' : ''}
                  {dragOverIndex === actualIndex && draggedIndex !== null && draggedIndex !== actualIndex
                    ? (actualIndex < draggedIndex ? 'border-t-2! border-t-brand-accent bg-brand-accent/5' : 'border-b-2! border-b-brand-accent bg-brand-accent/5')
                    : ''
                  }"
              >
                <td class="py-2.5 px-4 text-center text-brand-text-secondary/50 font-medium w-12 relative cursor-grab active:cursor-grabbing">
                  <div class="relative w-5 h-4 mx-auto flex items-center justify-center">
                    <GripVertical class="w-3.5 h-3.5 opacity-0 group-hover:opacity-60 text-brand-text-secondary transition-opacity shrink-0 absolute -left-3 top-0.5 pointer-events-none" />
                    {#if playerStore.playlistItemUuid === item.uuid && playerStore.state === "playing"}
                      <div class="flex items-center justify-center gap-0.5 h-4 w-4 absolute inset-0 group-hover:opacity-0 transition-opacity">
                        <span class="w-0.5 bg-brand-accent animate-bounce h-full" style="animation-delay: 0.1s"></span>
                        <span class="w-0.5 bg-brand-accent animate-bounce h-2/3" style="animation-delay: 0.2s"></span>
                        <span class="w-0.5 bg-brand-accent animate-bounce h-full" style="animation-delay: 0.3s"></span>
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
                </td>
                <td class="py-2.5 px-4 font-medium truncate max-w-xs {isSelected || (!unavailable && playerStore.playlistItemUuid === item.uuid) ? 'text-brand-accent-text-hover' : unavailable ? 'text-brand-text-secondary/50' : 'text-brand-text-primary'}">
                  <div class="flex items-center gap-2 max-w-full">
                    {#if unavailable}
                      <span title={i18n.t("playlists.fileNotFoundTooltip")}>
                        <AlertTriangle class="w-3.5 h-3.5 shrink-0 text-amber-400/80" />
                      </span>
                      <span class="truncate line-through decoration-brand-text-secondary/40">
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
                        role="button"
                        tabindex="0"
                        onclick={(e) => { e.stopPropagation(); collectionStore.navigateTo("collection", "songs", item.song?.title || ""); }}
                        onkeydown={(e) => { if (e.key === "Enter" || e.key === " ") { e.stopPropagation(); collectionStore.navigateTo("collection", "songs", item.song?.title || ""); } }}
                        class="hover:underline hover:text-brand-accent-text transition-all duration-150 text-left truncate cursor-pointer font-medium {playerStore.playlistItemUuid === item.uuid ? 'text-brand-accent-text-hover' : 'text-brand-text-primary'}"
                        title={i18n.t("collection.filterByTitle", { title: item.song.title })}
                      >
                        {item.song.title}
                      </span>
                    {:else}
                      <span class="truncate">{i18n.t("collection.unknownSong")}</span>
                    {/if}
                  </div>
                </td>
                <td class="py-2.5 px-4 text-brand-text-secondary/90 truncate max-w-xs">
                  {#if unavailable}
                    <span class="text-brand-text-secondary/40 italic text-xs">{i18n.t("playlists.fileNotFoundText")}</span>
                  {:else if item.song?.artist}
                    <span
                      role="button"
                      tabindex="0"
                      onclick={(e) => { e.stopPropagation(); collectionStore.viewArtist(item.song?.album_artist?.trim() || item.song?.artist || ""); }}
                      onkeydown={(e) => { if (e.key === "Enter" || e.key === " ") { e.stopPropagation(); collectionStore.viewArtist(item.song?.album_artist?.trim() || item.song?.artist || ""); } }}
                      class="hover:underline hover:text-brand-accent-text transition-all duration-150 text-left truncate cursor-pointer text-brand-text-secondary/90"
                      title={i18n.t("collection.filterByArtist", { artist: item.song.artist })}
                    >
                      {item.song.artist}
                    </span>
                  {:else}
                    <span class="text-brand-text-secondary/50">{i18n.t("collection.unknownArtist")}</span>
                  {/if}
                </td>
                <td class="py-2.5 px-4 text-brand-text-secondary/70 truncate max-w-xs">
                  {#if unavailable}
                    <span class="text-brand-text-secondary/40 italic text-xs">{item.song?.album ?? ""}</span>
                  {:else if item.song?.album}
                    <span
                      role="button"
                      tabindex="0"
                      onclick={(e) => { e.stopPropagation(); collectionStore.viewAlbum(item.song?.album || ""); }}
                      onkeydown={(e) => { if (e.key === "Enter" || e.key === " ") { e.stopPropagation(); collectionStore.viewAlbum(item.song?.album || ""); } }}
                      class="hover:underline hover:text-brand-accent-text transition-all duration-150 text-left truncate cursor-pointer text-brand-text-secondary/70"
                      title={i18n.t("collection.filterByAlbum", { album: item.song.album })}
                    >
                      {item.song.album}
                    </span>
                  {:else}
                    <span class="text-brand-text-secondary/50">{i18n.t("collection.unknownAlbum")}</span>
                  {/if}
                </td>
                <td class="py-2.5 px-4 text-center">
                  {#if item.song && !unavailable}
                    <div class="flex justify-center" onclick={(e) => e.stopPropagation()} role="presentation">
                      <SongRating rating={item.song.rating} onRate={(r) => rateItem(item, r)} />
                    </div>
                  {/if}
                </td>
                <td class="py-2.5 px-4 text-center text-brand-text-secondary/80">{formatDuration(item.song?.length_nanosec)}</td>
                <td class="py-2.5 px-4 text-center">
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
                </td>
              </tr>
            {/each}
            {#if filteredTracks.length === 0}
              <tr>
                <td colspan="7" class="py-12 text-center text-brand-text-secondary/45">
                  <ListMusic class="w-12 h-12 mx-auto mb-2 text-brand-text-secondary/30" />
                  {#if filterQuery}
                    {i18n.t("playlists.noFilterResults", { query: filterQuery })}
                  {:else}
                    {i18n.t("playlists.emptyPlaylistTitle")}. {i18n.t("playlists.emptyPlaylistText")}
                  {/if}
                </td>
              </tr>
            {/if}
          </tbody>
        </table>
      </div>
    </div>
    </div>

    <!-- Floating Multi-Select Batch Toolbar -->
    {#if selectedUuids.size > 0}
      <div data-floating-toolbar="true" class="absolute bottom-10 left-1/2 -translate-x-1/2 z-40 bg-brand-sidebar/95 border border-brand-accent/40 rounded-full px-5 py-2 shadow-2xl backdrop-blur-xl flex items-center gap-4 text-xs animate-in slide-in-from-bottom-4 duration-200">
        <span class="font-bold text-brand-accent-text">
          {i18n.t("playlists.selectedCount", { count: selectedUuids.size })}
        </span>
        <div class="h-4 w-px bg-brand-border/60"></div>
        <button
          onclick={playSelected}
          class="flex items-center gap-1.5 text-brand-text-primary hover:text-brand-accent-text font-semibold transition-colors cursor-pointer"
        >
          <Play class="w-3.5 h-3.5 fill-current" />
          <span>{i18n.t("playlists.playSelected")}</span>
        </button>
        <button
          onclick={removeSelected}
          class="flex items-center gap-1.5 text-red-400 hover:text-red-300 font-semibold transition-colors cursor-pointer"
        >
          <Trash2 class="w-3.5 h-3.5" />
          <span>{i18n.t("playlists.removeSelected")}</span>
        </button>
        <button
          onclick={() => { selectedUuids = new Set(); lastSelectedIndex = null; }}
          class="p-1 text-brand-text-secondary hover:text-brand-text-primary transition-colors cursor-pointer ml-1"
          title={i18n.t("playlists.clearSelection")}
        >
          <XCircle class="w-4 h-4" />
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

{#if editingSongId !== null}
  <TagEditor
    songId={editingSongId}
    onClose={() => { editingSongId = null; }}
    onSave={handleTagEditorSaved}
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
