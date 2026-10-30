<script lang="ts" module>
  import type { FileEntryStatus } from "../../types";

  /** The words for a status; the backend decides which one a file has. */
  export const STATUS_LABEL_KEYS: Record<FileEntryStatus, string> = {
    inLibrary: "files.status.inLibrary",
    notScanned: "files.status.notScanned",
    albumArt: "files.status.albumArt",
    notAudio: "files.status.notAudio",
    excluded: "files.status.excluded",
  };
</script>

<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { tick, type Snippet } from "svelte";
  import { VirtualList } from "svelte-virtual-list-ts";
  import { FolderIcon, FileIcon, MusicNoteIcon, ImageIcon, CaretRightIcon, ArrowClockwiseIcon } from "phosphor-svelte";
  import { i18n } from "../../stores/i18n.svelte";
  import { formatFileSize } from "../../utils/formatters";
  import type { DirListing, FileEntry, FileSource } from "../../types";
  import Button from "../Button.svelte";

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
    /** The selected entry of the current folder, by name. */
    selectedName?: string | null;
    /** Shown when the source can't be reached. */
    unreachableLabel: string;
    footer?: Snippet;
  } = $props();

  const ROW_HEIGHT_PX = 31;
  /** Folder columns shown before the current folder's table. */
  const PARENT_COLUMNS = 2;

  type Loaded = { state: "loading" } | { state: "ready"; listing: DirListing } | { state: "error"; code: string };

  let listings = $state<Record<string, Loaded>>({});
  let cacheOwner = "";

  const keyOf = (dir: string[]) => dir.join("\u0000");

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
          selectedName = entries[nextIndex].name;
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
      selectedName = previousPath[now.length];
    } else if (keyOf(now) !== keyOf(previousPath)) {
      selectedName = null;
    }
    previousPath = now;
  });

  function statusLabel(entry: FileEntry): string {
    return entry.status ? i18n.t(STATUS_LABEL_KEYS[entry.status]) : "";
  }
</script>

<div class="@container flex-1 min-h-0 flex rounded-xl border border-brand-border/40 bg-brand-sidebar/40 overflow-hidden" data-testid="file-browser">
  {#each levels as dir, column (keyOf(dir))}
    {@const isCurrent = column === levels.length - 1}
    {@const loaded = listings[keyOf(dir)]}
    {#if !isCurrent}
      <div
        bind:this={columnEls[column]}
        class="w-56 shrink-0 min-h-0 {column === levels.length - 2 ? 'hidden @xl:flex' : 'hidden @3xl:flex'} flex-col border-r border-brand-border/40 outline-none focus-visible:ring-2 focus-visible:ring-inset focus-visible:ring-brand-accent/50"
        role="listbox"
        tabindex="0"
        aria-label={dir.length ? dir[dir.length - 1] : i18n.t("files.folders")}
        onkeydown={(e) => handleParentKey(e, column, dir)}
      >
        <div class="flex-1 min-h-0 p-1.5">
          <VirtualList items={folderEntries(dir)} height="100%" itemHeight={ROW_HEIGHT_PX} bind:this={lists[column]} bind:start={starts[column]} bind:end={ends[column]}>
            {#snippet children({ item: entry }: { item: FileEntry })}
              {@const selected = path[dir.length] === entry.name}
              <button
                class="w-full h-[31px] flex items-center gap-2 px-2.5 rounded-md text-[13px] text-left transition-colors {selected ? 'bg-brand-accent/20 text-brand-text-primary font-semibold' : 'text-brand-text-primary hover:bg-brand-accent/10'} {entry.status === 'excluded' ? 'opacity-60' : ''}"
                role="option"
                aria-selected={selected}
                tabindex="-1"
                onclick={() => openFolder(dir, entry.name)}
              >
                <FolderIcon class="w-4 h-4 shrink-0 text-brand-text-secondary" />
                <span class="truncate flex-1">{entry.name}</span>
                {#if entry.status === "excluded"}
                  <span class="text-[11px] text-brand-text-secondary shrink-0">{i18n.t("files.status.excluded")}</span>
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
            aria-label={dir.length ? dir[dir.length - 1] : i18n.t("files.folders")}
            onkeydown={(e) => handleCurrentKey(e, column)}
          >
            <VirtualList items={currentEntries} height="100%" itemHeight={ROW_HEIGHT_PX} bind:this={lists[column]} bind:start={starts[column]} bind:end={ends[column]}>
              {#snippet children({ item: entry }: { item: FileEntry })}
                {@const selected = selectedName === entry.name}
                {@const muted = entry.status === "excluded" || entry.status === "albumArt" || entry.status === "notAudio"}
                <div
                  class="w-full h-[31px] flex items-center gap-3 px-2.5 rounded-md text-[13px] cursor-default transition-colors {selected ? 'bg-brand-accent/20' : 'hover:bg-brand-accent/10'}"
                  role="option"
                  aria-selected={selected}
                  tabindex="-1"
                  onclick={() => (entry.isDir ? openFolder(dir, entry.name) : (selectedName = entry.name))}
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
                    <span class="truncate" title={entry.name}>{entry.name}</span>
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
