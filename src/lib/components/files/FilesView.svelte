<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import {
    PlayIcon,
    DotsThreeIcon,
    PlusIcon,
    CaretRightIcon,
  } from "phosphor-svelte";
  import { navigationStore } from "../../stores/navigation.svelte";
  import { filesStore, sourceKey } from "../../stores/files.svelte";
  import { sourceIcon } from "./sourceIcon";
  import { shownName } from "./shownName";
  import { collectionStore } from "../../stores/collection.svelte";
  import { playerStore } from "../../stores/player.svelte";
  import { toastStore } from "../../stores/toast.svelte";
  import { i18n } from "../../stores/i18n.svelte";
  import { formatFileSize } from "../../utils/formatters";
  import { CONTEXT_MENU_WIDTH_PX } from "../../constants";
  import type { DirListing, FileEntry, FileMenuTarget } from "../../types";
  import Button from "../Button.svelte";
  import TagEditor from "../TagEditor.svelte";
  import AlbumTagEditor from "../AlbumTagEditor.svelte";
  import FileBrowser from "./FileBrowser.svelte";
  import FileContextMenu from "./FileContextMenu.svelte";
  import DeviceView from "../devices/DeviceView.svelte";
  import { openFile, playFolder, scanFolder } from "./folderActions";

  const location = $derived(navigationStore.filesLocation);
  const openDevice = $derived(
    location?.source.kind === "device" ? filesStore.devices.find((d) => d.id === location.source.id) : undefined
  );
  const sourceName = $derived(location ? filesStore.name(location.source) : "");

  let listing = $state<DirListing | null>(null);
  let selectedName = $state<string | null>(null);
  let selection = $state<string[]>([]);
  let menu = $state<(FileMenuTarget & { x: number; y: number; fromHeader?: boolean }) | null>(null);
  let headerMenuButtonEl = $state<HTMLButtonElement | null>(null);
  let editingSongId = $state<number | null>(null);
  let editingFile = $state<{ path: string; name: string; folderPath: string[]; folderName: string } | null>(null);
  let editingFolder = $state<{ folderName: string; songIds: number[]; filePaths: string[]; folderPath: string[] } | null>(null);
  let editingBatch = $state<{ songIds: number[]; filePaths: string[]; folderPath: string[] } | null>(null);

  function toggleHeaderMenu() {
    if (menu?.fromHeader) {
      menu = null;
      return;
    }
    if (!headerMenuButtonEl) return;
    const rect = headerMenuButtonEl.getBoundingClientRect();
    menu = {
      x: rect.right - CONTEXT_MENU_WIDTH_PX,
      y: rect.bottom + 6,
      dir: location?.path ?? [],
      entries: [],
      inColumn: false,
      fromHeader: true,
    };
  }

  async function editFolderTags(dir: FileEntry, dirPath: string[]) {
    if (!location) return;
    try {
      const dirListing = await invoke<DirListing>("list_source_dir", {
        source: location.source,
        path: dirPath,
      });
      const audioEntries = dirListing.entries.filter(
        (e) => !e.isDir && (e.kind === "audio" || e.status === "inLibrary" || e.status === "notScanned")
      );
      const songIds = audioEntries.flatMap((e) => (e.songId !== null ? [e.songId] : []));
      const filePaths = audioEntries.flatMap((e) => (e.songId === null && e.location !== null ? [e.location] : []));
      if (songIds.length === 0 && filePaths.length === 0) {
        toastStore.show(i18n.t("files.nothingToPlay"), "info");
        return;
      }
      editingFolder = {
        folderName: shownName(dir.name, dir.missingTag),
        songIds,
        filePaths,
        folderPath: dirPath,
      };
    } catch (err) {
      console.error("Failed to list folder for tag editing:", err);
    }
  }

  function editBatchTags(entries: FileEntry[]) {
    if (!location) return;
    const songIds = entries.flatMap((e) => (e.songId !== null ? [e.songId] : []));
    const filePaths = entries.flatMap((e) => (e.songId === null && e.location !== null ? [e.location] : []));
    editingBatch = { songIds, filePaths, folderPath: location.path };
  }

  // Statuses change when the library does; the source list is re-read
  // after every library change and scan, so follow it.
  let seenSources: unknown = null;
  $effect(() => {
    const sources = filesStore.sources;
    if (seenSources !== null && sources !== seenSources) filesStore.reloadFolder();
    seenSources = sources;
  });

  const isLocal = $derived(location?.source.kind === "folder");
  /** Folders and WebDAV servers have a real path or URL to show. The header lays out from
   * this, known at once, so it doesn't jump when a slow folder's listing arrives. */
  const hasLocation = $derived(isLocal || location?.source.kind === "webdav");
  /** The `i`th name of the current path as shown; the listing says which are missing tags. */
  const pathName = (i: number) => shownName(location?.path[i] ?? "", listing?.pathMissingTags[i]);
  const title = $derived(location && location.path.length > 0 ? pathName(location.path.length - 1) : sourceName);
  const unreachableLabel = $derived(i18n.t("files.unreachable", { name: sourceName }));

  $effect(() => {
    if (!openDevice && location) {
      filesStore.activeFolderTitle = title;
    }
  });

  function navigate(path: string[]) {
    if (!location) return;
    navigationStore.filesLocation = { source: location.source, path };
  }

  /** Enter or double-click on a file: a sidecar opens in its app, a song plays its folder from it. */
  function activate(entry: FileEntry) {
    if (!location) return;
    if (entry.opensInApp) {
      openFile(location.source, location.path, entry);
      return;
    }
    if (entry.songId === null) {
      toastStore.show(
        i18n.t(isLocal && entry.status === "notScanned" ? "files.notInLibraryScan" : "files.notInLibrary"),
        "info"
      );
      return;
    }
    playFolder(location.source, location.path, title, entry.songId);
  }

  /** The files selected, when there's more than one; the footer counts them. */
  const selectedFiles = $derived(
    selection.length > 1 && listing ? listing.entries.filter((e) => !e.isDir && selection.includes(e.name)) : []
  );

  /** How many files in this folder are waiting to be scanned into the library. */
  const unscannedCount = $derived(
    listing ? listing.entries.filter((e) => !e.isDir && e.status === "notScanned").length : 0
  );

  /** The footer's lines: the selection, or the folder's size, then what is in the library. */
  function summary(l: DirListing): string[] {
    if (selectedFiles.length > 1) {
      const vars = { selected: selectedFiles.length };
      return [
        l.totalBytes === null
          ? i18n.plural("files.selectedFilesNoSize", l.fileCount, vars)
          : i18n.plural("files.selectedFiles", l.fileCount, {
              ...vars,
              size: formatFileSize(selectedFiles.reduce((sum, e) => sum + (e.size ?? 0), 0)),
            }),
      ];
    }
    if (filesStore.isFilterActive) {
      return [
        i18n.plural("files.filterMatchSummary", l.entries.length, {
          matched: filesStore.filterMatchCount,
        }),
      ];
    }
    const files =
      l.totalBytes === null
        ? i18n.plural("files.summaryFilesNoSize", l.fileCount)
        : i18n.plural("files.summaryFiles", l.fileCount, { size: formatFileSize(l.totalBytes) });
    const hidden = l.hiddenCount > 0 ? ` ${i18n.plural("files.hiddenFiles", l.hiddenCount)}` : "";
    return [files, `${i18n.plural("files.summaryInLibrary", l.inLibraryCount)}${hidden}`];
  }
</script>

<div class="flex-1 flex flex-col h-full min-h-0 bg-brand-main text-brand-text-primary select-none overflow-hidden" data-testid="files-view">
  {#if openDevice}
    {#key openDevice.id}
      <DeviceView device={openDevice} />
    {/key}
  {:else if location}
    <div class="flex-1 min-h-0 flex flex-col px-6 pt-4 gap-4" class:pb-28={!!playerStore.currentSong} class:pb-6={!playerStore.currentSong}>
      <header class="flex items-end justify-between gap-4 flex-wrap">
        <div class="min-w-0">
          <nav class="flex items-center gap-1.5 text-sm text-brand-text-secondary min-w-0" aria-label={i18n.t("files.breadcrumb")}>
            <button class="hover:text-brand-text-primary truncate" onclick={() => navigationStore.viewFiles()}>{i18n.t("sidebar.files")}</button>
            <span aria-hidden="true">/</span>
            <button
              class="truncate {location.path.length === 0 ? 'text-brand-text-primary font-semibold' : 'hover:text-brand-text-primary'}"
              onclick={() => navigate([])}
            >{sourceName}</button>
            {#each location.path as _, i (i)}
              <span aria-hidden="true">/</span>
              <button
                class="truncate {i === location.path.length - 1 ? 'text-brand-text-primary font-semibold' : 'hover:text-brand-text-primary'}"
                onclick={() => navigate(location.path.slice(0, i + 1))}
              >{pathName(i)}</button>
            {/each}
          </nav>
          <h1 class="text-3xl font-heading font-bold text-brand-text-primary leading-snug truncate py-0.5" title={title}>{title}</h1>
          {#if hasLocation}
            <p class="text-xs text-brand-text-secondary truncate select-text" title={listing?.location}>{listing?.location ?? " "}</p>
          {/if}
        </div>
        <div class="flex items-center gap-2 shrink-0">
          <Button variant="primary" onclick={() => playFolder(location.source, location.path, title)}>
            <PlayIcon weight="fill" class="w-4 h-4" />
            {i18n.t("files.playFolder")}
          </Button>
          <button
            bind:this={headerMenuButtonEl}
            class="w-9 h-9 flex items-center justify-center rounded-full text-brand-text-secondary hover:text-brand-text-primary hover:bg-brand-accent/10 transition-colors cursor-pointer"
            onclick={(e) => {
              e.stopPropagation();
              toggleHeaderMenu();
            }}
            title={i18n.t("files.folderActions")}
            aria-label={i18n.t("files.folderActions")}
          >
            <DotsThreeIcon weight="bold" class="w-5 h-5" />
          </button>
        </div>
      </header>

      <FileBrowser
        source={location.source}
        path={location.path}
        onnavigate={navigate}
        onactivate={activate}
        target={location.selectedName ?? null}
        bind:arrival={navigationStore.pendingFilesArrival}
        reloadToken={filesStore.reloadToken}
        {unreachableLabel}
        bind:listing
        bind:selectedName
        bind:selection
        bind:filterQuery={filesStore.filterQuery}
        bind:matchedCount={filesStore.filterMatchCount}
        bind:totalCount={filesStore.filterTotalCount}
        oncontextmenu={(target, x, y) => (menu = { x, y, ...target })}
      >
        {#snippet footer()}
          {#if listing}
            <div class="flex items-center px-4 py-2.5 text-xs text-brand-text-secondary">
              <div class="min-w-0 flex flex-col">
                {#each summary(listing) as line, i (i)}
                  <span class="truncate" title={line}>
                    {line}
                    {#if i === 1 && collectionStore.isScanning}
                      <span class="text-brand-accent font-medium ml-1.5">· {i18n.t("files.scanning")}</span>
                    {:else if i === 1 && isLocal && !listing.excluded && unscannedCount > 0}
                      <span class="ml-1.5">
                        · {i18n.plural("files.summaryUnscanned", unscannedCount)}
                        <button
                          class="text-brand-accent hover:underline font-medium ml-1 cursor-pointer"
                          onclick={() => scanFolder(location.source, location.path)}
                        >{i18n.t("files.scanNudge")}</button>
                      </span>
                    {/if}
                  </span>
                {/each}
                {#if summary(listing).length === 1 && collectionStore.isScanning}
                  <span class="text-brand-accent font-medium">{i18n.t("files.scanning")}</span>
                {/if}
              </div>
            </div>
          {/if}
        {/snippet}
      </FileBrowser>
    </div>
    {#if menu}
      <FileContextMenu
        x={menu.x}
        y={menu.y}
        source={location.source}
        path={menu.dir}
        folderName={menu.inColumn ? "" : title}
        entries={menu.entries}
        inColumn={menu.inColumn}
        fromHeader={menu.fromHeader}
        folderLocation={menu.fromHeader ? listing?.location : null}
        scannable={isLocal && !listing?.excluded}
        onFolderSettings={() => navigationStore.openSettings("sources")}
        onEditTags={(id) => (editingSongId = id)}
        onEditFileTags={(entry) => {
          if (entry.location) {
            editingFile = {
              path: entry.location,
              name: entry.name,
              folderPath: location.path,
              folderName: title,
            };
          }
        }}
        onEditFolderTags={editFolderTags}
        onEditBatchTags={editBatchTags}
        onClose={() => (menu = null)}
      />
    {/if}
    {#if editingSongId !== null}
      <TagEditor songId={editingSongId} onClose={() => (editingSongId = null)} onSave={() => filesStore.reloadFolder()} />
    {/if}
    {#if editingFile !== null}
      <TagEditor
        filePath={editingFile.path}
        fileName={editingFile.name}
        folderPath={editingFile.folderPath}
        folderName={editingFile.folderName}
        source={location.source}
        onClose={() => (editingFile = null)}
        onSave={() => filesStore.reloadFolder()}
      />
    {/if}
    {#if editingFolder !== null}
      <AlbumTagEditor
        title={i18n.t("files.editTagsForFolder")}
        songIds={editingFolder.songIds}
        filePaths={editingFolder.filePaths}
        folderName={editingFolder.folderName}
        folderPath={editingFolder.folderPath}
        source={location.source}
        onClose={() => (editingFolder = null)}
        onSave={() => filesStore.reloadFolder()}
      />
    {/if}
    {#if editingBatch !== null}
      <AlbumTagEditor
        title={i18n.t("files.editTags")}
        songIds={editingBatch.songIds}
        filePaths={editingBatch.filePaths}
        folderPath={editingBatch.folderPath}
        source={location.source}
        onClose={() => (editingBatch = null)}
        onSave={() => filesStore.reloadFolder()}
      />
    {/if}
  {:else}
    <div class="flex-1 overflow-y-auto px-6 pt-4" class:pb-28={!!playerStore.currentSong} class:pb-12={!playerStore.currentSong}>
      <header class="flex items-end justify-between gap-4 mb-4">
        <h1 class="text-3xl font-heading font-bold text-brand-text-primary leading-snug">{i18n.t("sidebar.files")}</h1>
        <Button onclick={() => navigationStore.openSettings("sources")}>
          <PlusIcon class="w-4 h-4" />
          {i18n.t("files.addSource")}
        </Button>
      </header>
      {#if filesStore.loaded && filesStore.sources.length === 0}
        <p class="text-sm text-brand-text-secondary">{i18n.t("files.noSources")}</p>
      {:else}
        <ul class="flex flex-col gap-1 max-w-3xl">
          {#each filesStore.sources as entry (sourceKey(entry.source))}
            {@const Icon = sourceIcon(entry.source)}
            <li>
              <button
                class="w-full flex items-center gap-3 px-4 py-3 rounded-xl border border-brand-border/40 hover:bg-brand-accent/10 transition-colors text-left {entry.available ? '' : 'opacity-60'}"
                onclick={() => navigationStore.viewFiles(entry.source)}
              >
                <Icon class="w-5 h-5 shrink-0 text-brand-text-secondary" />
                <span class="flex-1 min-w-0">
                  <span class="block text-sm font-semibold truncate">{entry.name}</span>
                  <span class="block text-xs text-brand-text-secondary">
                    {i18n.t(`files.kind.${entry.source.kind}`)} · {i18n.plural("files.songCount", entry.songCount)}
                  </span>
                </span>
                <CaretRightIcon class="w-4 h-4 text-brand-text-secondary" />
              </button>
            </li>
          {/each}
        </ul>
      {/if}
    </div>
  {/if}
</div>
