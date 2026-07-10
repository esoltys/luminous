<script lang="ts">
  import { ChevronLeft, ChevronRight, Search, FolderOpen } from "lucide-svelte";
  import { collectionStore } from "../stores/collection.svelte";
  import ReactiveLogoBrand from "./ReactiveLogoBrand.svelte";

  let searchQuery = $state("");
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

  // Search handler
  async function handleSearch(e: Event) {
    e.preventDefault();
    await collectionStore.search(searchQuery);
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

<header class="fixed top-0 left-0 w-full h-20 bg-brand-secondary border-b border-brand-border flex items-center px-6 gap-6 z-40">
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
      bind:value={searchQuery}
      type="text"
      placeholder="Search tracks, albums, artists... (Ctrl+L)"
      class="flex-1 bg-transparent text-brand-text-primary text-sm focus:outline-none placeholder-brand-text-secondary/50"
    />
    <button
      type="button"
      onclick={handleFolderIngest}
      class="p-1 text-brand-text-secondary hover:text-brand-accent hover:bg-brand-sidebar rounded transition-colors"
      title="Add folder to index"
    >
      <FolderOpen class="w-4 h-4" />
    </button>
  </form>

  <!-- Reactive Logo Brand -->
  <div class="ml-auto">
    <ReactiveLogoBrand size="md" />
  </div>
</header>

<style>
  header {
    backdrop-filter: blur(8px);
    background-color: rgba(10, 8, 19, 0.8);
  }
</style>
