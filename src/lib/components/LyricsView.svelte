<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { playerStore } from "../stores/player.svelte";
  import { FileText, Edit3, Save, X, RefreshCw, LoaderCircle } from "lucide-svelte";

  interface LyricLine {
    timeMs: number;
    text: string;
  }

  let lyricsText = $state("");
  let isLoading = $state(false);
  let errorMsg = $state("");
  let isEditing = $state(false);
  let editText = $state("");
  let containerEl = $state<HTMLDivElement | null>(null);

  // Parse lyrics from LRC string
  let parsedLines = $derived.by<LyricLine[]>(() => {
    if (!lyricsText) return [];
    const lines = lyricsText.split("\n");
    const parsed: LyricLine[] = [];
    const timeRegex = /\[(\d+):(\d+)(?:[.:](\d+))?\]/;

    for (const line of lines) {
      const match = timeRegex.exec(line);
      if (match) {
        const minutes = parseInt(match[1], 10);
        const seconds = parseInt(match[2], 10);
        const hundredths = match[3] ? parseInt(match[3], 10) : 0;

        const timeMs = minutes * 60 * 1000 + seconds * 1000 + (match[3] && match[3].length === 2 ? hundredths * 10 : hundredths);
        const text = line.replace(timeRegex, "").trim();
        parsed.push({ timeMs, text });
      }
    }
    return parsed.sort((a, b) => a.timeMs - b.timeMs);
  });

  let isSynced = $derived(parsedLines.length > 0);

  // Find active line index based on current playback position
  let activeLineIndex = $derived.by(() => {
    if (!isSynced || parsedLines.length === 0) return -1;
    const currentMs = playerStore.positionNanosec / 1_000_000;

    let matchIdx = -1;
    for (let i = 0; i < parsedLines.length; i++) {
      if (currentMs >= parsedLines[i].timeMs) {
        matchIdx = i;
      } else {
        break;
      }
    }
    return matchIdx;
  });

  async function loadLyrics(songId: number | undefined, forceRefresh = false) {
    if (songId === undefined) {
      lyricsText = "";
      errorMsg = "";
      return;
    }

    isLoading = true;
    errorMsg = "";
    isEditing = false;

    try {
      if (forceRefresh) {
        // Clear local cache first to force online refetch
        await invoke("save_lyrics", { songId, lyrics: "" });
      }
      const lyrics = await invoke<string>("get_lyrics", { songId });
      lyricsText = lyrics;
      editText = lyrics;
    } catch (e: any) {
      console.error("Failed to load lyrics:", e);
      errorMsg = e.toString();
      lyricsText = "";
      editText = "";
    } finally {
      isLoading = false;
    }
  }

  async function saveManualLyrics() {
    if (!playerStore.currentSong) return;
    try {
      await invoke("save_lyrics", { songId: playerStore.currentSong.id, lyrics: editText });
      lyricsText = editText;
      isEditing = false;
    } catch (e: any) {
      console.error("Failed to save lyrics:", e);
      alert("Failed to save lyrics: " + e.toString());
    }
  }

  function startEditing() {
    editText = lyricsText;
    isEditing = true;
  }

  // Load lyrics when song changes
  $effect(() => {
    loadLyrics(playerStore.currentSong?.id);
  });

  // Auto-scroll to active lyric line
  $effect(() => {
    if (activeLineIndex !== -1 && containerEl && !isEditing) {
      const activeEl = containerEl.querySelector(`[data-index="${activeLineIndex}"]`);
      if (activeEl) {
        activeEl.scrollIntoView({
          behavior: "smooth",
          block: "center",
        });
      }
    }
  });
</script>

<div class="flex-1 flex flex-col h-full bg-gray-950 text-gray-200 select-none overflow-hidden relative">
  <!-- Top Panel Toolbar -->
  <div class="h-16 flex items-center justify-between px-8 border-b border-gray-800 shrink-0">
    <div class="flex items-center gap-3">
      <FileText class="w-6 h-6 text-violet-400" />
      <div>
        <h2 class="text-sm font-bold truncate max-w-xs md:max-w-md">
          {playerStore.currentSong ? playerStore.currentSong.title : "No song playing"}
        </h2>
        <p class="text-[10px] text-gray-400 truncate max-w-xs md:max-w-md">
          {playerStore.currentSong ? `${playerStore.currentSong.artist || "Unknown Artist"} — ${playerStore.currentSong.album || "Unknown Album"}` : "Select a track to view lyrics"}
        </p>
      </div>
    </div>

    <!-- Actions -->
    {#if playerStore.currentSong}
      <div class="flex items-center gap-2">
        {#if !isEditing}
          <button
            onclick={() => loadLyrics(playerStore.currentSong?.id, true)}
            class="flex items-center gap-1.5 bg-gray-900 border border-gray-800 hover:bg-gray-800 text-gray-400 hover:text-white px-3 py-1.5 rounded-lg text-xs font-semibold transition-all duration-150"
            title="Refetch lyrics online"
          >
            <RefreshCw class="w-3.5 h-3.5" /> Refetch
          </button>
          <button
            onclick={startEditing}
            class="flex items-center gap-1.5 bg-violet-600 hover:bg-violet-500 text-white px-3 py-1.5 rounded-lg text-xs font-semibold transition-all duration-150 shadow-lg shadow-violet-900/30"
          >
            <Edit3 class="w-3.5 h-3.5" /> Edit
          </button>
        {:else}
          <button
            onclick={() => { isEditing = false; }}
            class="flex items-center gap-1.5 bg-gray-900 border border-gray-800 hover:bg-gray-800 text-gray-400 hover:text-white px-3 py-1.5 rounded-lg text-xs font-semibold transition-all"
          >
            <X class="w-3.5 h-3.5" /> Cancel
          </button>
          <button
            onclick={saveManualLyrics}
            class="flex items-center gap-1.5 bg-green-600 hover:bg-green-500 text-white px-3 py-1.5 rounded-lg text-xs font-semibold transition-all shadow-lg shadow-green-900/30"
          >
            <Save class="w-3.5 h-3.5" /> Save
          </button>
        {/if}
      </div>
    {/if}
  </div>

  <!-- Lyrics Container Viewport -->
  <div class="flex-1 overflow-y-auto px-6 py-12" bind:this={containerEl}>
    {#if isLoading}
      <div class="w-full h-full flex flex-col items-center justify-center gap-3">
        <LoaderCircle class="w-8 h-8 animate-spin text-violet-400" />
        <span class="text-xs text-gray-500 font-medium">Fetching lyrics...</span>
      </div>
    {:else if isEditing}
      <!-- Editor Mode -->
      <div class="max-w-2xl mx-auto h-full flex flex-col gap-3">
        <label for="lyrics-editor" class="text-xs font-bold text-gray-400 uppercase tracking-wider">Lyrics Text (plain or LRC synced format)</label>
        <textarea
          id="lyrics-editor"
          bind:value={editText}
          class="flex-1 bg-gray-900 border border-gray-800 rounded-xl p-4 text-sm font-mono text-gray-200 outline-none focus:border-violet-500 resize-none h-[calc(100vh-280px)] focus:ring-1 focus:ring-violet-500"
          placeholder="Paste synced LRC or plain text lyrics here..."
        ></textarea>
      </div>
    {:else if lyricsText}
      <div class="max-w-3xl mx-auto text-center">
        {#if isSynced}
          <!-- Synced View -->
          <div class="flex flex-col gap-6 md:gap-8 pb-32">
            {#each parsedLines as line, idx}
              {@const isActive = idx === activeLineIndex}
              <!-- svelte-ignore a11y_click_events_have_key_events -->
              <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
              <p
                data-index={idx}
                onclick={() => playerStore.seek(line.timeMs * 1_000_000)}
                class="text-xl md:text-2xl font-bold cursor-pointer transition-all duration-300 transform {isActive ? 'text-white scale-105 filter drop-shadow-[0_0_8px_rgba(139,92,246,0.5)]' : 'text-gray-600 hover:text-gray-400'}"
              >
                {line.text || "•••"}
              </p>
            {/each}
          </div>
        {:else}
          <!-- Plain Text View -->
          <div class="whitespace-pre-line text-lg leading-relaxed text-gray-300 select-text pb-20 font-medium">
            {lyricsText}
          </div>
        {/if}
      </div>
    {:else if errorMsg}
      <div class="w-full h-full flex flex-col items-center justify-center gap-3 p-8 text-center">
        <p class="text-sm font-semibold text-red-400">Unable to load lyrics</p>
        <p class="text-xs text-gray-500 max-w-sm">{errorMsg}</p>
        <button
          onclick={() => loadLyrics(playerStore.currentSong?.id)}
          class="mt-2 bg-gray-900 hover:bg-gray-800 border border-gray-800 text-gray-300 px-4 py-2 rounded-lg text-xs font-semibold transition-all"
        >
          Retry Search
        </button>
      </div>
    {:else}
      <div class="w-full h-full flex flex-col items-center justify-center gap-2 text-center text-gray-500">
        <FileText class="w-12 h-12 stroke-[1] text-gray-700 mb-2" />
        {#if playerStore.currentSong}
          <p class="text-sm font-semibold text-gray-400">No lyrics found for this song</p>
          <p class="text-xs text-gray-600 max-w-xs mt-1">Try clicking 'Edit' above to manually paste the lyrics.</p>
        {:else}
          <p class="text-sm font-semibold text-gray-400">No song selected</p>
          <p class="text-xs text-gray-600 mt-1">Start playback to fetch lyrics.</p>
        {/if}
      </div>
    {/if}
  </div>
</div>
