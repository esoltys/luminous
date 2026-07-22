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

  const DEFAULT_TEMPLATE = "%albumartist/%album/%disc-%track %title";
  const VARIABLE_CHIPS = [
    { label: "%albumartist", desc: "Album Artist" },
    { label: "%artist", desc: "Artist" },
    { label: "%album", desc: "Album" },
    { label: "%disc", desc: "Disc #" },
    { label: "%track", desc: "Track #" },
    { label: "%title", desc: "Title" },
    { label: "%year", desc: "Year" },
    { label: "%genre", desc: "Genre" },
  ];

  let template = $state(DEFAULT_TEMPLATE);
  let scope = $state<"selection" | "library">("library");
  let replaceSpaces = $state(false);
  let asciiOnly = $state(false);
  let cleanEmptyDirs = $state(true);
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

  function getItemStatus(item: OrganizePreviewItem): "ok" | "unchanged" | "collision" | "missing_tag" | "error" {
    const s = String(item?.status || "").toLowerCase();
    if (s === "ok") return "ok";
    if (s === "unchanged") return "unchanged";
    if (s === "collision") return "collision";
    if (s === "missing_tag" || s === "missingtag") return "missing_tag";
    return "error";
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
    if (isOpen) {
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
        <div class="space-y-2">
          <div class="flex items-center justify-between">
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
          <input
            id="template-input"
            type="text"
            bind:value={template}
            class="w-full px-3.5 py-2 bg-brand-sidebar/80 border border-brand-border/80 rounded-xl text-brand-text-primary text-xs font-mono focus:outline-none focus:border-brand-accent transition-colors"
          />

          <!-- Variable chips -->
          <div class="flex flex-wrap items-center gap-1.5 pt-1">
            <span class="text-[11px] text-brand-text-secondary mr-1">{i18n.t("organizer.placeholders")}:</span>
            {#each VARIABLE_CHIPS as chip}
              <button
                type="button"
                onclick={() => insertChip(chip.label)}
                class="px-2 py-0.5 rounded-lg bg-brand-sidebar border border-brand-border/60 text-[11px] font-mono text-brand-accent-text hover:bg-brand-accent/20 transition-colors cursor-pointer"
                title={chip.desc}
              >
                {chip.label}
              </button>
            {/each}
          </div>
        </div>

        <!-- Options row -->
        <div class="grid grid-cols-1 md:grid-cols-2 gap-3 pt-2">
          <!-- Destination folder -->
          <div class="space-y-1.5 md:col-span-2">
            <label for="dest-dir-input" class="font-semibold text-brand-text-primary">
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
              bind:checked={cleanEmptyDirs}
              class="w-3.5 h-3.5 rounded border-brand-border accent-brand-accent cursor-pointer"
            />
            <span>{i18n.t("organizer.cleanEmptyDirs")}</span>
          </label>
        </div>

        <!-- Preview Table Section -->
        <div class="space-y-2 pt-2">
          <div class="flex items-center justify-between">
            <h3 class="font-bold text-brand-text-primary flex items-center gap-2">
              <span>{i18n.t("organizer.previewTitle", { count: items.length })}</span>
              {#if isLoading}
                <RefreshCw class="w-3 h-3 text-brand-accent-text animate-spin" />
              {/if}
            </h3>

            <!-- Summary badges -->
            <div class="flex items-center gap-2 text-[11px]">
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
          <div class="h-64 border border-brand-border/60 rounded-xl overflow-hidden bg-brand-sidebar/40">
            {#if items.length === 0}
              <div class="h-full flex items-center justify-center text-brand-text-secondary">
                {#if isLoading}
                  <RefreshCw class="w-5 h-5 text-brand-accent-text animate-spin" />
                {:else}
                  <span>No tracks to organize.</span>
                {/if}
              </div>
            {:else}
              <VirtualList items={items} height="100%" itemHeight={36}>
                {#snippet children(item: OrganizePreviewItem)}
                  {@const st = getItemStatus(item)}
                  <div
                    class="h-9 px-3 flex items-center border-b border-brand-border/20 text-[11px] font-mono hover:bg-brand-accent/10 transition-colors"
                  >
                    <div class="w-28 shrink-0">
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

                    <div class="flex-1 truncate px-2 {item.from_path ? 'text-brand-text-secondary' : 'text-rose-400 font-medium'}" title={item.from_path}>
                      {item.from_path || "(No path recorded)"}
                    </div>
                    <div class="text-brand-text-secondary px-1">→</div>
                    <div
                      class="flex-1 truncate px-2 {st === 'ok' ? 'text-emerald-400 font-semibold' : st === 'collision' || st === 'error' ? 'text-rose-400 font-semibold' : 'text-brand-text-primary'}"
                      title={item.error_message ? `${item.to_path ? item.to_path + ' — ' : ''}${item.error_message}` : item.to_path}
                    >
                      {#if st === 'error' || st === 'collision' || item.error_message}
                        <span class="{st === 'missing_tag' ? 'text-amber-400' : 'text-rose-400'} font-medium">
                          {item.error_message ? item.error_message : (item.to_path || 'Unknown error')}
                        </span>
                      {:else}
                        {item.to_path}
                      {/if}
                    </div>
                  </div>
                {/snippet}
              </VirtualList>
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
            Cancel
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
