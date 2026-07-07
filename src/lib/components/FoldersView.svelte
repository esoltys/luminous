<script lang="ts">
  import { collectionStore } from "../stores/collection.svelte";
  import { Folder, Plus, Trash2, HelpCircle } from "lucide-svelte";
  import { open } from "@tauri-apps/plugin-dialog";

  async function handleAddDirectory() {
    try {
      const selected = await open({
        directory: true,
        multiple: false,
        title: "Select Music Directory",
      });
      if (selected && typeof selected === "string") {
        await collectionStore.addDirectory(selected);
      }
    } catch (err) {
      console.error("Failed to open directory dialog:", err);
    }
  }

  async function handleRemoveDirectory(path: string) {
    if (confirm(`Stop watching folder: ${path}?\nSongs from this folder will not be removed from your playlists but will be marked unavailable.`)) {
      await collectionStore.removeDirectory(path);
    }
  }
</script>

<div class="flex-1 flex flex-col overflow-hidden bg-gray-950 text-gray-200 h-full">
  <!-- Top Header bar -->
  <div class="h-16 px-6 border-b border-gray-800 flex items-center justify-between">
    <div class="flex items-center gap-3">
      <Folder class="w-5 h-5 text-violet-400" />
      <h2 class="text-base font-bold text-white">Watched Folders</h2>
    </div>
    <button
      onclick={handleAddDirectory}
      class="bg-violet-600 hover:bg-violet-500 text-white px-3.5 py-1.5 rounded-lg text-xs font-semibold flex items-center gap-1.5 transition-colors shadow-lg shadow-violet-900/30"
    >
      <Plus class="w-4 h-4" /> Add Folder
    </button>
  </div>

  <!-- Content -->
  <div class="flex-1 overflow-y-auto p-6 max-w-4xl space-y-6">
    <!-- Info Banner -->
    <div class="bg-violet-950/20 border border-violet-900/40 rounded-xl p-4 flex gap-3.5 text-sm text-gray-300">
      <HelpCircle class="w-5 h-5 text-violet-400 shrink-0 mt-0.5" />
      <div class="space-y-1">
        <h4 class="font-semibold text-white">How do Watched Folders work?</h4>
        <p class="text-xs text-gray-400 leading-relaxed">
          Luminous monitors these folders for audio files (MP3, FLAC, M4A, etc.). When you press "Rescan Library",
          the scanner recursively searches these folders and adds new/updated tracks to your collection.
          Luminous uses mtime-based hashing to perform fast incremental scans so that files that haven't changed
          are scanned instantly.
        </p>
      </div>
    </div>

    <!-- Folders List -->
    <div class="space-y-3">
      <h3 class="text-xs text-gray-500 font-semibold tracking-wider uppercase">Currently Watching</h3>
      <div class="space-y-2">
        {#each collectionStore.directories as dir}
          <div class="flex items-center justify-between bg-gray-900/60 border border-gray-800 rounded-xl p-4 hover:border-gray-700 transition-colors">
            <div class="flex items-center gap-3.5 min-w-0">
              <div class="w-10 h-10 rounded-lg bg-gray-950 border border-gray-800 flex items-center justify-center text-violet-400">
                <Folder class="w-5 h-5" />
              </div>
              <div class="min-w-0">
                <p class="text-sm font-medium text-white truncate" title={dir.path}>{dir.path}</p>
                <p class="text-[10px] text-gray-500 mt-0.5">Recursive scanning active</p>
              </div>
            </div>
            <button
              onclick={() => handleRemoveDirectory(dir.path)}
              class="p-2 rounded-lg bg-gray-950 hover:bg-red-950/30 text-gray-500 hover:text-red-400 border border-gray-800 hover:border-red-900/40 transition-colors"
              title="Stop watching this folder"
            >
              <Trash2 class="w-4 h-4" />
            </button>
          </div>
        {/each}

        {#if collectionStore.directories.length === 0}
          <div class="border border-dashed border-gray-800 rounded-xl py-12 text-center text-gray-500">
            <Folder class="w-12 h-12 mx-auto mb-2 text-gray-700" />
            <h4 class="font-semibold text-gray-400 mb-1">No Watched Folders</h4>
            <p class="text-xs text-gray-500 mb-4">Click "Add Folder" above to add your music directory.</p>
          </div>
        {/if}
      </div>
    </div>
  </div>
</div>
