<script lang="ts">
  import { ChevronLeft, ChevronRight, Search, FolderOpen, PanelLeft, PanelBottom, PanelRight } from "lucide-svelte";
  import { collectionStore } from "../stores/collection.svelte";
  import { playerStore } from "../stores/player.svelte";
  import { open } from "@tauri-apps/plugin-dialog";
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

  async function handleOpenFiles() {
    try {
      const selected = await open({
        multiple: true,
        directory: false,
        title: "Open Audio Files or Playlists",
        filters: [
          {
            name: "Supported Files",
            extensions: ["mp3", "flac", "ogg", "opus", "m4a", "aac", "alac", "wav", "aiff", "aif", "wv", "mpc", "ape", "tta", "dsf", "dff", "asf", "wma", "m4b", "m3u"]
          },
          {
            name: "Audio Files",
            extensions: ["mp3", "flac", "ogg", "opus", "m4a", "aac", "alac", "wav", "aiff", "aif", "wv", "mpc", "ape", "tta", "dsf", "dff", "asf", "wma", "m4b"]
          },
          {
            name: "Playlists",
            extensions: ["m3u"]
          }
        ]
      });

      if (selected) {
        const paths = Array.isArray(selected) ? selected : [selected];
        if (paths.length > 0) {
          await playerStore.openAndPlay(paths);
        }
      }
    } catch (err) {
      console.error("Failed to open files/playlists:", err);
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

<header in:fade={{ duration: 600 }} class="w-full h-20 bg-brand-secondary border-b border-brand-border flex items-center px-6 gap-6 z-40">
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

    <!-- Open Files/Playlists button -->
    <button
      type="button"
      onclick={handleOpenFiles}
      class="p-1 text-brand-text-secondary hover:text-brand-accent transition-colors flex-shrink-0 cursor-pointer"
      title="Open Audio Files or Playlists (*.m3u)"
    >
      <FolderOpen class="w-4 h-4" />
    </button>

    <!-- Search feedback / progress -->
    {#if collectionStore.searchLoading}
      <div class="animate-spin rounded-full h-4 w-4 border-2 border-brand-accent border-t-transparent flex-shrink-0" title="Searching..."></div>
    {:else if collectionStore.searchQuery}
      <span class="text-[10px] bg-brand-border/60 px-1.5 py-0.5 rounded text-brand-text-secondary font-mono flex-shrink-0 select-none">
        {collectionStore.searchResults.length} tracks
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
      onclick={() => collectionStore.toggleImmersiveMode()}
      class="p-1.5 rounded-md transition-all cursor-pointer {!collectionStore.immersiveMode ? 'bg-brand-border text-brand-accent shadow-sm' : 'text-brand-text-secondary hover:text-brand-text-primary hover:bg-brand-main/50'}"
      title="Toggle Immersive Album Art Screen"
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
  <div class="flex items-center justify-center flex-shrink-0">
    <ReactiveLogoBrand size="lg" />
  </div>
</header>

<style>
  header {
    backdrop-filter: blur(8px);
    background-color: rgba(10, 8, 19, 0.8);
  }
</style>
