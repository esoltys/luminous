<script lang="ts">
  import { ChevronLeft, ChevronRight, Search, FolderOpen, PanelLeft, PanelBottom, PanelRight } from "lucide-svelte";
  import { collectionStore } from "../stores/collection.svelte";
  import ReactiveLogoBrand from "./ReactiveLogoBrand.svelte";
  import { fade } from "svelte/transition";

  let searchInput: HTMLInputElement | undefined;

  // Navigation history stack
  let historyStack = $state<string[]>([]);
  let historyIndex = $state(-1);

  // Handle Ctrl+L to focus search
  function handleKeyDown(e: KeyboardEvent) {
    if ((e.ctrlKey || e.metaKey) && e.key === "l") {
      e.preventDefault();
      searchInput?.focus();
    }
  }

  // Search handler (prevent reload)
  function handleSearch(e: Event) {
    e.preventDefault();
  }

  // Clear search query
  function clearSearch() {
    collectionStore.searchQuery = "";
    collectionStore.search("");
  }

  // Folder ingestion trigger
  async function handleFolderIngest() {
    try {
      const { open } = await import("@tauri-apps/plugin-dialog");
      const selected = await open({
        directory: true,
        multiple: false,
        title: "Select Music Directory to Index"
      });
      if (selected && typeof selected === "string") {
        await collectionStore.addDirectory(selected);
      }
    } catch (err) {
      console.error("Failed to open folder dialog:", err);
    }
  }

  // Navigation history handlers
  function goBack() {
    if (historyIndex > 0) {
      historyIndex--;
      // Would navigate to historyStack[historyIndex]
    }
  }

  function goForward() {
    if (historyIndex < historyStack.length - 1) {
      historyIndex++;
      // Would navigate to historyStack[historyIndex]
    }
  }
</script>

<svelte:window on:keydown={handleKeyDown} />

<header in:fade={{ duration: 600 }} class="fixed top-0 left-0 w-full h-20 bg-brand-secondary border-b border-brand-border flex items-center px-6 gap-6 z-40">
  <!-- History Navigation Controls -->
  <div class="flex items-center gap-2">
    <button
      onclick={goBack}
      disabled={historyIndex <= 0}
      class="p-2 rounded-lg text-brand-text-secondary hover:bg-brand-main hover:text-brand-text-primary disabled:opacity-50 disabled:cursor-not-allowed transition-colors"
      title="Go back"
    >
      <ChevronLeft class="w-5 h-5" />
    </button>
    <button
      onclick={goForward}
      disabled={historyIndex >= historyStack.length - 1}
      class="p-2 rounded-lg text-brand-text-secondary hover:bg-brand-main hover:text-brand-text-primary disabled:opacity-50 disabled:cursor-not-allowed transition-colors"
      title="Go forward"
    >
      <ChevronRight class="w-5 h-5" />
    </button>
  </div>

  <!-- Divider -->
  <div class="w-px h-6 bg-brand-border"></div>

  <!-- Universal Search Bar -->
  <form onsubmit={handleSearch} class="flex-1 max-w-2xl flex items-center gap-3 bg-brand-main rounded-lg px-4 py-2 border border-brand-border focus-within:border-brand-accent transition-colors">
    <Search class="w-4 h-4 text-brand-text-secondary flex-shrink-0" />
    <input
      bind:this={searchInput}
      bind:value={collectionStore.searchQuery}
      type="text"
      placeholder="Search tracks, albums, artists... (Ctrl+L)"
      class="flex-1 bg-transparent text-brand-text-primary text-sm focus:outline-none placeholder-brand-text-secondary/50"
    />

    <!-- Search feedback / progress -->
    {#if collectionStore.searchLoading}
      <div class="animate-spin rounded-full h-4 w-4 border-2 border-brand-accent border-t-transparent flex-shrink-0" title="Searching..."></div>
    {:else if collectionStore.searchQuery}
      <span class="text-[10px] bg-brand-border/60 px-1.5 py-0.5 rounded text-brand-text-secondary font-mono flex-shrink-0 select-none">
        {collectionStore.searchResults.length} results
      </span>
      <button
        type="button"
        onclick={clearSearch}
        class="p-1 text-brand-text-secondary hover:text-brand-accent transition-colors flex-shrink-0 font-bold leading-none text-sm"
        title="Clear Search"
      >
        ✕
      </button>
    {/if}

    <button
      type="button"
      onclick={handleFolderIngest}
      class="p-1 text-brand-text-secondary hover:text-brand-accent hover:bg-brand-sidebar rounded transition-colors flex-shrink-0"
      title="Add folder to index"
    >
      <FolderOpen class="w-4 h-4" />
    </button>
  </form>

  <!-- Layout Panel Toggles Group -->
  <div class="flex items-center gap-1.5 bg-brand-main/60 p-1 rounded-lg border border-brand-border/60 ml-auto flex-shrink-0 select-none">
    <button
      onclick={() => collectionStore.toggleSidebar()}
      class="p-1.5 rounded-md transition-all cursor-pointer {collectionStore.sidebarOpen ? 'bg-brand-border text-brand-accent shadow-sm' : 'text-brand-text-secondary hover:text-brand-text-primary hover:bg-brand-main/50'}"
      title="Toggle Left Sidebar"
    >
      <PanelLeft class="w-4 h-4" />
    </button>
    <button
      onclick={() => collectionStore.togglePlayerBar()}
      class="p-1.5 rounded-md transition-all cursor-pointer {collectionStore.playerBarOpen ? 'bg-brand-border text-brand-accent shadow-sm' : 'text-brand-text-secondary hover:text-brand-text-primary hover:bg-brand-main/50'}"
      title="Toggle Bottom Player Bar"
    >
      <PanelBottom class="w-4 h-4" />
    </button>
    <button
      onclick={() => collectionStore.toggleRightPanel()}
      class="p-1.5 rounded-md transition-all cursor-pointer {collectionStore.rightPanelOpen ? 'bg-brand-border text-brand-accent shadow-sm' : 'text-brand-text-secondary hover:text-brand-text-primary hover:bg-brand-main/50'}"
      title="Toggle Right Panel"
    >
      <PanelRight class="w-4 h-4" />
    </button>
  </div>

  <!-- Reactive Logo Brand -->
  <div class="flex-shrink-0">
    <ReactiveLogoBrand size="md" />
  </div>
</header>

<style>
  header {
    backdrop-filter: blur(8px);
    background-color: rgba(10, 8, 19, 0.8);
  }
</style>
