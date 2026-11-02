<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { tick, type Snippet } from "svelte";
  import { VirtualList } from "svelte-virtual-list-ts";
  import { FolderIcon, FileIcon, MusicNoteIcon, ImageIcon, CaretRightIcon, ArrowClockwiseIcon } from "phosphor-svelte";
  import { i18n } from "../../stores/i18n.svelte";
  import { formatFileSize } from "../../utils/formatters";
  import type { DirListing, FileEntry, FileEntryStatus, FileSource } from "../../types";
  import Button from "../Button.svelte";
  import { shownName } from "./shownName";

  /**
   * A column browser over one source (#1841): up to two columns of the
   * folders above the current one, then the current folder's folders and
   * files as a table. It reads listings itself; where it is (`path`) and
   * what happens on navigate and play belong to the caller. Reused by the
   * device Files tab (#839).
   */
  let {
    source,
    path,
    onnavigate,
    onplay,
    reloadToken = 0,
    listing = $bindable(null),
    selectedName = $bindable(null),
    selection = $bindable([]),
    unreachableLabel,
    footer,
  }: {
    source: FileSource;
    /** The current folder, as names from the source's root. */
    path: string[];
    onnavigate: (path: string[]) => void;
    /** Enter or double-click on a file in the current folder. */
    onplay: (entry: FileEntry) => void;
    /** Bump to read every listing again (after a scan, say). */
    reloadToken?: number;
    /** Out: the current folder's listing once read. */
    listing?: DirListing | null;
    /** The focused entry of the current folder, by name. */
    selectedName?: string | null;
    /** Every selected entry of the current folder, by name: the focused one, plus
     * those added with Ctrl/Cmd-click or Shift-click. */
    selection?: string[];
    /** Shown when the source can't be reached. */
    unreachableLabel: string;
    footer?: Snippet;
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
      case "ArrowDown":
      case "ArrowUp": {
        const next = folders[Math.min(folders.length - 1, Math.max(0, index + (event.key === "ArrowDown" ? 1 : -1)))];
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
    const entries = currentEntries;
    const index = entries.findIndex((e) => e.name === selectedName);
    const entry = entries[index];
    let handled = true;
    switch (event.key) {
      case "ArrowDown":
      case "ArrowUp": {
        const nextIndex = Math.min(entries.length - 1, Math.max(0, index + (event.key === "ArrowDown" ? 1 : -1)));
        if (entries[nextIndex]) {
          select(entries[nextIndex].name);
          scrollIntoView(column, nextIndex);
        }
        break;
      }
      case "ArrowRight":
        if (entry?.isDir) openFolder(path, entry.name);
        break;
      case "Enter":
        if (entry?.isDir) openFolder(path, entry.name);
        else if (entry) onplay(entry);
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

  // Coming back up a level selects the folder just left.
  let previousPath: string[] = [];
  $effect(() => {
    const now = path;
    if (now.length < previousPath.length && keyOf(previousPath.slice(0, now.length)) === keyOf(now)) {
      select(previousPath[now.length]);
    } else if (keyOf(now) !== keyOf(previousPath)) {
      select(null);
    }
    previousPath = now;
  });

  /** Makes `name` the only selected entry. */
  function select(name: string | null) {
    selectedName = name;
    selection = name === null ? [] : [name];
  }

  /** A click on a file: Ctrl/Cmd adds or drops it, Shift selects the run from the focused entry. */
  function clickFile(event: MouseEvent, name: string) {
    if (event.shiftKey && selectedName !== null) {
      const names = currentEntries.map((e) => e.name);
      const [a, b] = [names.indexOf(selectedName), names.indexOf(name)].sort((x, y) => x - y);
      if (a >= 0) {
        selection = names.slice(a, b + 1);
        return;
      }
    }
    if (event.ctrlKey || event.metaKey) {
      selection = selection.includes(name) ? selection.filter((n) => n !== name) : [...selection, name];
      selectedName = name;
      return;
    }
    select(name);
  }

  function statusLabel(entry: FileEntry): string {
    return entry.status ? i18n.t(STATUS_LABEL_KEYS[entry.status]) : "";
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
        <div class="h-8 shrink-0 flex items-center gap-3 px-4 border-b border-brand-border/40 text-[11px] text-brand-text-secondary" aria-hidden="true">
          <span class="flex-1 min-w-0 pl-6">{i18n.t("files.columnName")}</span>
          <span class="w-12 shrink-0 hidden @lg:block">{i18n.t("files.columnType")}</span>
          <span class="w-16 shrink-0 text-right hidden @lg:block">{i18n.t("files.columnSize")}</span>
          <span class="w-40 shrink-0 hidden @md:block">{i18n.t("files.columnInLibrary")}</span>
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
        {:else}
          <div
            bind:this={columnEls[column]}
            class="flex-1 min-h-0 p-1.5 outline-none focus-visible:ring-2 focus-visible:ring-inset focus-visible:ring-brand-accent/50"
            role="listbox"
            tabindex="0"
            aria-label={columnLabel(dir)}
            onkeydown={(e) => handleCurrentKey(e, column)}
          >
            <VirtualList items={currentEntries} height="100%" itemHeight={ROW_HEIGHT_PX} bind:this={lists[column]} bind:start={starts[column]} bind:end={ends[column]}>
              {#snippet children({ item: entry }: { item: FileEntry })}
                {@const selected = selection.includes(entry.name)}
                {@const muted = MUTED.includes(entry.status)}
                {@const name = shownName(entry.name, entry.missingTag)}
                <div
                  class="w-full h-[31px] flex items-center gap-3 px-2.5 rounded-md text-[13px] cursor-default transition-colors {selected ? 'bg-brand-accent/20' : 'hover:bg-brand-accent/10'}"
                  role="option"
                  aria-selected={selected}
                  tabindex="-1"
                  onclick={(e) => (entry.isDir ? openFolder(dir, entry.name) : clickFile(e, entry.name))}
                  ondblclick={() => { if (!entry.isDir) onplay(entry); }}
                  onkeydown={() => {}}
                >
                  <span class="flex-1 min-w-0 flex items-center gap-2 {muted ? 'text-brand-text-secondary' : 'text-brand-text-primary'} {selected ? 'font-semibold' : ''}">
                    {#if entry.isDir}
                      <FolderIcon class="w-4 h-4 shrink-0 text-brand-text-secondary" />
                    {:else if entry.status === "albumArt"}
                      <ImageIcon class="w-4 h-4 shrink-0 text-brand-text-secondary" />
                    {:else if entry.status === "inLibrary" || entry.status === "notScanned"}
                      <MusicNoteIcon class="w-4 h-4 shrink-0 text-brand-text-secondary" />
                    {:else}
                      <FileIcon class="w-4 h-4 shrink-0 text-brand-text-secondary" />
                    {/if}
                    <span class="truncate" title={name}>{name}</span>
                  </span>
                  <span class="w-12 shrink-0 text-[11px] text-brand-text-secondary hidden @lg:block">{entry.fileType ?? ""}</span>
                  <span class="w-16 shrink-0 text-right text-[11px] tabular-nums text-brand-text-secondary hidden @lg:block">{entry.isDir || entry.size === null ? "" : formatFileSize(entry.size)}</span>
                  <span class="w-40 shrink-0 truncate text-xs hidden @md:block {entry.status === 'notScanned' ? 'text-brand-accent-text' : entry.status === 'inLibrary' ? 'text-brand-text-primary' : 'text-brand-text-secondary'}">{statusLabel(entry)}</span>
                </div>
              {/snippet}
            </VirtualList>
          </div>
        {/if}
        {#if footer}
          <div class="shrink-0 border-t border-brand-border/40">{@render footer()}</div>
        {/if}
      </div>
    {/if}
  {/each}
</div>
