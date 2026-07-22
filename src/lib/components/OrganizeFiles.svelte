<script lang="ts">
  import { onMount } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { open } from "@tauri-apps/plugin-dialog";
  import { i18n } from "../stores/i18n.svelte";
  import { portal } from "../utils/portal";
  import { X, Folder, Sparkles, Check, AlertTriangle, RefreshCw, Layers } from "lucide-svelte";
  import { VirtualList } from "svelte-virtual-list-ts";

  export interface OrganizePreviewItem {
    song_id: number;
    from_path: string;
    to_path: string;
    status: "ok" | "unchanged" | "collision" | "missing_tag" | "cross_device" | "error";
    error_message: string | null;
  }

  let {
    isOpen = false,
    songIds = [],
    initialScope = "selection",
    onClose,
    onSuccess,
  }: {
    isOpen: boolean;
    songIds?: number[];
    initialScope?: "selection" | "library";
    onClose: () => void;
    onSuccess?: () => void;
  } = $props();

  const DEFAULT_TEMPLATE = "%albumartist/{%album/}{%disc-}{%track }%title";
  const VARIABLE_CHIPS = [
    { label: "%albumartist", desc: "Album Artist" },
    { label: "%artist", desc: "Artist" },
    { label: "%album", desc: "Album" },
    { label: "{%album/}", desc: "Optional Album Folder" },
    { label: "/", desc: "Folder Separator" },
    { label: "{%disc-}", desc: "Conditional Disc Prefix" },
    { label: "%track", desc: "Track #" },
    { label: "{%track }", desc: "Optional Track #" },
    { label: "%title", desc: "Title" },
    { label: "%year", desc: "Year" },
    { label: "%genre", desc: "Genre" },
  ];

  function highlightTemplateHtml(str: string): string {
    if (!str) return "";
    let escaped = str
      .replace(/&/g, "&amp;")
      .replace(/</g, "&lt;")
      .replace(/>/g, "&gt;");

    const regex = /({[^{}]*})|(%[a-z]+)|([/\\])|(\s)/gi;
    return escaped.replace(regex, (match, pBlock, pVar, pSep, pSpace) => {
      if (pBlock) {
        const inner = pBlock.replace(/%[a-z]+/gi, (v: string) => `<span class="text-cyan-300 font-bold bg-cyan-500/30 rounded-xs">${v}</span>`);
        return `<span class="text-purple-300 font-bold bg-purple-500/25 rounded-xs">${inner}</span>`;
      }
      if (pVar) {
        return `<span class="text-cyan-300 font-bold bg-cyan-500/25 rounded-xs">${pVar}</span>`;
      }
      if (pSep) {
        return `<span class="text-amber-400 font-bold bg-amber-500/30 rounded-xs">${pSep}</span>`;
      }
      if (pSpace) {
        return `<span class="bg-white/15 rounded-xs select-none" title="Space">&nbsp;</span>`;
      }
      return match;
    });
  }

  function highlightPathHtml(path: string): string {
    if (!path) return "";
    const escaped = path
      .replace(/&/g, "&amp;")
      .replace(/</g, "&lt;")
      .replace(/>/g, "&gt;");

    return escaped.replace(/([/\\])/g, '<span class="text-amber-400 font-bold px-0.5 bg-amber-500/15 rounded">$1</span>');
  }

  let template = $state(DEFAULT_TEMPLATE);
  let scope = $state<"selection" | "library">("library");
  let replaceSpaces = $state(false);
  let asciiOnly = $state(false);
  let cleanEmptyDirs = $state(true);
  let moveExtraFiles = $state(true);
  let destinationDir = $state<string>("");

  $effect(() => {
    if (isOpen) {
      scope = songIds.length > 0 ? initialScope : "library";
    }
  });

  let items = $state<OrganizePreviewItem[]>([]);
  let isLoading = $state(false);
  let isApplying = $state(false);
  let errorMessage = $state<string | null>(null);
  let successMessage = $state<string | null>(null);

  let activeSongIds = $derived(scope === "library" ? [] : songIds);

  function getItemObj(raw: any): OrganizePreviewItem {
    if (raw && typeof raw === "object" && "item" in raw && raw.item) {
      return raw.item as OrganizePreviewItem;
    }
    return raw as OrganizePreviewItem;
  }

  function getItemStatus(item: OrganizePreviewItem): "ok" | "unchanged" | "collision" | "missing_tag" | "error" {
    const target = getItemObj(item);
    const s = String(target?.status || "").toLowerCase();
    if (s === "ok") return "ok";
    if (s === "unchanged") return "unchanged";
    if (s === "collision") return "collision";
    if (s === "missing_tag" || s === "missingtag") return "missing_tag";
    return "error";
  }

  let showOnlyChanging = $state(false);

  let displayedItems = $derived(
    showOnlyChanging
      ? items.filter((i) => getItemStatus(i) !== "unchanged")
      : items
  );

  let commonPrefix = $derived.by(() => {
    const allPaths: string[] = [];
    for (const raw of displayedItems) {
      const i = getItemObj(raw);
      if (i.from_path) allPaths.push(i.from_path);
      if (i.to_path) allPaths.push(i.to_path);
    }
    if (allPaths.length < 2) return "";
    let prefix = allPaths[0];
    const lastSep = Math.max(prefix.lastIndexOf("/"), prefix.lastIndexOf("\\"));
    if (lastSep > 0) prefix = prefix.slice(0, lastSep + 1);

    for (const p of allPaths) {
      while (prefix.length > 0 && !p.startsWith(prefix)) {
        const sep = Math.max(prefix.slice(0, -1).lastIndexOf("/"), prefix.slice(0, -1).lastIndexOf("\\"));
        if (sep >= 0) {
          prefix = prefix.slice(0, sep + 1);
        } else {
          prefix = "";
          break;
        }
      }
    }
    return prefix;
  });

  function getDisplayPath(fullPath: string, prefix: string): string {
    if (!fullPath) return "";
    if (prefix && prefix.length > 3 && fullPath.startsWith(prefix)) {
      return "…" + fullPath.slice(prefix.length - 1);
    }
    return fullPath;
  }

  let fromColWidth = $state(340);
  let toColWidth = $state(380);
  let isResizing = $state<"from" | "to" | null>(null);
  let resizeStartX = 0;
  let resizeStartWidth = 0;

  function startResize(col: "from" | "to", e: MouseEvent) {
    e.preventDefault();
    isResizing = col;
    resizeStartX = e.clientX;
    resizeStartWidth = col === "from" ? fromColWidth : toColWidth;
    window.addEventListener("mousemove", handleResizeMove);
    window.addEventListener("mouseup", handleResizeUp);
  }

  function handleResizeMove(e: MouseEvent) {
    if (!isResizing) return;
    const diff = e.clientX - resizeStartX;
    if (isResizing === "from") {
      fromColWidth = Math.max(150, resizeStartWidth + diff);
    } else {
      toColWidth = Math.max(150, resizeStartWidth + diff);
    }
  }

  function handleResizeUp() {
    isResizing = null;
    window.removeEventListener("mousemove", handleResizeMove);
    window.removeEventListener("mouseup", handleResizeUp);
  }

  function autoFitColumns() {
    let maxFrom = 250;
    let maxTo = 300;
    for (const raw of items) {
      const i = getItemObj(raw);
      if (i.from_path) maxFrom = Math.max(maxFrom, i.from_path.length * 7.5);
      if (i.to_path) maxTo = Math.max(maxTo, i.to_path.length * 7.5);
    }
    fromColWidth = Math.min(1000, Math.max(250, maxFrom));
    toColWidth = Math.min(1000, Math.max(300, maxTo));
  }

  let collisionCount = $derived(items.filter((i) => getItemStatus(i) === "collision").length);
  let errorCount = $derived(items.filter((i) => getItemStatus(i) === "error").length);
  let missingTagCount = $derived(items.filter((i) => getItemStatus(i) === "missing_tag").length);
  let readyCount = $derived(items.filter((i) => getItemStatus(i) === "ok" || getItemStatus(i) === "missing_tag").length);
  let unchangedCount = $derived(items.filter((i) => getItemStatus(i) === "unchanged").length);
  let canApply = $derived(readyCount > 0 && collisionCount === 0 && !isLoading && !isApplying);

  let debounceTimer: ReturnType<typeof setTimeout> | null = null;

  function handleKeydown(e: KeyboardEvent) {
    if (e.key === "Escape" && isOpen) {
      onClose();
    }
  }

  function insertChip(chipText: string) {
    template = template + chipText;
  }

  async function selectDestinationDir() {
    try {
      const selected = await open({
        directory: true,
        multiple: false,
        title: i18n.t("organizer.destinationDirLabel"),
      });
      if (selected && typeof selected === "string") {
        destinationDir = selected;
      }
    } catch (e) {
      console.error("Failed to select destination folder:", e);
    }
  }

  async function fetchPreview() {
    if (!isOpen) return;
    isLoading = true;
    errorMessage = null;

    try {
      const res = await invoke<OrganizePreviewItem[]>("preview_organize", {
        songIds: activeSongIds,
        template,
        options: {
          destination_dir: destinationDir.trim() !== "" ? destinationDir : null,
          replace_spaces_with_underscores: replaceSpaces,
          ascii_only: asciiOnly,
          clean_empty_dirs: cleanEmptyDirs,
          move_extra_files: moveExtraFiles,
        },
      });
      items = res;
    } catch (err: any) {
      console.error("Preview failed:", err);
      errorMessage = typeof err === "string" ? err : err.message || "Failed to generate preview";
      items = [];
    } finally {
      isLoading = false;
    }
  }

  $effect(() => {
    // Read reactive variables synchronously so Svelte 5 tracks dependencies
    const _t = template;
    const _d = destinationDir;
    const _r = replaceSpaces;
    const _a = asciiOnly;
    const _c = cleanEmptyDirs;
    const _m = moveExtraFiles;
    const _s = scope;
    const _ids = activeSongIds;
    const _open = isOpen;

    if (_open) {
      if (debounceTimer) clearTimeout(debounceTimer);
      debounceTimer = setTimeout(() => {
        fetchPreview();
      }, 300);
    }
    return () => {
      if (debounceTimer) clearTimeout(debounceTimer);
    };
  });

  async function handleApply() {
    if (!canApply) return;
    isApplying = true;
    errorMessage = null;

    const itemsToApply = items
      .filter((i) => i.status === "ok" || i.status === "missing_tag")
      .map((i) => ({
        song_id: i.song_id,
        from_path: i.from_path,
        to_path: i.to_path,
      }));

    try {
      const result = await invoke<{ moved_count: number; skipped_count: number; errors: string[] }>(
        "apply_organize",
        {
          items: itemsToApply,
          cleanEmptyDirs: cleanEmptyDirs,
          moveExtraFiles: moveExtraFiles,
        }
      );

      if (result.errors && result.errors.length > 0) {
        errorMessage = result.errors.join("; ");
      } else {
        successMessage = i18n.t("organizer.applySuccess", { count: result.moved_count });
        setTimeout(() => {
          onSuccess?.();
          onClose();
        }, 1200);
      }
    } catch (err: any) {
      console.error("Failed to apply organize:", err);
      errorMessage = typeof err === "string" ? err : err.message || "Failed to organize files";
    } finally {
      isApplying = false;
    }
  }

  onMount(() => {
    window.addEventListener("keydown", handleKeydown);
    return () => {
      window.removeEventListener("keydown", handleKeydown);
    };
  });
</script>

{#if isOpen}
  <div
    use:portal
    class="fixed inset-0 z-50 flex items-center justify-center p-4 bg-black/60 backdrop-blur-md animate-in fade-in duration-200"
    role="dialog"
    aria-modal="true"
  >
    <div
      class="w-full max-w-4xl max-h-[90vh] bg-brand-sidebar border border-brand-border/80 rounded-2xl shadow-2xl flex flex-col overflow-hidden animate-in zoom-in-95 duration-200 text-brand-text-primary"
    >
      <!-- Header -->
      <div class="px-6 py-4 border-b border-brand-border/40 flex items-center justify-between shrink-0">
        <div class="flex items-center gap-3">
          <div class="p-2 rounded-xl bg-brand-accent/15 text-brand-accent-text">
            <Folder class="w-5 h-5" />
          </div>
          <div>
            <h2 class="text-base font-bold text-brand-text-primary">
              {i18n.t("organizer.title")}
            </h2>
            <p class="text-xs text-brand-text-secondary">
              {i18n.t("organizer.subtitle")}
            </p>
          </div>
        </div>

        <button
          onclick={onClose}
          class="p-1.5 rounded-lg text-brand-text-secondary hover:text-brand-text-primary hover:bg-brand-sidebar/80 transition-colors cursor-pointer"
          title="Close"
        >
          <X class="w-4 h-4" />
        </button>
      </div>

      <!-- Content area -->
      <div class="p-6 space-y-4 overflow-y-auto flex-1 text-xs">
        {#if songIds.length > 0}
          <!-- Scope selector -->
          <div class="flex items-center justify-between bg-brand-sidebar/40 p-3 rounded-xl border border-brand-border/30">
            <span class="font-semibold text-brand-text-primary">{i18n.t("organizer.scopeLabel")}</span>
            <div class="flex items-center gap-1.5 bg-brand-sidebar p-1 rounded-lg border border-brand-border/50">
              <button
                onclick={() => { scope = "selection"; }}
                class="px-3 py-1 rounded-md transition-colors cursor-pointer {scope === 'selection' ? 'bg-brand-accent text-brand-accent-contrast font-bold shadow-sm' : 'text-brand-text-secondary hover:text-brand-text-primary'}"
              >
                {i18n.t("organizer.scopeSelection", { count: songIds.length })}
              </button>
              <button
                onclick={() => { scope = "library"; }}
                class="px-3 py-1 rounded-md transition-colors cursor-pointer {scope === 'library' ? 'bg-brand-accent text-brand-accent-contrast font-bold shadow-sm' : 'text-brand-text-secondary hover:text-brand-text-primary'}"
              >
                {i18n.t("organizer.scopeLibrary", { count: items.length })}
              </button>
            </div>
          </div>
        {/if}

        <!-- Template Pattern Input -->
        <div>
          <div class="flex items-center justify-between mb-1.5">
            <label for="template-input" class="font-semibold text-brand-text-primary">
              {i18n.t("organizer.templateLabel")}
            </label>
            <button
              onclick={() => { template = DEFAULT_TEMPLATE; }}
              class="text-[11px] text-brand-accent-text hover:underline cursor-pointer"
            >
              {i18n.t("organizer.defaultPattern")}
            </button>
          </div>

          <!-- Highlighted input box -->
          <div class="relative w-full rounded-xl bg-brand-sidebar/90 border border-brand-border/90 focus-within:border-brand-accent transition-colors font-mono text-xs overflow-hidden min-h-[40px] flex items-center shadow-inner">
            <div class="absolute inset-0 px-3.5 py-2.5 text-brand-text-primary whitespace-pre overflow-hidden flex items-center pointer-events-none font-mono text-xs leading-normal tracking-normal select-none z-0">
              {@html highlightTemplateHtml(template)}
            </div>
            <input
              id="template-input"
              type="text"
              bind:value={template}
              class="relative w-full px-3.5 py-2.5 bg-transparent text-transparent caret-brand-accent font-mono text-xs leading-normal tracking-normal focus:outline-none z-10"
              spellcheck="false"
            />
          </div>

          <!-- Variable chips -->
          <div class="flex flex-wrap items-center gap-1.5 pt-1">
            <span class="text-[11px] text-brand-text-secondary mr-1">{i18n.t("organizer.placeholders")}:</span>
            {#each VARIABLE_CHIPS as chip}
              <button
                type="button"
                onclick={() => insertChip(chip.label)}
                class="px-2 py-0.5 rounded-lg text-[11px] font-mono transition-colors cursor-pointer border {chip.label === '/' ? 'bg-amber-500/20 border-amber-500/40 text-amber-300 hover:bg-amber-500/30 font-bold' : chip.label.startsWith('{') ? 'bg-purple-500/15 border-purple-500/40 text-purple-300 hover:bg-purple-500/30' : 'bg-cyan-500/15 border-cyan-500/40 text-cyan-300 hover:bg-cyan-500/30'}"
                title={chip.desc}
              >
                {chip.label === '/' ? '/ (Folder)' : chip.label}
              </button>
            {/each}
          </div>
        </div>

        <!-- Options row -->
        <div class="grid grid-cols-1 md:grid-cols-2 gap-3 pt-2">
          <!-- Destination folder -->
          <div class="md:col-span-2">
            <label for="dest-dir-input" class="block font-semibold text-brand-text-primary mb-1.5">
              {i18n.t("organizer.destinationDirLabel")}
            </label>
            <div class="flex items-center gap-2">
              <input
                id="dest-dir-input"
                type="text"
                bind:value={destinationDir}
                placeholder={i18n.t("organizer.destinationDefault")}
                class="flex-1 px-3 py-1.5 bg-brand-sidebar/80 border border-brand-border/80 rounded-xl text-brand-text-primary text-xs focus:outline-none focus:border-brand-accent transition-colors truncate"
              />
              <button
                onclick={selectDestinationDir}
                class="px-3 py-1.5 bg-brand-sidebar border border-brand-border/80 hover:bg-brand-accent/15 hover:text-brand-accent-text text-brand-text-primary rounded-xl transition-colors cursor-pointer font-medium"
              >
                {i18n.t("organizer.browse")}
              </button>
            </div>
          </div>

          <label class="flex items-center gap-2 text-brand-text-secondary hover:text-brand-text-primary cursor-pointer">
            <input
              type="checkbox"
              bind:checked={replaceSpaces}
              class="w-3.5 h-3.5 rounded border-brand-border accent-brand-accent cursor-pointer"
            />
            <span>{i18n.t("organizer.replaceSpaces")}</span>
          </label>

          <label class="flex items-center gap-2 text-brand-text-secondary hover:text-brand-text-primary cursor-pointer">
            <input
              type="checkbox"
              bind:checked={asciiOnly}
              class="w-3.5 h-3.5 rounded border-brand-border accent-brand-accent cursor-pointer"
            />
            <span>{i18n.t("organizer.asciiOnly")}</span>
          </label>

          <label class="flex items-center gap-2 text-brand-text-secondary hover:text-brand-text-primary cursor-pointer">
            <input
              type="checkbox"
              bind:checked={moveExtraFiles}
              class="w-3.5 h-3.5 rounded border-brand-border accent-brand-accent cursor-pointer"
            />
            <span>{i18n.t("organizer.moveExtraFiles")}</span>
          </label>
        </div>

        <!-- Preview Table Section -->
        <div class="space-y-2 pt-2">
          <div class="flex items-center justify-between">
            <h3 class="font-bold text-brand-text-primary flex items-center gap-2">
              <span>{i18n.t("organizer.previewTitle", { count: items.length })}</span>
              <button
                type="button"
                onclick={fetchPreview}
                class="p-1 rounded-md text-brand-text-secondary hover:text-brand-accent-text hover:bg-brand-sidebar cursor-pointer transition-colors"
                title="Refresh Preview"
              >
                <RefreshCw class="w-3.5 h-3.5 {isLoading ? 'animate-spin text-brand-accent-text' : ''}" />
              </button>
            </h3>

            <!-- Summary badges & filter toggle -->
            <div class="flex items-center gap-2 text-[11px]">
              <label class="flex items-center gap-1.5 px-2 py-0.5 rounded-full bg-brand-sidebar border border-brand-border/60 text-brand-text-secondary hover:text-brand-text-primary cursor-pointer select-none">
                <input
                  type="checkbox"
                  bind:checked={showOnlyChanging}
                  class="w-3 h-3 rounded border-brand-border accent-brand-accent cursor-pointer"
                />
                <span>{i18n.t("organizer.onlyChanging")}</span>
              </label>

              <span class="px-2 py-0.5 rounded-full bg-emerald-500/15 text-emerald-400 font-semibold border border-emerald-500/30">
                {i18n.t("organizer.summaryReady", { count: readyCount })}
              </span>
              <span class="px-2 py-0.5 rounded-full bg-brand-sidebar border border-brand-border/60 text-brand-text-secondary">
                {i18n.t("organizer.summaryUnchanged", { count: unchangedCount })}
              </span>
              {#if missingTagCount > 0}
                <span class="px-2 py-0.5 rounded-full bg-amber-500/20 text-amber-400 font-semibold border border-amber-500/40">
                  {i18n.t("organizer.summaryMissingTags", { count: missingTagCount })}
                </span>
              {/if}
              {#if collisionCount > 0}
                <span class="px-2 py-0.5 rounded-full bg-rose-500/20 text-rose-400 font-semibold border border-rose-500/40">
                  {i18n.t("organizer.summaryCollisions", { count: collisionCount })}
                </span>
              {/if}
              {#if errorCount > 0}
                <span class="px-2 py-0.5 rounded-full bg-rose-500/20 text-rose-400 font-semibold border border-rose-500/40">
                  {i18n.t("organizer.summaryErrors", { count: errorCount })}
                </span>
              {/if}
            </div>
          </div>

          {#if errorMessage}
            <div class="p-3 rounded-xl bg-rose-500/15 border border-rose-500/30 text-rose-400 flex items-center gap-2">
              <AlertTriangle class="w-4 h-4 shrink-0" />
              <span>{errorMessage}</span>
            </div>
          {/if}

          {#if successMessage}
            <div class="p-3 rounded-xl bg-emerald-500/15 border border-emerald-500/30 text-emerald-400 flex items-center gap-2">
              <Check class="w-4 h-4 shrink-0" />
              <span>{successMessage}</span>
            </div>
          {/if}

          <!-- Virtualized table -->
          <div class="h-64 border border-brand-border/60 rounded-xl overflow-hidden bg-brand-sidebar/40 flex flex-col">
            {#if items.length === 0}
              <div class="h-full flex items-center justify-center text-brand-text-secondary">
                {#if isLoading}
                  <RefreshCw class="w-5 h-5 text-brand-accent-text animate-spin" />
                {:else}
                  <span>No tracks to organize.</span>
                {/if}
              </div>
            {:else if displayedItems.length === 0}
              <div class="h-full flex items-center justify-center text-brand-text-secondary">
                <span>No changing files match filter.</span>
              </div>
            {:else}
              <!-- Common Base Path Bar -->
              {#if commonPrefix}
                <div class="px-3 py-1 bg-brand-sidebar/80 border-b border-brand-border/40 text-[10px] text-brand-text-secondary font-mono flex items-center gap-1.5 shrink-0" title={commonPrefix}>
                  <span class="font-semibold text-brand-accent-text shrink-0">{i18n.t("organizer.commonBasePath")}</span>
                  <span class="truncate text-brand-text-primary">{commonPrefix}</span>
                </div>
              {/if}

              <!-- Horizontally scrollable viewport wrapper -->
              <div class="flex-1 min-h-0 overflow-x-auto overflow-y-hidden">
                <div style="min-width: {96 + fromColWidth + 24 + toColWidth + 20}px;" class="h-full flex flex-col">
                  <!-- Table Column Header with Draggable Splitters -->
                  <div class="h-7 px-3 flex items-center bg-brand-sidebar/95 border-b border-brand-border/60 text-[10px] font-semibold text-brand-text-secondary uppercase tracking-wider select-none shrink-0 font-mono">
                    <div class="w-24 shrink-0">Status</div>

                    <!-- Original Path Header -->
                    <div class="flex items-center shrink-0 pr-1" style="width: {fromColWidth}px;">
                      <span class="truncate flex-1">Original Path</span>
                      <button
                        type="button"
                        aria-label="Resize Original Path Column"
                        onmousedown={(e) => startResize("from", e)}
                        ondblclick={autoFitColumns}
                        class="w-3 h-5 hover:bg-brand-accent/30 cursor-col-resize flex items-center justify-center group shrink-0 bg-transparent border-0 p-0"
                        title="Drag to resize column, double-click to auto-fit"
                      >
                        <div class="w-0.5 h-3 bg-brand-border group-hover:bg-brand-accent"></div>
                      </button>
                    </div>

                    <div class="w-6 text-center shrink-0 text-brand-text-secondary">→</div>

                    <!-- Target Path Header -->
                    <div class="flex items-center shrink-0 pl-1" style="width: {toColWidth}px;">
                      <span class="truncate flex-1">Target Path</span>
                      <button
                        type="button"
                        aria-label="Resize Target Path Column"
                        onmousedown={(e) => startResize("to", e)}
                        ondblclick={autoFitColumns}
                        class="w-3 h-5 hover:bg-brand-accent/30 cursor-col-resize flex items-center justify-center group shrink-0 bg-transparent border-0 p-0"
                        title="Drag to resize column, double-click to auto-fit"
                      >
                        <div class="w-0.5 h-3 bg-brand-border group-hover:bg-brand-accent"></div>
                      </button>
                    </div>
                  </div>

                  <!-- Virtualized rows -->
                  <div class="flex-1 min-h-0">
                    <VirtualList items={displayedItems} height="100%" itemHeight={36}>
                      {#snippet children(rawRow: any)}
                        {@const item = getItemObj(rawRow)}
                        {@const st = getItemStatus(item)}
                        {@const displayFrom = getDisplayPath(item.from_path, commonPrefix)}
                        {@const displayTo = getDisplayPath(item.to_path, commonPrefix)}
                        <div
                          class="h-9 px-3 flex items-center border-b border-brand-border/20 text-[11px] font-mono hover:bg-brand-accent/10 transition-colors whitespace-nowrap"
                        >
                          <!-- Status badge -->
                          <div class="w-24 shrink-0">
                            {#if st === "ok"}
                              <span class="px-2 py-0.5 rounded bg-emerald-500/15 text-emerald-400 border border-emerald-500/30">
                                {i18n.t("organizer.statusOk")}
                              </span>
                            {:else if st === "unchanged"}
                              <span class="px-2 py-0.5 rounded bg-brand-sidebar border border-brand-border/60 text-brand-text-secondary">
                                {i18n.t("organizer.statusUnchanged")}
                              </span>
                            {:else if st === "collision"}
                              <span class="px-2 py-0.5 rounded bg-rose-500/20 text-rose-400 border border-rose-500/40" title={item.error_message || i18n.t("organizer.statusCollision")}>
                                {i18n.t("organizer.statusCollision")}
                              </span>
                            {:else if st === "missing_tag"}
                              <span class="px-2 py-0.5 rounded bg-amber-500/20 text-amber-400 border border-amber-500/40" title={item.error_message || i18n.t("organizer.statusMissingTag")}>
                                {i18n.t("organizer.statusMissingTag")}
                              </span>
                            {:else}
                              <span class="px-2 py-0.5 rounded bg-rose-500/20 text-rose-400 border border-rose-500/40" title={item.error_message || i18n.t("organizer.statusError")}>
                                {i18n.t("organizer.statusError")}
                              </span>
                            {/if}
                          </div>

                          <!-- Original path cell -->
                          <div
                            class="px-2 overflow-x-auto scrollbar-none shrink-0 {item.from_path ? 'text-brand-text-secondary' : 'text-rose-400 font-medium'}"
                            style="width: {fromColWidth}px;"
                            title={item.from_path}
                          >
                            {@html highlightPathHtml(displayFrom || "(No path recorded)")}
                          </div>

                          <!-- Arrow separator -->
                          <div class="w-6 text-center text-brand-text-secondary shrink-0">→</div>

                          <!-- Target path cell -->
                          <div
                            class="px-2 overflow-x-auto scrollbar-none shrink-0 {st === 'ok' ? 'text-emerald-400 font-semibold' : st === 'collision' || st === 'error' ? 'text-rose-400 font-semibold' : 'text-brand-text-primary'}"
                            style="width: {toColWidth}px;"
                            title={item.error_message ? `${item.to_path ? item.to_path + ' — ' : ''}${item.error_message}` : item.to_path}
                          >
                            {#if st === 'error' || st === 'collision' || (item.error_message && st !== 'missing_tag')}
                              <span class="text-rose-400 font-medium">
                                {item.error_message ? item.error_message : (displayTo || 'Unknown error')}
                              </span>
                            {:else}
                              {@html highlightPathHtml(displayTo)}
                            {/if}
                          </div>
                        </div>
                      {/snippet}
                    </VirtualList>
                  </div>
                </div>
              </div>
            {/if}
          </div>
        </div>
      </div>

      <!-- Footer -->
      <div class="px-6 py-4 border-t border-brand-border/40 flex items-center justify-between shrink-0 bg-brand-sidebar/50">
        <div class="text-xs text-brand-text-secondary">
          {#if collisionCount > 0}
            <span class="text-rose-400 font-medium flex items-center gap-1.5">
              <AlertTriangle class="w-3.5 h-3.5" />
              Resolve collisions before applying.
            </span>
          {/if}
        </div>

        <div class="flex items-center gap-3">
          <button
            type="button"
            onclick={onClose}
            class="px-4 py-2 rounded-xl border border-brand-border/80 text-brand-text-primary hover:bg-brand-sidebar transition-colors cursor-pointer"
          >
            {i18n.t("organizer.cancel")}
          </button>

          <button
            type="button"
            onclick={handleApply}
            disabled={!canApply}
            class="px-5 py-2 rounded-xl bg-brand-accent text-brand-accent-contrast font-bold shadow-lg shadow-brand-accent/20 hover:brightness-110 disabled:opacity-40 disabled:cursor-not-allowed transition-all cursor-pointer flex items-center gap-2"
          >
            {#if isApplying}
              <RefreshCw class="w-4 h-4 animate-spin" />
              <span>Applying...</span>
            {:else}
              <Sparkles class="w-4 h-4" />
              <span>{i18n.t("organizer.applyButton")}</span>
            {/if}
          </button>
        </div>
      </div>
    </div>
  </div>
{/if}
