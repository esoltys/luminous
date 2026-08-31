<script lang="ts" module>
  import type { Song } from "../types";

  /** One renderable row. `song` is optional because a playlist item can point at an unavailable/missing song. */
  export interface SongTableRow {
    /** Stable identity for selection/keying — `String(song.id)` for song-keyed views, `item.uuid` for playlist views. */
    key: string;
    song: Song | undefined;
    disabled?: boolean;
    disabledTooltip?: string;
    /** "dim" (default) just lowers row opacity; "strikethrough" also strikes the title text. A warning icon shows in the title cell whenever the row is disabled, regardless of variant. */
    disabledVariant?: "dim" | "strikethrough";
    isDuplicate?: boolean;
    /** Position of this row in the caller's underlying (unfiltered) order — required when `onReorder` is provided. */
    underlyingIndex?: number;
  }
</script>

<script lang="ts">
  import { collectionStore } from "../stores/collection.svelte";
  import { navigationStore } from "../stores/navigation.svelte";
  import { playerStore } from "../stores/player.svelte";
  import { i18n } from "../stores/i18n.svelte";
  import { formatDate, formatFileSize, formatSampleRate, formatBitDepth, formatChannels, formatDuration } from "../utils/formatters";
  import { formatDateAdded } from "../utils/date";
  import { formatTrackNumber } from "../utils/artist";
  import { parseMultiValue } from "../utils/multiValue";
  import { SONG_TABLE_COLUMNS } from "../utils/songColumns";
  import { columnResize } from "../utils/columnResize";
  import { watchScrollMemory } from "../utils/scrollMemory";
  import type { VisibleColumns } from "../stores/collection.svelte";
  import SortableHeader from "./SortableHeader.svelte";
  import SongRating from "./SongRating.svelte";
  import GenreChips from "./GenreChips.svelte";
  import ArtistTagChips from "./ArtistTagChips.svelte";
  import LinkButton from "./LinkButton.svelte";
  import CoverArt from "./CoverArt.svelte";
  import NowPlayingBars from "./NowPlayingBars.svelte";
  import EmptyState from "./EmptyState.svelte";
  import { Play, Plus, Edit3, Trash2, GripVertical, AlertTriangle, Music, Clock, DiscAlbum } from "lucide-svelte";
  import { VirtualList } from "svelte-virtual-list-ts";
  import type { Snippet } from "svelte";

  const POINTER_DRAG_THRESHOLD_PX = 4;

  interface Props {
    rows: SongTableRow[];
    /** "track" shows a disc-aware track number (album/artist/collection/auto-playlist views); "position" shows 1-based position + a play button, optionally with a drag handle when `onReorder` is set (playlist views). */
    mode: "track" | "position";
    discCount?: number;
    leadingColumnWidth: string;
    colDefaults: Partial<Record<keyof VisibleColumns, string>>;
    /** `keyof Song`, or "track"/"position" for the leading column's natural order. */
    sortField: string;
    sortAsc: boolean;
    onToggleSort: (field: string) => void;
    selectedKeys?: Set<string>;
    /**
     * Row order to use for shift-click range selection. Defaults to `rows`.
     * Pass the caller's natural (pre-sort) order to match the existing quirk
     * where range-select operates on natural order even when the table is
     * currently displayed sorted by a column.
     */
    rangeSelectionOrder?: SongTableRow[];
    loading?: boolean;
    emptyState?: Snippet;
    onRowDoubleClick: (row: SongTableRow) => void;
    onRowContextMenu: (event: MouseEvent, row: SongTableRow) => void;
    onRate: (song: Song, rating: number) => void;
    onAddToPlaylist?: (song: Song) => void;
    onRemoveFromPlaylist?: (row: SongTableRow) => void;
    onEditTags: (song: Song) => void;
    /** Adds a per-row "edit this song's album" quick action (AutoPlaylistDetailView only). */
    onEditAlbum?: (song: Song) => void;
    /** When set, position rows show a drag handle and pointer-based reorder is enabled (PlaylistView only). */
    onReorder?: (fromIndex: number, toIndex: number, selectedKeys: string[]) => void;
    /** When set in "position" mode, the leading column header becomes a sortable control for this field instead of a plain label. */
    positionSortField?: string;
    /** When true, disabled rows stay selectable/right-clickable (PlaylistView, so an unavailable track can still be selected and removed) — only play/double-click and drag stay blocked. */
    interactiveWhenDisabled?: boolean;
    /** When true, disabled rows show `disabledTooltip` in place of the artist/album cell content instead of the song's real (possibly stale) values (PlaylistView). */
    disabledPlaceholder?: boolean;
    /**
     * Overrides how a row is identified as "currently playing" — defaults to
     * matching `song.id` against `playerStore.currentSong`. PlaylistView
     * needs the more precise `playerStore.playlistItemUuid` match too, since
     * the same song can appear more than once in a playlist and only one
     * instance is actually playing.
     */
    isRowPlaying?: (row: SongTableRow) => boolean;
    virtualized?: boolean;
    /** Persists/restores virtualized scroll position — only used when `virtualized` is true. */
    scrollMemoryKey?: string;
  }

  let {
    rows,
    mode,
    discCount = 1,
    leadingColumnWidth,
    colDefaults,
    sortField,
    sortAsc,
    onToggleSort,
    selectedKeys = $bindable(new Set()),
    rangeSelectionOrder,
    loading = false,
    emptyState,
    onRowDoubleClick,
    onRowContextMenu,
    onRate,
    onAddToPlaylist,
    onRemoveFromPlaylist,
    onEditTags,
    onEditAlbum,
    onReorder,
    positionSortField,
    interactiveWhenDisabled = false,
    disabledPlaceholder = false,
    isRowPlaying,
    virtualized = false,
    scrollMemoryKey,
  }: Props = $props();

  function rowIsPlaying(row: SongTableRow): boolean {
    if (isRowPlaying) return isRowPlaying(row);
    return !!row.song && !!playerStore.currentSong && playerStore.currentSong.id === row.song.id;
  }

  let lastSelectedKey = $state<string | null>(null);
  let keyIndex = $derived.by(() => {
    const m = new Map<string, number>();
    rows.forEach((r, i) => m.set(r.key, i));
    return m;
  });

  // The virtualized body's <svelte-virtual-list-viewport> reserves layout
  // width for its own scrollbar; the sticky header sits outside that
  // viewport and doesn't. On platforms with a non-overlay scrollbar (e.g.
  // Windows), that gutter makes the header's grid track a few pixels wider
  // than the rows' grid track, misaligning every column after the ones that
  // can absorb it. Measure the actual scrollbar width and pad the header by
  // the same amount so both grids compute identical track widths.
  let bodyContainer = $state<HTMLDivElement | undefined>(undefined);
  let scrollbarWidth = $state(0);

  $effect(() => {
    if (!virtualized || !bodyContainer) return;
    const viewport = bodyContainer.querySelector<HTMLElement>("svelte-virtual-list-viewport");
    if (!viewport) return;
    const update = () => {
      scrollbarWidth = viewport.offsetWidth - viewport.clientWidth;
    };
    update();
    const observer = new ResizeObserver(update);
    observer.observe(viewport);
    return () => observer.disconnect();
  });

  $effect(() => {
    if (!virtualized || !scrollMemoryKey || !bodyContainer) return;
    const viewport = bodyContainer.querySelector<HTMLElement>("svelte-virtual-list-viewport");
    if (!viewport) return;
    return watchScrollMemory(viewport, scrollMemoryKey);
  });

  let orderForRange = $derived(rangeSelectionOrder ?? rows);

  function handleRowClick(e: MouseEvent, row: SongTableRow) {
    const order = orderForRange;
    if (e.shiftKey && lastSelectedKey !== null) {
      const idx1 = order.findIndex((r) => r.key === lastSelectedKey);
      const idx2 = order.findIndex((r) => r.key === row.key);
      if (idx1 !== -1 && idx2 !== -1) {
        const start = Math.min(idx1, idx2);
        const end = Math.max(idx1, idx2);
        const newSet = new Set(e.ctrlKey || e.metaKey ? selectedKeys : []);
        for (let i = start; i <= end; i++) newSet.add(order[i].key);
        selectedKeys = newSet;
      }
    } else if (e.ctrlKey || e.metaKey) {
      const newSet = new Set(selectedKeys);
      if (newSet.has(row.key)) newSet.delete(row.key);
      else newSet.add(row.key);
      selectedKeys = newSet;
      lastSelectedKey = row.key;
    } else if (selectedKeys.size === 1 && selectedKeys.has(row.key)) {
      selectedKeys = new Set();
      lastSelectedKey = null;
    } else {
      selectedKeys = new Set([row.key]);
      lastSelectedKey = row.key;
    }
  }

  function handleRowContextMenu(e: MouseEvent, row: SongTableRow) {
    e.preventDefault();
    if (!selectedKeys.has(row.key)) {
      selectedKeys = new Set([row.key]);
      lastSelectedKey = row.key;
    }
    onRowContextMenu(e, row);
  }

  function handleKeydown(e: KeyboardEvent) {
    if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "a") {
      const target = e.target as HTMLElement;
      if (target && (target.tagName === "INPUT" || target.tagName === "TEXTAREA")) return;
      e.preventDefault();
      selectedKeys = new Set(rows.map((r) => r.key));
    } else if (e.key === "Escape") {
      selectedKeys = new Set();
    }
  }

  function handleWindowMouseDown(e: MouseEvent) {
    if (selectedKeys.size === 0) return;
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
    selectedKeys = new Set();
  }

  // --- Pointer-based drag reorder (only active when onReorder is provided) ---
  // Native HTML5 drag events are swallowed by Tauri's dragDropEnabled window
  // option (needed for OS file-drop import), so reordering uses pointer
  // events instead.
  let draggedIndex = $state<number | null>(null);
  let dragOverIndex = $state<number | null>(null);
  let pointerDragArmed = false;
  let pointerDragStartX = 0;
  let pointerDragStartY = 0;

  function commitReorder(targetIndex: number) {
    if (draggedIndex === null || targetIndex === draggedIndex) return;
    const keys =
      selectedKeys.size > 1 && selectedKeys.has(rows.find((r) => r.underlyingIndex === draggedIndex)?.key ?? "")
        ? Array.from(selectedKeys)
        : [rows.find((r) => r.underlyingIndex === draggedIndex)?.key ?? ""];
    onReorder?.(draggedIndex, targetIndex, keys);
  }

  function handleRowPointerDown(e: PointerEvent, row: SongTableRow) {
    if (!onReorder || row.underlyingIndex === undefined || row.disabled) return;
    const target = e.target as HTMLElement;
    if (target.closest("button, a, input, select, textarea, [data-interactive]")) return;
    if (e.button !== 0) return;
    pointerDragArmed = false;
    pointerDragStartX = e.clientX;
    pointerDragStartY = e.clientY;
    draggedIndex = row.underlyingIndex;
    // With the window's dragDropEnabled option on, WebView2 can hijack an in-progress mouse
    // gesture into a native OS drag once it crosses the platform's drag threshold, silently
    // stopping pointermove/pointerup from reaching the DOM. Explicit pointer capture pins
    // subsequent events to this element (and this JS event loop) instead, preventing that.
    (e.currentTarget as HTMLElement).setPointerCapture?.(e.pointerId);
    window.addEventListener("pointermove", handlePointerDragMove);
    window.addEventListener("pointerup", handlePointerDragUp);
  }

  function handlePointerDragMove(e: PointerEvent) {
    if (draggedIndex === null) return;
    if (!pointerDragArmed) {
      const dx = e.clientX - pointerDragStartX;
      const dy = e.clientY - pointerDragStartY;
      if (Math.hypot(dx, dy) < POINTER_DRAG_THRESHOLD_PX) return;
      pointerDragArmed = true;
    }
    const el = document.elementFromPoint(e.clientX, e.clientY)?.closest("[data-song-row]") as HTMLElement | null;
    const idx = el?.dataset.index;
    dragOverIndex = idx !== undefined ? Number(idx) : null;
  }

  function handlePointerDragUp() {
    window.removeEventListener("pointermove", handlePointerDragMove);
    window.removeEventListener("pointerup", handlePointerDragUp);
    if (pointerDragArmed && dragOverIndex !== null) {
      commitReorder(dragOverIndex);
    }
    draggedIndex = null;
    dragOverIndex = null;
    pointerDragArmed = false;
  }

  // --- Grid column template ---
  let gridColsStyle = $derived.by(() => {
    const vc = collectionStore.visibleColumns;
    const cw = collectionStore.columnWidths;
    const cols: string[] = [leadingColumnWidth];
    for (const { key } of SONG_TABLE_COLUMNS) {
      if (mode === "position" && key === "track") continue;
      if (!vc[key]) continue;
      const saved = cw[key];
      cols.push(saved !== undefined ? `${saved}px` : (colDefaults[key] ?? "80px"));
    }
    return `grid-template-columns: ${cols.join(" ")}`;
  });

  interface HeaderMeta {
    i18nKey: string;
    className: string;
    truncateClass?: string;
  }
  const HEADER_META: Partial<Record<string, HeaderMeta>> = {
    track: { i18nKey: "collection.tableHeaderTrack", className: "text-left hover:text-brand-text-primary transition-colors flex items-center gap-1 font-semibold uppercase tracking-wider min-w-0 w-full", truncateClass: "max-w-[calc(100%-1rem)]" },
    title: { i18nKey: "collection.tableHeaderTitle", className: "text-left hover:text-brand-text-primary transition-colors flex items-center gap-1 font-semibold uppercase tracking-wider min-w-0 w-full", truncateClass: "max-w-[calc(100%-1rem)]" },
    artist: { i18nKey: "collection.tableHeaderArtist", className: "text-left hover:text-brand-text-primary transition-colors flex items-center gap-1 font-semibold uppercase tracking-wider min-w-0 w-full", truncateClass: "max-w-[calc(100%-1rem)]" },
    album: { i18nKey: "collection.tableHeaderAlbum", className: "text-left hover:text-brand-text-primary transition-colors flex items-center gap-1 font-semibold uppercase tracking-wider min-w-0 w-full", truncateClass: "max-w-[calc(100%-1rem)]" },
    composer: { i18nKey: "collection.tableHeaderComposer", className: "text-left hover:text-brand-text-primary transition-colors flex items-center gap-1 font-semibold uppercase tracking-wider min-w-0 w-full", truncateClass: "max-w-[calc(100%-0.5rem)]" },
    album_artist: { i18nKey: "collection.tableHeaderAlbumArtist", className: "text-left hover:text-brand-text-primary transition-colors flex items-center gap-1 font-semibold uppercase tracking-wider min-w-0 w-full", truncateClass: "max-w-[calc(100%-0.5rem)]" },
    artist_tag: { i18nKey: "collection.tableHeaderArtistTag", className: "text-left hover:text-brand-text-primary transition-colors flex items-center gap-1 font-semibold uppercase tracking-wider min-w-0 w-full", truncateClass: "max-w-[calc(100%-0.5rem)]" },
    format: { i18nKey: "collection.tableHeaderFormat", className: "text-left hover:text-brand-text-primary transition-colors flex items-center gap-1 font-semibold uppercase tracking-wider min-w-0 w-full", truncateClass: "max-w-[calc(100%-0.5rem)]" },
    year: { i18nKey: "collection.tableHeaderYear", className: "text-left hover:text-brand-text-primary transition-colors flex items-center gap-1 font-semibold uppercase tracking-wider min-w-0 w-full", truncateClass: "max-w-[calc(100%-0.5rem)]" },
    originalyear: { i18nKey: "collection.tableHeaderOriginalYear", className: "text-left hover:text-brand-text-primary transition-colors flex items-center gap-1 font-semibold uppercase tracking-wider min-w-0 w-full", truncateClass: "max-w-[calc(100%-0.5rem)]" },
    genre: { i18nKey: "collection.tableHeaderGenre", className: "text-left hover:text-brand-text-primary transition-colors flex items-center gap-1 font-semibold uppercase tracking-wider min-w-0 w-full", truncateClass: "max-w-[calc(100%-0.5rem)]" },
    grouping: { i18nKey: "collection.tableHeaderGrouping", className: "text-left hover:text-brand-text-primary transition-colors flex items-center gap-1 font-semibold uppercase tracking-wider min-w-0 w-full", truncateClass: "max-w-[calc(100%-0.5rem)]" },
    bpm: { i18nKey: "collection.tableHeaderBpm", className: "text-left hover:text-brand-text-primary transition-colors flex items-center gap-1 font-semibold uppercase tracking-wider min-w-0 w-full", truncateClass: "max-w-[calc(100%-0.5rem)]" },
    initial_key: { i18nKey: "collection.tableHeaderInitialKey", className: "text-left hover:text-brand-text-primary transition-colors flex items-center gap-1 font-semibold uppercase tracking-wider min-w-0 w-full", truncateClass: "max-w-[calc(100%-0.5rem)]" },
    bitrate: { i18nKey: "collection.tableHeaderBitrate", className: "text-left hover:text-brand-text-primary transition-colors flex items-center gap-1 font-semibold uppercase tracking-wider min-w-0 w-full", truncateClass: "max-w-[calc(100%-0.5rem)]" },
    samplerate: { i18nKey: "collection.tableHeaderSampleRate", className: "text-left hover:text-brand-text-primary transition-colors flex items-center gap-1 font-semibold uppercase tracking-wider min-w-0 w-full", truncateClass: "max-w-[calc(100%-0.5rem)]" },
    bitdepth: { i18nKey: "collection.tableHeaderBitDepth", className: "text-left hover:text-brand-text-primary transition-colors flex items-center gap-1 font-semibold uppercase tracking-wider min-w-0 w-full", truncateClass: "max-w-[calc(100%-0.5rem)]" },
    channels: { i18nKey: "collection.tableHeaderChannels", className: "text-left hover:text-brand-text-primary transition-colors flex items-center gap-1 font-semibold uppercase tracking-wider min-w-0 w-full", truncateClass: "max-w-[calc(100%-0.5rem)]" },
    filesize: { i18nKey: "collection.tableHeaderFileSize", className: "text-left hover:text-brand-text-primary transition-colors flex items-center gap-1 font-semibold uppercase tracking-wider min-w-0 w-full", truncateClass: "max-w-[calc(100%-0.5rem)]" },
    rating: { i18nKey: "collection.tableHeaderRating", className: "flex items-center justify-center hover:text-brand-text-primary transition-colors font-semibold uppercase tracking-wider min-w-0 w-full" },
    playcount: { i18nKey: "collection.tableHeaderPlays", className: "text-center hover:text-brand-text-primary transition-colors flex items-center justify-center gap-1 font-semibold uppercase tracking-wider min-w-0 w-full" },
    skipcount: { i18nKey: "collection.tableHeaderSkips", className: "text-center hover:text-brand-text-primary transition-colors flex items-center justify-center gap-1 font-semibold uppercase tracking-wider min-w-0 w-full" },
    lastplayed: { i18nKey: "collection.tableHeaderLastPlayed", className: "text-center hover:text-brand-text-primary transition-colors flex items-center justify-center gap-1 font-semibold uppercase tracking-wider min-w-0 w-full" },
    added: { i18nKey: "collection.tableHeaderAdded", className: "text-center hover:text-brand-text-primary transition-colors flex items-center justify-center gap-1 font-semibold uppercase tracking-wider min-w-0 w-full" },
    duration: { i18nKey: "", className: "flex items-center justify-center hover:text-brand-text-primary transition-colors font-semibold uppercase tracking-wider min-w-0 w-full" },
    path: { i18nKey: "collection.tableHeaderPath", className: "text-left hover:text-brand-text-primary transition-colors flex items-center gap-1 font-semibold uppercase tracking-wider min-w-0 w-full", truncateClass: "max-w-[calc(100%-0.5rem)]" },
  };

  function headerActiveField(col: (typeof SONG_TABLE_COLUMNS)[number]): string {
    return col.field ?? col.key;
  }

  function bindResize(key: keyof VisibleColumns) {
    return { column: key, onResize: collectionStore.setColumnWidth.bind(collectionStore), onReset: collectionStore.resetColumnWidth.bind(collectionStore) };
  }

  function rowDisabled(row: SongTableRow): boolean {
    return !!row.disabled;
  }

  // Body cells use primary text throughout — secondary was too low-contrast
  // to read comfortably.
  function secondaryColor(_song: Song): string {
    return "text-brand-text-primary";
  }
</script>

{#snippet headerCell(col: (typeof SONG_TABLE_COLUMNS)[number])}
  {@const meta = HEADER_META[col.key]}
  {#if col.key === "actions"}
    <div use:columnResize={bindResize(col.key)} class="relative overflow-hidden text-center">{i18n.t("collection.tableHeaderActions")}</div>
  {:else if meta}
    <div use:columnResize={bindResize(col.key)} class="relative overflow-hidden">
      <SortableHeader
        active={sortField === headerActiveField(col)}
        {sortAsc}
        onclick={() => onToggleSort(headerActiveField(col))}
        class={meta.className}
      >
        {#snippet label(arrow)}
          {#if col.key === "duration"}
            <Clock class="w-3.5 h-3.5 shrink-0" /> {arrow}
          {:else}
            <span class="truncate {meta.truncateClass ?? ''}">{i18n.t(meta.i18nKey)} {arrow}</span>
          {/if}
        {/snippet}
      </SortableHeader>
    </div>
  {/if}
{/snippet}

{#snippet bodyCell(col: (typeof SONG_TABLE_COLUMNS)[number], song: Song, row: SongTableRow)}
  {#if col.key === "track"}
    <div class="{secondaryColor(song)} truncate pr-2 min-w-0 text-xs font-medium">
      {formatTrackNumber(song.track, song.disc, discCount, keyIndex.get(row.key) ?? 0)}
    </div>
  {:else if col.key === "title"}
    <div class="font-medium truncate pr-4 flex items-center gap-2 min-w-0">
      <CoverArt
        songId={song.id}
        artEmbedded={song.art_embedded}
        artAutomatic={song.art_automatic}
        artManual={song.art_manual}
        sizeClass="w-7 h-7 rounded shrink-0"
      />
      {#if rowDisabled(row)}
        <span title={row.disabledTooltip}>
          <AlertTriangle class="w-3.5 h-3.5 shrink-0 text-amber-400/80" />
        </span>
      {/if}
      {#if row.isDuplicate}
        <span
          class="px-1.5 py-0.5 text-[10px] font-bold rounded bg-brand-accent/20 text-brand-accent-text border border-brand-accent/30 shrink-0"
          title={i18n.t("playlists.duplicateTrackFlag")}
        >
          {i18n.t("playlists.duplicateTrackFlag")}
        </span>
      {/if}
      <span
        class="truncate text-brand-text-primary {row.disabledVariant === 'strikethrough' && rowDisabled(row) ? 'line-through' : ''}"
        title={song.title || i18n.t("collection.unknownSong")}
      >
        {song.title || i18n.t("collection.unknownSong")}
      </span>
    </div>
  {:else if col.key === "artist"}
    <div class="{secondaryColor(song)} truncate pr-4 flex items-center min-w-0">
      {#if disabledPlaceholder && rowDisabled(row)}
        <span class="text-brand-text-secondary italic text-xs">{row.disabledTooltip}</span>
      {:else if song.artist}
        {#each parseMultiValue(song.artist) as name, i (name)}
          {#if i > 0}<span class="{secondaryColor(song)}/50 shrink-0">,&nbsp;</span>{/if}
          <LinkButton
            onclick={(e) => { e.stopPropagation(); navigationStore.viewArtist(name); }}
            class="{secondaryColor(song)} truncate min-w-0"
            title={i18n.t("collection.filterByArtist", { artist: name })}
          >
            {name}
          </LinkButton>
        {/each}
      {:else}
        <span class="{secondaryColor(song)} truncate min-w-0">{i18n.t("collection.unknownArtist")}</span>
      {/if}
    </div>
  {:else if col.key === "album"}
    <div class="{secondaryColor(song)} truncate pr-4 flex items-center min-w-0">
      {#if disabledPlaceholder && rowDisabled(row)}
        <span class="text-brand-text-secondary italic text-xs">{song.album ?? ""}</span>
      {:else if song.album}
        <LinkButton
          onclick={(e) => { e.stopPropagation(); navigationStore.viewAlbum(song.album || ""); }}
          class="{secondaryColor(song)} truncate min-w-0"
          title={i18n.t("collection.filterByAlbum", { album: song.album })}
        >
          {song.album}
        </LinkButton>
      {:else}
        <span class="{secondaryColor(song)} truncate min-w-0">{i18n.t("collection.unknownAlbum")}</span>
      {/if}
    </div>
  {:else if col.key === "composer"}
    <div class="{secondaryColor(song)} truncate pr-2 min-w-0 text-xs font-medium" title={song.composer}>
      {song.composer || "—"}
    </div>
  {:else if col.key === "album_artist"}
    <div class="{secondaryColor(song)} truncate pr-2 min-w-0 text-xs font-medium" title={song.album_artist}>
      {song.album_artist || "—"}
    </div>
  {:else if col.key === "artist_tag"}
    {@const artistKey = song.album_artist || song.artist || ""}
    {@const profile = collectionStore.getArtistProfile(artistKey)}
    {@const tags = profile?.tags ?? []}
    <div class="truncate pr-2 min-w-0" title={tags.join("; ")}>
      {#if tags.length > 0}
        <ArtistTagChips tags={tags} />
      {:else}
        <span class="{secondaryColor(song)} text-xs font-medium">—</span>
      {/if}
    </div>
  {:else if col.key === "format"}
    <div class="{secondaryColor(song)} truncate pr-2 min-w-0 text-xs font-semibold uppercase">
      {song.filetype ? song.filetype.toUpperCase() : "—"}
    </div>
  {:else if col.key === "year"}
    <div class="{secondaryColor(song)} truncate pr-2 min-w-0 text-xs font-medium">
      {song.year || "—"}
    </div>
  {:else if col.key === "originalyear"}
    <div class="{secondaryColor(song)} truncate pr-2 min-w-0 text-xs font-medium">
      {song.originalyear || "—"}
    </div>
  {:else if col.key === "genre"}
    <div class="truncate pr-2 min-w-0" title={song.genre}>
      {#if song.genre}
        <GenreChips genre={song.genre} />
      {:else}
        <span class="{secondaryColor(song)} text-xs font-medium">—</span>
      {/if}
    </div>
  {:else if col.key === "grouping"}
    <div class="{secondaryColor(song)} truncate pr-2 min-w-0 text-xs font-medium" title={song.grouping}>
      {song.grouping || "—"}
    </div>
  {:else if col.key === "bpm"}
    <div class="{secondaryColor(song)} truncate pr-2 min-w-0 text-xs font-medium">
      {song.bpm || "—"}
    </div>
  {:else if col.key === "initial_key"}
    <div class="{secondaryColor(song)} truncate pr-2 min-w-0 text-xs font-medium">
      {song.initial_key || "—"}
    </div>
  {:else if col.key === "bitrate"}
    <div class="{secondaryColor(song)} truncate pr-2 min-w-0 text-xs font-medium">
      {song.bitrate ? `${song.bitrate}k` : "—"}
    </div>
  {:else if col.key === "samplerate"}
    <div class="{secondaryColor(song)} truncate pr-2 min-w-0 text-xs font-medium">
      {formatSampleRate(song.samplerate)}
    </div>
  {:else if col.key === "bitdepth"}
    <div class="{secondaryColor(song)} truncate pr-2 min-w-0 text-xs font-medium">
      {formatBitDepth(song.bitdepth)}
    </div>
  {:else if col.key === "channels"}
    <div class="{secondaryColor(song)} truncate pr-2 min-w-0 text-xs font-medium">
      {formatChannels(song.channels)}
    </div>
  {:else if col.key === "filesize"}
    <div class="{secondaryColor(song)} truncate pr-2 min-w-0 text-xs font-medium">
      {formatFileSize(song.filesize)}
    </div>
  {:else if col.key === "rating"}
    <div class="flex justify-center">
      <SongRating rating={song.rating} onRate={(r) => onRate(song, r)} />
    </div>
  {:else if col.key === "playcount"}
    <div class="text-center {secondaryColor(song)} font-medium text-xs">
      {song.playcount ?? 0}
    </div>
  {:else if col.key === "skipcount"}
    <div class="text-center {secondaryColor(song)} font-medium text-xs">
      {song.skipcount ?? 0}
    </div>
  {:else if col.key === "lastplayed"}
    <div class="text-center {secondaryColor(song)} text-xs whitespace-nowrap">
      {formatDate(song.lastplayed)}
    </div>
  {:else if col.key === "added"}
    <div class="text-center {secondaryColor(song)} text-xs whitespace-nowrap">
      {formatDateAdded(song.added)}
    </div>
  {:else if col.key === "duration"}
    <div class="text-center {secondaryColor(song)} text-xs font-medium">
      {formatDuration(song.length_nanosec)}
    </div>
  {:else if col.key === "path"}
    <div class="{secondaryColor(song)} truncate pr-4 min-w-0 text-xs font-medium" title={song.path}>
      {song.path || "—"}
    </div>
  {:else if col.key === "actions"}
    <div class="flex items-center justify-center gap-2.5">
      {#if onAddToPlaylist}
        <button
          onclick={() => onAddToPlaylist?.(song)}
          class="text-brand-text-secondary hover:text-brand-accent-text transition-colors"
          title={i18n.t("collection.addPlaylistTooltipDefault")}
        >
          <Plus class="w-4 h-4" />
        </button>
      {/if}
      <button
        onclick={() => onEditTags(song)}
        class="text-brand-text-secondary hover:text-brand-accent-text transition-colors"
        title={i18n.t("collection.editTagsTooltip")}
      >
        <Edit3 class="w-4 h-4" />
      </button>
      {#if onEditAlbum}
        <button
          onclick={() => onEditAlbum?.(song)}
          class="text-brand-text-secondary hover:text-brand-accent-text transition-colors disabled:opacity-40 disabled:cursor-not-allowed"
          disabled={!song.album}
          title={i18n.t("songTags.editAlbumTooltip", {}, "Edit Album")}
        >
          <DiscAlbum class="w-4 h-4" />
        </button>
      {/if}
      {#if onRemoveFromPlaylist}
        <button
          onclick={() => onRemoveFromPlaylist?.(row)}
          class="text-brand-text-secondary hover:text-red-400 transition-colors"
          title={i18n.t("playlists.removeFromPlaylist")}
        >
          <Trash2 class="w-4 h-4" />
        </button>
      {/if}
    </div>
  {/if}
{/snippet}

{#snippet leadingCell(row: SongTableRow, displayIndex: number)}
  {@const song = row.song}
  <div class="text-center flex justify-center relative w-9 h-6 items-center">
    {#if song && rowIsPlaying(row) && playerStore.state === "playing"}
      <div class="flex items-center justify-center gap-0.5 h-3.5 w-3.5 absolute group-hover:opacity-0 transition-opacity">
        <NowPlayingBars />
      </div>
    {:else if mode === "position"}
      <span class="absolute text-xs font-medium text-brand-text-secondary group-hover:opacity-0 transition-opacity">{displayIndex + 1}</span>
    {/if}
    {#if song}
      <button
        onclick={(e) => { e.stopPropagation(); if (!rowDisabled(row)) onRowDoubleClick(row); }}
        class="absolute flex items-center justify-center opacity-0 group-hover:opacity-100 text-brand-accent-text hover:text-brand-accent-text-hover transition-all duration-150 disabled:opacity-0 disabled:cursor-not-allowed"
        disabled={rowDisabled(row)}
        title={row.disabledTooltip ?? i18n.t("collection.playSong")}
      >
        <Play class="w-3.5 h-3.5 fill-current" />
      </button>
    {/if}
    {#if onReorder}
      <GripVertical class="absolute right-0 w-3.5 h-3.5 text-brand-text-secondary/60 opacity-0 group-hover:opacity-100 transition-opacity pointer-events-none" />
    {/if}
  </div>
{/snippet}

{#snippet row_(row: SongTableRow, displayIndex: number)}
  {@const song = row.song}
  {@const disabled = rowDisabled(row)}
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div
    data-song-row="true"
    data-index={row.underlyingIndex}
    role="row"
    tabindex="0"
    onclick={(e) => song && (!disabled || interactiveWhenDisabled) && handleRowClick(e, row)}
    ondblclick={() => song && !disabled && onRowDoubleClick(row)}
    oncontextmenu={(e) => song && (!disabled || interactiveWhenDisabled) && handleRowContextMenu(e, row)}
    onkeydown={(e) => { if (e.key === "Enter" && song && !disabled) onRowDoubleClick(row); }}
    onpointerdown={(e) => handleRowPointerDown(e, row)}
    style={gridColsStyle}
    title={row.disabledTooltip}
    class="grid items-center border-b border-brand-border/40 hover:bg-brand-sidebar/40 group transition-colors py-2.5 px-4 text-sm
      {disabled ? 'opacity-50 cursor-not-allowed' : ''}
      {onReorder ? 'cursor-grab active:cursor-grabbing' : ''}
      {selectedKeys.has(row.key) ? 'bg-brand-accent/20 border-l-2 border-brand-accent text-brand-accent-text-hover' : (song && rowIsPlaying(row) ? 'bg-brand-accent/10' : '')}
      {draggedIndex !== null && dragOverIndex === row.underlyingIndex ? 'bg-brand-accent/30!' : ''}"
  >
    {@render leadingCell(row, displayIndex)}
    {#if song}
      {#each SONG_TABLE_COLUMNS as col (col.key)}
        {#if !(mode === "position" && col.key === "track") && collectionStore.visibleColumns[col.key]}
          {@render bodyCell(col, song, row)}
        {/if}
      {/each}
    {/if}
  </div>
{/snippet}

<div class="sticky top-0 z-10 flex flex-col rounded-t-lg bg-brand-sidebar border-b border-brand-border text-xs text-brand-text-secondary uppercase tracking-wider font-semibold select-none">
  <div role="row" class="grid items-center py-3 px-4" style="{gridColsStyle}{virtualized ? `; padding-right: calc(1rem + ${scrollbarWidth}px)` : ''}">
    {#if mode === "position" && positionSortField}
      <SortableHeader
        active={sortField === positionSortField}
        {sortAsc}
        onclick={() => onToggleSort(positionSortField)}
        class="text-center hover:text-brand-text-primary transition-colors flex items-center justify-center gap-1 font-semibold uppercase tracking-wider min-w-0"
      >
        {#snippet label(arrow)}<span class="truncate max-w-[calc(100%-1rem)]">{i18n.t('playlists.tableHeaderTrack')} {arrow}</span>{/snippet}
      </SortableHeader>
    {:else}
      <div class="text-center w-9"></div>
    {/if}
    {#each SONG_TABLE_COLUMNS as col (col.key)}
      {#if !(mode === "position" && col.key === "track") && collectionStore.visibleColumns[col.key]}
        {@render headerCell(col)}
      {/if}
    {/each}
  </div>
</div>

<div
  bind:this={bodyContainer}
  class="rounded-b-lg overflow-hidden {virtualized ? 'flex-1 min-h-0 relative' : ''}"
>
  {#if loading}
    <div class="flex items-center justify-center py-16">
      <div class="text-brand-text-primary text-sm">{i18n.t("home.loading")}</div>
    </div>
  {:else if rows.length === 0}
    <div class="py-16 text-center select-none">
      {#if emptyState}
        {@render emptyState()}
      {:else}
        <EmptyState icon={Music} title={i18n.t("collection.noSongsTitle")} />
      {/if}
    </div>
  {:else if virtualized}
    {#key gridColsStyle}
      <VirtualList items={rows} itemHeight={41}>
        {#snippet children({ item }: { item: SongTableRow })}
          {@render row_(item, keyIndex.get(item.key) ?? 0)}
        {/snippet}
      </VirtualList>
    {/key}
  {:else}
    {#each rows as row, index (row.key)}
      {@render row_(row, index)}
    {/each}
  {/if}
</div>

<svelte:window onkeydown={handleKeydown} onmousedown={handleWindowMouseDown} />
