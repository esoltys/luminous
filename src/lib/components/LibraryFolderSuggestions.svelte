<script lang="ts">
  import { onMount } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { listen } from "@tauri-apps/api/event";
  import { FolderSimplePlusIcon as FolderPlus } from "phosphor-svelte";
  import { i18n } from "../stores/i18n.svelte";
  import { collectionStore } from "../stores/collection.svelte";
  import type { SuggestedFolder } from "../types";
  import Button from "./Button.svelte";

  /** Folders shown at once; the backend returns the most recently played first. */
  const MAX_FOLDERS = 3;

  let folders = $state<SuggestedFolder[]>([]);
  let refreshDebounce: ReturnType<typeof setTimeout> | undefined;
  /** Only the newest fetch may write `folders`, so a slow earlier one can't
   * resurrect a folder the user has just added or dismissed. */
  let fetchSeq = 0;

  async function refresh() {
    const seq = ++fetchSeq;
    try {
      const next = await invoke<SuggestedFolder[]>("get_suggested_library_folders", {
        limit: MAX_FOLDERS,
      });
      if (seq === fetchSeq) folders = next;
    } catch (err) {
      console.error("Failed to load library folder suggestions:", err);
    }
  }

  function refreshSoon() {
    clearTimeout(refreshDebounce);
    refreshDebounce = setTimeout(refresh, 500);
  }

  /** Last path segment, for a short label; the full path stays visible below it. */
  function folderName(path: string): string {
    const parts = path.split(/[\\/]+/).filter(Boolean);
    return parts.at(-1) ?? path;
  }

  async function addFolder(path: string) {
    try {
      await collectionStore.addDirectory(path);
    } catch (err) {
      console.error("Failed to add suggested folder:", err);
    }
    // The folder is watched now, so the backend stops offering it. Ask instead
    // of waiting for `library-changed`: a scan that finds nothing to change
    // (files already recorded) never emits it.
    await refresh();
  }

  // The backend emits `library-folder-suggestions-changed` once the write has
  // landed; nothing is removed from the list optimistically.
  function notNow(path: string) {
    invoke("dismiss_library_folder_suggestion", { path }).catch((err) =>
      console.error("Failed to dismiss folder suggestion:", err),
    );
  }

  function hideAll() {
    invoke("set_library_folder_suggestions_hidden", { hidden: true }).catch((err) =>
      console.error("Failed to hide folder suggestions:", err),
    );
  }

  onMount(() => {
    refresh();
    // Added folders and new opened files change what is offered; plugging or
    // unplugging a drive changes which folders are reachable.
    const unlisteners = [
      listen("library-changed", refreshSoon),
      listen("library-folder-suggestions-changed", refresh),
      listen("devices-changed", refreshSoon),
    ];
    return () => {
      clearTimeout(refreshDebounce);
      for (const unlisten of unlisteners) unlisten.then((fn) => fn());
    };
  });
</script>

{#if folders.length > 0}
  <section
    class="rounded-xl border border-brand-border bg-brand-sidebar/40 p-4 @xl:p-5"
    aria-labelledby="library-folder-suggestions-title"
  >
    <div class="flex items-start justify-between gap-4">
      <div class="min-w-0">
        <h2
          id="library-folder-suggestions-title"
          class="text-base font-semibold text-brand-text-primary text-balance"
        >
          {i18n.t("home.suggestTitle")}
        </h2>
        <p class="mt-1 text-xs text-brand-text-secondary leading-relaxed text-pretty">
          {i18n.t("home.suggestHint")}
        </p>
      </div>
      <button
        type="button"
        onclick={hideAll}
        class="shrink-0 text-xs text-brand-text-secondary underline-offset-2 hover:underline hover:text-brand-accent-text transition-colors"
      >
        {i18n.t("home.suggestHide")}
      </button>
    </div>

    <ul class="mt-3 flex flex-col divide-y divide-brand-border/60">
      {#each folders as folder (folder.path)}
        {@const name = folderName(folder.path)}
        <li class="flex flex-wrap items-center gap-x-4 gap-y-2 py-2.5">
          <div class="min-w-0 flex-1 basis-48">
            <div class="truncate text-sm font-medium text-brand-text-primary">{name}</div>
            <div class="truncate text-[11px] text-brand-text-secondary" title={folder.path}>
              {folder.path}
            </div>
            <div class="text-[11px] text-brand-text-secondary">
              {i18n.plural("home.suggestSongs", folder.song_count)}
            </div>
          </div>
          <div class="flex shrink-0 items-center gap-2">
            <Button
              variant="primary"
              size="sm"
              aria-label={i18n.t("home.suggestAddNamed", { folder: name })}
              onclick={() => addFolder(folder.path)}
            >
              <FolderPlus class="w-3.5 h-3.5" />
              {i18n.t("home.suggestAdd")}
            </Button>
            <Button
              variant="secondary"
              size="sm"
              aria-label={i18n.t("home.suggestNotNowNamed", { folder: name })}
              onclick={() => notNow(folder.path)}
            >
              {i18n.t("home.suggestNotNow")}
            </Button>
          </div>
        </li>
      {/each}
    </ul>
  </section>
{/if}
