<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { tick, type Snippet } from "svelte";
  import { VirtualList } from "svelte-virtual-list-ts";
  import { FolderIcon, CaretRightIcon, ArrowClockwiseIcon } from "phosphor-svelte";
  import { i18n } from "../../stores/i18n.svelte";
  import { formatFileSize } from "../../utils/formatters";
  import { startPointerDrag, type PointerDragHandle } from "../../utils/pointerDrag";
  import type { DirListing, FileEntry, FileEntryStatus, FileMenuTarget, FileSource } from "../../types";
  import Button from "../Button.svelte";
  import AlphabetScrubber from "../AlphabetScrubber.svelte";
  import { shownName } from "./shownName";
  import { fileIcon } from "./fileIcon";
  import { highlightSegments, matchesFileFilter, parseFileFilter } from "./fileFilter";
  import { sortFileEntries, type FileSortColumn } from "./fileSorting";
  import { songDrag, filesPayload } from "../../stores/songDrag.svelte";

  /**
   * A column browser over one source (#1841): up to two columns of the
   * folders above the current one, then the current folder's folders and
   * files as a table. It reads listings itself; where it is (`path`) and
   * what happens on navigate and activate belong to the caller. Reused by
   * the device Files tab (#839).
   */
  let {
    source,
    path,
    onnavigate,
    onactivate,
    reloadToken = 0,
    listing = $bindable(null),
    selectedName = $bindable(null),
    selection = $bindable([]),
    filterQuery = $bindable(""),
    matchedCount = $bindable(0),
    totalCount = $bindable(0),
    sortColumn = $bindable("name"),
    sortAsc = $bindable(true),
    target = null,
    arrival = $bindable(null),
    unreachableLabel,
    footer,
    oncontextmenu,
  }: {
    source: FileSource;
    /** The current folder, as names from the source's root. */
    path: string[];
    /** The entry to select, and scroll to, on arriving at `path` (or when it
     * changes while there); none clears the selection when the folder changes. */
    target?: string | null;
    /** In and cleared: an entry just arrived at (View in Files). Outlined while it
     * stays selected, with a halo that fades after a moment. */
    arrival?: string | null;
    onnavigate: (path: string[]) => void;
    /** Enter or double-click on a file in the current folder; what that does is the caller's to decide. */
    onactivate: (entry: FileEntry) => void;
    /** Bump to read every listing again (after a scan, say). */
    reloadToken?: number;
    /** Out: the current folder's listing once read. */
    listing?: DirListing | null;
    /** The focused entry of the current folder, by name. */
    selectedName?: string | null;
    /** Every selected entry of the current folder, by name: the focused one, or
     * files picked with Ctrl/Cmd-click, Shift-click, Shift+arrows or Ctrl+A. */
    selection?: string[];
    /** Instant filter query for the current folder (#1886). */
    filterQuery?: string;
    /** Out: number of entries in the current folder matching `filterQuery`. */
    matchedCount?: number;
    /** Out: total entries in the current folder before filtering. */
    totalCount?: number;
    /** Active sorting column for the current folder's file table (#1888). */
    sortColumn?: FileSortColumn;
    /** Sort direction for the current folder's file table (#1888). */
    sortAsc?: boolean;
    /** Shown when the source can't be reached. */
    unreachableLabel: string;
    footer?: Snippet;
    /** A right-click on a row; `dir` is the folder `entries` are in. In the current
     * folder they are the selected files when the row is one of them, else just
     * that row (which becomes the selection); in a folder column they are that
     * folder, with `inColumn` set, and nothing is selected or opened. Without it
     * the browser shows no menu. */
    oncontextmenu?: (target: FileMenuTarget, x: number, y: number) => void;
  } = $props();

  /** The words for a status; the backend decides which one a file has. */
  const STATUS_LABEL_KEYS: Record<FileEntryStatus, string> = {
    inLibrary: "files.status.inLibrary",
    notScanned: "files.status.notScanned",
    albumArt: "files.status.albumArt",
    notAudio: "files.status.notAudio",
    excluded: "files.status.excluded",
    notFromLuminous: "files.status.notFromLuminous",
  };

  /** Entries shown dimmed: not music the library or sync looks after. */
  const MUTED: (FileEntryStatus | null)[] = ["excluded", "albumArt", "notAudio", "notFromLuminous"];

  function statusLabel(entry: FileEntry): string {
    return entry.status ? i18n.t(STATUS_LABEL_KEYS[entry.status]) : "";
  }

  const ROW_HEIGHT_PX = 31;
  /** Folder columns shown before the current folder's table. */
  const PARENT_COLUMNS = 2;

  /** Folder column width, shared by every column and source; remembered like the sidebar's. */
  const COLUMN_WIDTH_KEY = "files_columnWidth";
  const COLUMN_MIN_PX = 140;
  const COLUMN_MAX_PX = 480;
  const COLUMN_DEFAULT_PX = 224;
  const COLUMN_STEP_PX = 16;

  let columnWidth = $state(readColumnWidth());

  function readColumnWidth(): number {
    try {
      const saved = Number(localStorage.getItem(COLUMN_WIDTH_KEY));
      return saved >= COLUMN_MIN_PX && saved <= COLUMN_MAX_PX ? saved : COLUMN_DEFAULT_PX;
    } catch {
      return COLUMN_DEFAULT_PX;
    }
  }

  function setColumnWidth(width: number) {
    columnWidth = Math.round(Math.min(COLUMN_MAX_PX, Math.max(COLUMN_MIN_PX, width)));
    try {
      localStorage.setItem(COLUMN_WIDTH_KEY, String(columnWidth));
    } catch {
      // Not remembered this time; the width still applies.
    }
  }

  function startColumnResize(event: PointerEvent) {
    event.preventDefault();
    const handle = event.currentTarget as HTMLElement;
    const startX = event.clientX;
    const startWidth = columnWidth;
    handle.setPointerCapture(event.pointerId);
    const move = (e: PointerEvent) => setColumnWidth(startWidth + e.clientX - startX);
    const end = () => {
      handle.removeEventListener("pointermove", move);
      handle.removeEventListener("pointerup", end);
      handle.removeEventListener("pointercancel", end);
    };
    handle.addEventListener("pointermove", move);
    handle.addEventListener("pointerup", end);
    handle.addEventListener("pointercancel", end);
  }

  function columnResizeKey(event: KeyboardEvent) {
    const step = event.key === "ArrowLeft" ? -COLUMN_STEP_PX : event.key === "ArrowRight" ? COLUMN_STEP_PX : 0;
    if (step === 0) return;
    event.preventDefault();
    setColumnWidth(columnWidth + step);
  }

  type Loaded = { state: "loading" } | { state: "ready"; listing: DirListing } | { state: "error"; code: string };

  let listings = $state<Record<string, Loaded>>({});
  let cacheOwner = "";

  // Not a join: a Subsonic folder's name can be empty, and `[""]` must not
  // be the source's root.
  const keyOf = (dir: string[]) => JSON.stringify(dir);

  // A new source or reload empties the cache; otherwise moving around
  // reuses what was already read.
  $effect(() => {
    const owner = `${JSON.stringify(source)}#${reloadToken}`;
    if (owner !== cacheOwner) {
      cacheOwner = owner;
      listings = {};
    }
  });

  const firstLevel = $derived(Math.max(0, path.length - PARENT_COLUMNS));
  /** The folders shown, outermost first; the last is the current folder. */
  const levels = $derived(
    Array.from({ length: path.length - firstLevel + 1 }, (_, i) => path.slice(0, firstLevel + i))
  );

  $effect(() => {
    void cacheOwner;
    for (const dir of levels) load(dir);
  });

  function load(dir: string[]) {
    const key = keyOf(dir);
    if (listings[key]) return;
    listings[key] = { state: "loading" };
    const owner = cacheOwner;
    invoke<DirListing>("list_source_dir", { source, path: dir })
      .then((result) => {
        if (owner === cacheOwner) listings[key] = { state: "ready", listing: result };
      })
      .catch((err) => {
        if (owner === cacheOwner) listings[key] = { state: "error", code: String(err) };
      });
  }

  function retry(dir: string[]) {
    delete listings[keyOf(dir)];
    load(dir);
  }

  const current = $derived(listings[keyOf(path)]);
  $effect(() => {
    listing = current?.state === "ready" ? current.listing : null;
  });

  const folderEntries = (dir: string[]) => {
    const loaded = listings[keyOf(dir)];
    return loaded?.state === "ready" ? loaded.listing.entries.filter((e) => e.isDir) : [];
  };
  const currentEntries = $derived(current?.state === "ready" ? current.listing.entries : []);
  const parsedFilter = $derived(parseFileFilter(filterQuery));
  const visibleEntries = $derived(
    parsedFilter.isActive ? currentEntries.filter((e) => matchesFileFilter(e, parsedFilter)) : currentEntries
  );
  const sortedEntries = $derived(
    sortFileEntries(visibleEntries, sortColumn, sortAsc, statusLabel)
  );
  $effect(() => {
    matchedCount = visibleEntries.length;
    totalCount = currentEntries.length;
  });

  function toggleSort(col: FileSortColumn) {
    if (sortColumn === col) {
      sortAsc = !sortAsc;
    } else {
      sortColumn = col;
      sortAsc = true;
    }
  }

  /** The name a column is labelled with: its folder's, or "Folders" at the root. */
  function columnLabel(dir: string[]): string {
    if (dir.length === 0) return i18n.t("files.folders");
    const loaded = listings[keyOf(dir)];
    const missingTag = loaded?.state === "ready" ? loaded.listing.pathMissingTags[dir.length - 1] : null;
    return shownName(dir[dir.length - 1], missingTag);
  }

  // Keyboard focus: which column, and its virtual list for scrolling.
  let columnEls = $state<(HTMLElement | null)[]>([]);
  let lists = $state<(any | null)[]>([]);
  let starts = $state<number[]>([]);
  let ends = $state<number[]>([]);

  function scrollIntoView(column: number, index: number) {
    const list = lists[column];
    if (!list || index < 0) return;
    const start = starts[column] ?? 0;
    const end = ends[column] ?? 0;
    // `end` is one past the last row drawn, which may be cut off.
    if (index < start) list.scrollToIndex(index, { behavior: "auto" });
    else if (index >= end - 1) list.scrollToIndex(Math.max(0, index - Math.max(1, end - start - 2)), { behavior: "auto" });
  }

  async function focusColumn(column: number) {
    await tick();
    columnEls[column]?.focus();
  }

  /** Parent columns give way to the table as the browser narrows (CSS hides them). */
  function isShown(column: number): boolean {
    const el = columnEls[column];
    return !!el && getComputedStyle(el).display !== "none";
  }

  function openFolder(dir: string[], name: string) {
    onnavigate([...dir, name]);
  }

  function handleParentKey(event: KeyboardEvent, column: number, dir: string[]) {
    const folders = folderEntries(dir);
    const selected = path[dir.length];
    const index = folders.findIndex((f) => f.name === selected);
    let handled = true;
    switch (event.key) {
      case "Backspace":
        if (dir.length > 0) onnavigate(dir);
        break;
      case "ArrowUp": {
        if (event.altKey && dir.length > 0) {
          onnavigate(dir);
          break;
        }
        const next = folders[Math.min(folders.length - 1, Math.max(0, index - 1))];
        if (next) {
          onnavigate([...dir, next.name]);
          scrollIntoView(column, folders.indexOf(next));
        }
        break;
      }
      case "ArrowDown": {
        const next = folders[Math.min(folders.length - 1, Math.max(0, index + 1))];
        if (next) {
          onnavigate([...dir, next.name]);
          scrollIntoView(column, folders.indexOf(next));
        }
        break;
      }
      case "ArrowRight":
      case "Enter":
        focusColumn(column + 1);
        break;
      case "ArrowLeft":
        if (column > 0 && isShown(column - 1)) focusColumn(column - 1);
        else if (dir.length > 0) onnavigate(dir);
        break;
      default:
        handled = false;
    }
    if (handled) event.preventDefault();
  }

  function handleCurrentKey(event: KeyboardEvent, column: number) {
    const entries = sortedEntries;
    const index = entries.findIndex((e) => e.name === selectedName);
    const entry = entries[index];
    if ((event.ctrlKey || event.metaKey) && event.key.toLowerCase() === "a") {
      event.preventDefault();
      selection = entries.filter((e) => !e.isDir).map((e) => e.name);
      return;
    }
    let handled = true;
    switch (event.key) {
      case "Backspace":
        if (path.length > 0) onnavigate(path.slice(0, -1));
        break;
      case "ArrowUp": {
        if (event.altKey && path.length > 0) {
          onnavigate(path.slice(0, -1));
          break;
        }
        const nextIndex = Math.min(entries.length - 1, Math.max(0, index - 1));
        const next = entries[nextIndex];
        if (next) {
          if (event.shiftKey && anchor !== null && !next.isDir) {
            selectRange(next.name);
          } else {
            select(next.name);
          }
          scrollIntoView(column, nextIndex);
        }
        break;
      }
      case "ArrowDown": {
        const nextIndex = Math.min(entries.length - 1, Math.max(0, index + 1));
        const next = entries[nextIndex];
        if (next) {
          if (event.shiftKey && anchor !== null && !next.isDir) {
            selectRange(next.name);
          } else {
            select(next.name);
          }
          scrollIntoView(column, nextIndex);
        }
        break;
      }
      case "ArrowRight":
        if (entry?.isDir) openFolder(path, entry.name);
        break;
      case "Enter":
        if (entry?.isDir) openFolder(path, entry.name);
        else if (entry) onactivate(entry);
        break;
      case "ArrowLeft":
        if (column > 0 && isShown(column - 1)) focusColumn(column - 1);
        else if (path.length > 0) onnavigate(path.slice(0, -1));
        break;
      default:
        handled = false;
    }
    if (handled) event.preventDefault();
  }

  // Arriving somewhere selects `target`; without one, coming back up a level
  // selects the folder just left and any other move selects nothing. A new
  // `target` in the same folder is an arrival too.
  let previousPath: string[] = [];
  let previousTarget: string | null = null;
  $effect(() => {
    const now = path;
    const wanted = target;
    const moved = keyOf(now) !== keyOf(previousPath);
    if (wanted !== null && (moved || wanted !== previousTarget)) {
      select(wanted);
      scrollPending = wanted;
    } else if (now.length < previousPath.length && keyOf(previousPath.slice(0, now.length)) === keyOf(now)) {
      select(previousPath[now.length]);
    } else if (moved) {
      select(null);
    }
    previousPath = now;
    previousTarget = wanted;
  });

  /** An entry to bring into view once the current folder's listing is read. */
  let scrollPending = $state<string | null>(null);
  $effect(() => {
    const name = scrollPending;
    if (name === null || current?.state !== "ready") return;
    scrollPending = null;
    const index = sortedEntries.findIndex((e) => e.name === name);
    if (index < 0) return;
    centreRow(levels.length - 1, index);
    if (haloArmed && name === arrived) showHalo();
  });

  // Each parent column, when it mounts, brings the folder open in it into
  // view; later moves leave it where the user scrolled it.
  const centredParents = new Set<string>();
  $effect(() => {
    const parents = levels.slice(0, -1);
    const keys = new Set(parents.map(keyOf));
    for (const key of [...centredParents]) if (!keys.has(key)) centredParents.delete(key);
    parents.forEach((dir, column) => {
      const key = keyOf(dir);
      if (centredParents.has(key) || listings[key]?.state !== "ready" || !lists[column]) return;
      centredParents.add(key);
      const index = folderEntries(dir).findIndex((f) => f.name === path[dir.length]);
      if (index >= 0) centreRow(column, index);
    });
  });

  /** Scrolls a column so row `index` sits mid-view. Unlike `scrollIntoView`,
   * this doesn't rely on the rows drawn so far, which are none yet on arrival. */
  async function centreRow(column: number, index: number) {
    await tick();
    const list = lists[column];
    const el = columnEls[column];
    if (!list || !el) return;
    const rowsShown = Math.floor(el.clientHeight / ROW_HEIGHT_PX);
    list.scrollToIndex(Math.max(0, index - Math.floor(rowsShown / 2)), { behavior: "auto" });
  }

  const HALO_MS = 1200;
  /** The entry `arrival` last named, outlined while it is the only selection. */
  let arrived = $state<string | null>(null);
  /** Its halo waits for its row: a slow source can take longer than the halo lasts. */
  let haloArmed = false;
  let haloShown = $state(false);
  let haloTimer: ReturnType<typeof setTimeout> | undefined;

  // An arrival selects its entry even when `target` already named it (View in
  // Files twice on the same song, with another row clicked in between).
  $effect(() => {
    if (arrival === null) return;
    const name = arrival;
    arrival = null;
    arrived = name;
    select(name);
    haloArmed = true;
    scrollPending = name;
  });

  function showHalo() {
    haloArmed = false;
    haloShown = true;
    clearTimeout(haloTimer);
    haloTimer = setTimeout(() => (haloShown = false), HALO_MS);
  }
  $effect(() => () => clearTimeout(haloTimer));

  /** Where a Shift range starts: the entry last picked without Shift. */
  let anchor: string | null = null;

  /** Makes `name` the only selected entry. */
  function select(name: string | null) {
    if (name !== arrived) arrived = null;
    selectedName = name;
    anchor = name;
    selection = name === null ? [] : [name];
  }

  /** Selects the files from the anchor to `name`, skipping folders between, and focuses `name`. */
  function selectRange(name: string) {
    const names = sortedEntries.map((e) => e.name);
    const [a, b] = [names.indexOf(anchor ?? name), names.indexOf(name)].sort((x, y) => x - y);
    if (a < 0) return select(name);
    selection = sortedEntries.slice(a, b + 1).filter((e) => !e.isDir).map((e) => e.name);
    selectedName = name;
    arrived = null;
  }

  /** A click on a file: Ctrl/Cmd adds or drops it, Shift selects the run from the anchor. */
  function clickFile(event: MouseEvent, name: string) {
    if (event.shiftKey && anchor !== null) return selectRange(name);
    if (event.ctrlKey || event.metaKey) {
      selection = selection.includes(name) ? selection.filter((n) => n !== name) : [...selection, name];
      selectedName = name;
      anchor = name;
      arrived = null;
      return;
    }
    select(name);
  }

  let fileDrag: PointerDragHandle | null = null;

  // A file row's click lives on the row itself, which takes the helper's capture-on-press.
  function handleRowPointerDown(e: PointerEvent, entry: FileEntry) {
    if (entry.isDir) return;
    fileDrag = startPointerDrag(e, {
      onArm: (pos) => {
        const dragged = selection.includes(entry.name)
          ? sortedEntries.filter((ent) => !ent.isDir && selection.includes(ent.name))
          : [entry];
        if (!selection.includes(entry.name)) select(entry.name);
        songDrag.start(filesPayload(dragged), pos);
      },
      onMove: (pos) => songDrag.move(pos),
      onDrop: async (pos) => {
        await songDrag.drop(pos);
      },
      onCancel: (wasArmed) => {
        if (wasArmed) songDrag.end();
      }
    });
  }

  $effect(() => () => fileDrag?.cancel());

  /** A right-click on a folder acts on it alone; on a file, on the selection it belongs to. */
  function openMenu(event: MouseEvent, entry: FileEntry) {
    if (!oncontextmenu) return;
    event.preventDefault();
    if (entry.isDir || !selection.includes(entry.name)) select(entry.name);
    const entries = entry.isDir ? [entry] : sortedEntries.filter((e) => !e.isDir && selection.includes(e.name));
    oncontextmenu({ dir: path, entries, inColumn: false }, event.clientX, event.clientY);
  }

  function openColumnMenu(event: MouseEvent, dir: string[], entry: FileEntry) {
    if (!oncontextmenu) return;
    event.preventDefault();
    oncontextmenu({ dir, entries: [entry], inColumn: true }, event.clientX, event.clientY);
  }
</script>

<div class="@container flex-1 min-h-0 flex rounded-xl border border-brand-border/40 bg-brand-sidebar/40 overflow-hidden" data-testid="file-browser">
  {#each levels as dir, column (keyOf(dir))}
    {@const isCurrent = column === levels.length - 1}
    {@const loaded = listings[keyOf(dir)]}
    {#if !isCurrent}
      {@const visibility = column === levels.length - 2 ? "hidden @xl:flex" : "hidden @3xl:flex"}
      <div
        bind:this={columnEls[column]}
        style:width="{columnWidth}px"
        class="shrink-0 min-h-0 {visibility} flex-col border-r border-brand-border/40 outline-none focus-visible:ring-2 focus-visible:ring-inset focus-visible:ring-brand-accent/50"
        role="listbox"
        tabindex="0"
        aria-label={columnLabel(dir)}
        onkeydown={(e) => handleParentKey(e, column, dir)}
      >
        <div class="flex-1 min-h-0 p-1.5">
          <VirtualList items={folderEntries(dir)} height="100%" itemHeight={ROW_HEIGHT_PX} bind:this={lists[column]} bind:start={starts[column]} bind:end={ends[column]}>
            {#snippet children({ item: entry }: { item: FileEntry })}
              {@const selected = path[dir.length] === entry.name}
              <button
                class="w-full h-[31px] flex items-center gap-2 px-2.5 rounded-md text-[13px] text-left transition-colors {selected ? 'bg-brand-accent/20 text-brand-text-primary font-semibold' : 'text-brand-text-primary hover:bg-brand-accent/10'} {MUTED.includes(entry.status) ? 'opacity-60' : ''}"
                role="option"
                aria-selected={selected}
                tabindex="-1"
                onclick={() => openFolder(dir, entry.name)}
                oncontextmenu={(e) => openColumnMenu(e, dir, entry)}
              >
                <FolderIcon class="w-4 h-4 shrink-0 text-brand-text-secondary" />
                <span class="truncate flex-1">{shownName(entry.name, entry.missingTag)}</span>
                {#if entry.status === "excluded" || entry.status === "notFromLuminous"}
                  <span class="text-[11px] text-brand-text-secondary shrink-0">{statusLabel(entry)}</span>
                {:else}
                  <CaretRightIcon class="w-3 h-3 shrink-0 text-brand-text-secondary" />
                {/if}
              </button>
            {/snippet}
          </VirtualList>
        </div>
        {#if loaded?.state === "error"}
          <p class="p-3 text-xs text-brand-text-secondary">{unreachableLabel}</p>
        {/if}
      </div>
      <!-- Drag, or focus and use the arrow keys; it sits over the column's border. -->
      <!-- svelte-ignore a11y_no_noninteractive_tabindex -->
      <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
      <div
        class="{visibility} relative z-10 w-2 -ml-2 shrink-0 cursor-col-resize touch-none justify-end outline-none group/resize focus-visible:bg-brand-accent/20"
        role="separator"
        aria-orientation="vertical"
        aria-label={i18n.t("files.resizeFolders")}
        aria-valuenow={columnWidth}
        aria-valuemin={COLUMN_MIN_PX}
        aria-valuemax={COLUMN_MAX_PX}
        tabindex="0"
        onpointerdown={startColumnResize}
        onkeydown={columnResizeKey}
        ondblclick={() => setColumnWidth(COLUMN_DEFAULT_PX)}
      >
        <div class="w-px h-full group-hover/resize:bg-brand-accent group-focus-visible/resize:bg-brand-accent transition-colors"></div>
      </div>
    {:else}
      <div class="@container flex-1 min-w-0 min-h-0 flex flex-col">
        <div class="h-8 shrink-0 flex items-center gap-3 px-4 border-b border-brand-border/40 text-[11px] text-brand-text-secondary select-none">
          <button
            type="button"
            onclick={() => toggleSort("name")}
            class="flex-1 min-w-0 pl-6 text-left hover:text-brand-text-primary transition-colors flex items-center gap-1 focus-visible:outline-none focus-visible:text-brand-accent-text"
            aria-label={i18n.t("files.columnName")}
          >
            <span class="truncate">{i18n.t("files.columnName")}</span>
            {#if sortColumn === "name"}
              <span class="text-[9px] shrink-0" aria-hidden="true">{sortAsc ? "▲" : "▼"}</span>
            {/if}
          </button>
          <button
            type="button"
            onclick={() => toggleSort("fileType")}
            class="w-12 shrink-0 hidden @lg:flex items-center gap-1 text-left hover:text-brand-text-primary transition-colors focus-visible:outline-none focus-visible:text-brand-accent-text"
            aria-label={i18n.t("files.columnType")}
          >
            <span class="truncate">{i18n.t("files.columnType")}</span>
            {#if sortColumn === "fileType"}
              <span class="text-[9px] shrink-0" aria-hidden="true">{sortAsc ? "▲" : "▼"}</span>
            {/if}
          </button>
          <button
            type="button"
            onclick={() => toggleSort("size")}
            class="w-16 shrink-0 hidden @lg:flex items-center justify-end gap-1 text-right hover:text-brand-text-primary transition-colors focus-visible:outline-none focus-visible:text-brand-accent-text"
            aria-label={i18n.t("files.columnSize")}
          >
            <span class="truncate">{i18n.t("files.columnSize")}</span>
            {#if sortColumn === "size"}
              <span class="text-[9px] shrink-0" aria-hidden="true">{sortAsc ? "▲" : "▼"}</span>
            {/if}
          </button>
          <button
            type="button"
            onclick={() => toggleSort("status")}
            class="w-40 shrink-0 hidden @md:flex items-center gap-1 text-left hover:text-brand-text-primary transition-colors focus-visible:outline-none focus-visible:text-brand-accent-text"
            aria-label={i18n.t("files.columnInLibrary")}
          >
            <span class="truncate">{i18n.t("files.columnInLibrary")}</span>
            {#if sortColumn === "status"}
              <span class="text-[9px] shrink-0" aria-hidden="true">{sortAsc ? "▲" : "▼"}</span>
            {/if}
          </button>
        </div>
        {#if loaded?.state === "error"}
          <div class="flex-1 flex flex-col items-center justify-center gap-3 p-6 text-center">
            <p class="text-sm text-brand-text-secondary">{loaded.code === "source-not-found" ? i18n.t("files.sourceGone") : unreachableLabel}</p>
            {#if loaded.code !== "source-not-found"}
              <Button variant="secondary" onclick={() => retry(dir)}>
                <ArrowClockwiseIcon class="w-4 h-4" />
                {i18n.t("files.retry")}
              </Button>
            {/if}
          </div>
        {:else if loaded?.state === "ready" && currentEntries.length === 0}
          <p class="flex-1 flex items-center justify-center text-sm text-brand-text-secondary">{i18n.t("files.emptyFolder")}</p>
        {:else if loaded?.state === "ready" && visibleEntries.length === 0}
          <div class="flex-1 flex flex-col items-center justify-center gap-2 p-6 text-center" data-testid="files-filter-empty">
            <p class="text-sm font-semibold text-brand-text-primary">
              {i18n.t("files.filterEmptyTitle", { query: filterQuery.trim() })}
            </p>
            <p class="text-xs text-brand-text-secondary max-w-md">
              {i18n.plural("files.filterEmptyHint", currentEntries.length)}
            </p>
            <div class="mt-2">
              <Button variant="secondary" onclick={() => (filterQuery = "")}>
                {i18n.t("files.clearFilter")}
              </Button>
            </div>
          </div>
        {:else}
          <div class="relative flex-1 min-h-0 flex">
            <div
              bind:this={columnEls[column]}
              class="flex-1 min-h-0 p-1.5 outline-none focus-visible:ring-2 focus-visible:ring-inset focus-visible:ring-brand-accent/50"
              role="listbox"
              tabindex="0"
              aria-label={columnLabel(dir)}
              onkeydown={(e) => handleCurrentKey(e, column)}
            >
              <VirtualList items={sortedEntries} height="100%" itemHeight={ROW_HEIGHT_PX} bind:this={lists[column]} bind:start={starts[column]} bind:end={ends[column]}>
                {#snippet children({ item: entry }: { item: FileEntry })}
                  {@const selected = selection.includes(entry.name)}
                  {@const muted = MUTED.includes(entry.status)}
                  {@const name = shownName(entry.name, entry.missingTag)}
                  {@const isArrival = entry.name === arrived && selected && selection.length === 1}
                  {@const Icon = fileIcon(entry)}
                  <div
                    class="w-full h-[31px] flex items-center gap-3 px-2.5 rounded-md text-[13px] cursor-default transition-colors {selected ? 'bg-brand-accent/20' : 'hover:bg-brand-accent/10'} {songDrag.isDragging && selected ? 'opacity-60' : ''}"
                    class:arrival={isArrival}
                    class:arrival-halo={isArrival && haloShown}
                    role="option"
                    aria-selected={selected}
                    tabindex="-1"
                    onpointerdown={(e) => handleRowPointerDown(e, entry)}
                    onclick={(e) => {
                      if (entry.isDir) openFolder(dir, entry.name);
                      else clickFile(e, entry.name);
                    }}
                    ondblclick={() => { if (!entry.isDir) onactivate(entry); }}
                    oncontextmenu={(e) => openMenu(e, entry)}
                    onkeydown={() => {}}
                  >
                    <span class="flex-1 min-w-0 flex items-center gap-2 {muted ? 'text-brand-text-secondary' : 'text-brand-text-primary'} {selected ? 'font-semibold' : ''}">
                      <Icon class="w-4 h-4 shrink-0 text-brand-text-secondary" data-kind={entry.kind ?? "folder"} />
                      <span class="truncate" title={name}>
                        {#if parsedFilter.terms.length > 0}
                          {#each highlightSegments(name, parsedFilter.terms) as seg, segIdx (segIdx)}
                            {#if seg.match}
                              <mark class="bg-brand-accent/25 text-brand-accent-text font-semibold rounded-xs">{seg.text}</mark>
                            {:else}
                              {seg.text}
                            {/if}
                          {/each}
                        {:else}
                          {name}
                        {/if}
                      </span>
                    </span>
                    <span class="w-12 shrink-0 text-[11px] text-brand-text-secondary hidden @lg:block">{entry.fileType ?? ""}</span>
                    <span class="w-16 shrink-0 text-right text-[11px] tabular-nums text-brand-text-secondary hidden @lg:block">{entry.isDir || entry.size === null ? "" : formatFileSize(entry.size)}</span>
                    <span class="w-40 shrink-0 truncate text-xs hidden @md:block {entry.status === 'notScanned' ? 'text-brand-accent-text' : entry.status === 'inLibrary' ? 'text-brand-text-primary' : 'text-brand-text-secondary'}">{statusLabel(entry)}</span>
                  </div>
                {/snippet}
              </VirtualList>
            </div>
            {#if sortColumn === "name"}
              <div class="absolute right-2 top-2 bottom-2 pointer-events-auto z-20">
                <AlphabetScrubber
                  items={sortedEntries}
                  getKey={(entry) => shownName(entry.name, entry.missingTag)}
                  ascending={sortAsc}
                  locale={i18n.currentLocale}
                  onjump={(index) => {
                    lists[column]?.scrollToIndex(index, { behavior: "auto" });
                  }}
                />
              </div>
            {/if}
          </div>
        {/if}
        {#if footer}
          <div class="shrink-0 border-t border-brand-border/40">{@render footer()}</div>
        {/if}
      </div>
    {/if}
  {/each}
</div>

<style>
  /* View in Files (#1883): the entry arrived at is outlined while it stays the
     only selection. Its halo shows at once and fades on removal (the transition
     lives on the resting state, so only the exit animates). Reduced motion keeps
     just the outline. The halo is inset: the list clips anything outside a row. */
  .arrival {
    outline: 2px solid var(--color-brand-accent);
    outline-offset: -2px;
    transition: background-color 150ms ease, box-shadow 300ms ease-out;
  }
  .arrival-halo {
    box-shadow: inset 0 0 0 5px color-mix(in srgb, var(--color-brand-accent) 30%, transparent);
    transition: background-color 150ms ease;
  }
  @media (prefers-reduced-motion: reduce) {
    .arrival-halo {
      box-shadow: none;
    }
  }
</style>
