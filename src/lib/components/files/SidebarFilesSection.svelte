<script lang="ts">
  import { open } from "@tauri-apps/plugin-dialog";
  import { FoldersIcon, FolderIcon, HardDrivesIcon, CloudIcon, PlusIcon } from "phosphor-svelte";
  import { navigationStore } from "../../stores/navigation.svelte";
  import { filesStore, sameSource } from "../../stores/files.svelte";
  import { collectionStore } from "../../stores/collection.svelte";
  import { i18n, formatNumber } from "../../stores/i18n.svelte";
  import type { FileSource } from "../../types";

  let { collapsed, showSubItems }: { collapsed: boolean; showSubItems: boolean } = $props();

  const active = $derived(navigationStore.activeTab === "files");
  const atOverview = $derived(active && navigationStore.filesLocation === null);

  function isOpen(source: FileSource): boolean {
    const location = navigationStore.filesLocation;
    return active && location !== null && sameSource(location.source, source);
  }

  async function addMusicFolder() {
    try {
      const selected = await open({ directory: true, multiple: false, title: i18n.t("settings.selectMusicDirectory") });
      if (selected && typeof selected === "string") await collectionStore.addDirectory(selected);
    } catch (err) {
      console.error("Failed to add a music folder:", err);
    }
  }
</script>

<div class="w-full flex flex-col {collapsed ? 'items-center' : ''}" data-testid="sidebar-files">
  {#if collapsed}
    <button
      onclick={() => navigationStore.viewFiles()}
      class="flex items-center justify-center w-10 h-10 rounded-xl p-0 transition-colors duration-150 {active ? 'bg-brand-accent text-brand-accent-contrast shadow-lg shadow-brand-accent/20' : 'text-brand-text-secondary hover:bg-brand-accent/10 hover:text-brand-accent-text-hover'}"
      title={i18n.t("sidebar.files")}
      aria-label={i18n.t("sidebar.files")}
    >
      <FoldersIcon class="w-5 h-5" />
    </button>
  {:else}
    <div class="w-full flex items-center rounded-lg transition-colors duration-150 {atOverview ? 'bg-brand-accent/20 text-brand-accent-text' : 'text-brand-text-secondary hover:bg-brand-accent/10 hover:text-brand-accent-text-hover'}">
      <button
        onclick={() => navigationStore.viewFiles()}
        class="flex-1 min-w-0 flex items-center gap-3 px-3 py-1.5 text-sm font-medium text-left"
      >
        <div class="w-7 h-7 flex items-center justify-center shrink-0">
          <FoldersIcon class="w-4 h-4 icon-align" />
        </div>
        <span class="truncate whitespace-nowrap">{i18n.t("sidebar.files")}</span>
      </button>
      <button
        onclick={() => navigationStore.openSettings("sources")}
        class="mr-1.5 w-6 h-6 flex items-center justify-center rounded-md text-brand-text-secondary hover:text-brand-text-primary hover:bg-brand-accent/10 transition-colors"
        title={i18n.t("files.addSource")}
        aria-label={i18n.t("files.addSource")}
      >
        <PlusIcon class="w-3.5 h-3.5" />
      </button>
    </div>
  {/if}

  {#if showSubItems}
    <div class="pl-[9px] pr-1 py-0.5 space-y-0 border-l-2 border-brand-accent/30 ml-[calc(var(--sidebar-icon-axis)_-_1px)] my-0.5">
      {#if filesStore.loaded && filesStore.sources.length === 0}
        <button
          onclick={addMusicFolder}
          class="w-full flex items-center gap-2 px-2.5 py-1 rounded-md text-xs font-semibold text-brand-accent-text hover:bg-brand-accent/10 transition-colors"
        >
          <PlusIcon class="w-3.5 h-3.5 icon-align" />
          <span class="truncate">{i18n.t("files.addMusicFolder")}</span>
        </button>
      {/if}
      {#each filesStore.sources as entry (`${entry.source.kind}:${entry.source.id}`)}
        <button
          onclick={() => navigationStore.viewFiles(entry.source)}
          class="w-full flex items-center justify-between px-2.5 py-1 rounded-md text-xs transition-colors {isOpen(entry.source) ? 'bg-brand-accent/20 text-brand-accent-text font-semibold' : 'text-brand-text-secondary hover:text-brand-text-primary hover:bg-brand-accent/10'} {entry.available ? '' : 'opacity-60'}"
        >
          <div class="flex items-center gap-2 truncate">
            {#if entry.source.kind === "subsonic"}
              <HardDrivesIcon class="w-3.5 h-3.5 icon-align shrink-0" />
            {:else if entry.source.kind === "webdav"}
              <CloudIcon class="w-3.5 h-3.5 icon-align shrink-0" />
            {:else}
              <FolderIcon class="w-3.5 h-3.5 icon-align shrink-0" />
            {/if}
            <span class="truncate">{entry.name}</span>
          </div>
          <span class="text-[10px] text-brand-text-secondary/60 ml-1">({formatNumber(entry.songCount)})</span>
        </button>
      {/each}
      <!-- Connected devices follow under a divider (#839). -->
    </div>
  {/if}
</div>
