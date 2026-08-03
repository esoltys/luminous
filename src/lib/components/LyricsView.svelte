<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { playerStore } from "../stores/player.svelte";
  import { FileText, Edit3, Save, X, RefreshCw, Music2 } from "lucide-svelte";
  import LoadingSpinner from "./LoadingSpinner.svelte";
  import Button from "./Button.svelte";
  import { i18n } from "../stores/i18n.svelte";
  import { toastStore } from "../stores/toast.svelte";
  import { rememberScroll } from "../utils/scrollMemory";

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

  // Parse lyrics from LRC string supporting multiple timestamps per line
  let parsedLines = $derived.by<LyricLine[]>(() => {
    if (!lyricsText) {
      console.log("[LyricsView] parsedLines: lyricsText is empty");
      return [];
    }
    
    // Clean marker if present
    let cleanText = lyricsText;
    if (cleanText.startsWith("[synced:false]\n")) {
      cleanText = cleanText.substring("[synced:false]\n".length);
    } else if (cleanText.startsWith("[synced:false]")) {
      cleanText = cleanText.substring("[synced:false]".length);
    }

    const lines = cleanText.split("\n");
    const parsed: LyricLine[] = [];
    const timeRegex = /\[(\d+):(\d+)(?:[.:](\d+))?\]/g;

    for (const line of lines) {
      const matches: { timeMs: number }[] = [];
      let match;
      
      // Reset the regex state before scanning
      timeRegex.lastIndex = 0;
      
      while ((match = timeRegex.exec(line)) !== null) {
        const minutes = parseInt(match[1], 10);
        const seconds = parseInt(match[2], 10);
        const hundredths = match[3] ? parseInt(match[3], 10) : 0;

        const timeMs = minutes * 60 * 1000 + seconds * 1000 + (match[3] && match[3].length === 2 ? hundredths * 10 : hundredths);
        matches.push({ timeMs });
      }

      if (matches.length > 0) {
        const text = line.replace(timeRegex, "").trim();
        for (const m of matches) {
          parsed.push({ timeMs: m.timeMs, text });
        }
      }
    }
    const sorted = parsed.sort((a, b) => a.timeMs - b.timeMs);
    console.log(`[LyricsView] parsedLines: parsed ${sorted.length} lines. isSynced: ${sorted.length > 0}`);
    return sorted;
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
    
    // Log every 5 seconds or on line index changes to avoid console flooding
    if (matchIdx !== -1 && (matchIdx % 5 === 0 || currentMs % 5000 < 250)) {
      console.log(`[LyricsView] Position: ${Math.round(currentMs)}ms, Active index: ${matchIdx}, Text: "${parsedLines[matchIdx]?.text}"`);
    }
    return matchIdx;
  });

  async function loadLyrics(songId: number | undefined, forceRefresh = false) {
    if (songId === undefined) {
      console.log("[LyricsView] loadLyrics: no songId provided");
      lyricsText = "";
      errorMsg = "";
      return;
    }

    if (playerStore.currentSong?.is_instrumental) {
      console.log("[LyricsView] loadLyrics: track is instrumental, skipping fetch");
      lyricsText = "";
      errorMsg = "";
      return;
    }

    console.log(`[LyricsView] loadLyrics: fetching lyrics for songId: ${songId}, forceRefresh: ${forceRefresh}`);
    isLoading = true;
    errorMsg = "";
    isEditing = false;

    try {
      if (forceRefresh) {
        console.log("[LyricsView] forceRefresh is true. Clearing DB cache...");
        await invoke("save_lyrics", { songId, lyrics: "" });
      }
      const lyrics = await invoke<string>("get_lyrics", { songId });
      console.log(`[LyricsView] get_lyrics returned lyrics of length: ${lyrics?.length || 0}`);
      lyricsText = lyrics;

      // Keep the shared player store in sync so other views (e.g. the Now
      // Playing detail panel's Lyrics status) reflect a freshly downloaded
      // or refetched lyric immediately, not just after the next track change.
      if (playerStore.currentSong && playerStore.currentSong.id === songId) {
        playerStore.currentSong.lyrics = lyrics;
      }

      let cleanEditText = lyrics;
      if (cleanEditText.startsWith("[synced:false]\n")) {
        cleanEditText = cleanEditText.substring("[synced:false]\n".length);
      } else if (cleanEditText.startsWith("[synced:false]")) {
        cleanEditText = cleanEditText.substring("[synced:false]".length);
      }
      editText = cleanEditText;
    } catch (e: any) {
      console.error("[LyricsView] Failed to load lyrics:", e);
      const errStr = e.toString();
      const errStrLower = errStr.toLowerCase();
      if (errStrLower.includes("instrumental")) {
        if (playerStore.currentSong && playerStore.currentSong.id === songId) {
          playerStore.currentSong.is_instrumental = true;
        }
        errorMsg = "";
      } else if (errStrLower.includes("no lyrics found on any online provider")) {
        // Known backend error (src-tauri/src/lyrics.rs) — surface the translated
        // message instead of the raw Rust error string.
        errorMsg = i18n.t('lyrics.noOnlineResults');
      } else if (errStrLower.includes("insufficient song metadata")) {
        errorMsg = i18n.t('lyrics.insufficientMetadata');
      } else {
        errorMsg = errStr;
      }
      lyricsText = "";
      editText = "";
    } finally {
      isLoading = false;
    }
  }

  async function toggleInstrumental(isInstrumental: boolean) {
    if (!playerStore.currentSong) return;
    const songId = playerStore.currentSong.id;
    try {
      console.log(`[LyricsView] Setting instrumental=${isInstrumental} for songId: ${songId}`);
      await invoke("set_instrumental", { songId, isInstrumental });
      playerStore.currentSong.is_instrumental = isInstrumental;
      if (isInstrumental) {
        lyricsText = "";
        errorMsg = "";
      } else {
        await loadLyrics(songId, true);
      }
    } catch (e: any) {
      console.error("[LyricsView] Failed to toggle instrumental status:", e);
      toastStore.show(i18n.t('lyrics.saveFailedPrefix', {}, "Failed to save: ") + e.toString(), "error");
    }
  }

  async function saveManualLyrics() {
    if (!playerStore.currentSong) return;
    try {
      console.log(`[LyricsView] Manually saving lyrics for songId: ${playerStore.currentSong.id}`);
      await invoke("save_lyrics", { songId: playerStore.currentSong.id, lyrics: editText });
      lyricsText = editText;
      playerStore.currentSong.lyrics = editText;
      if (playerStore.currentSong.is_instrumental) {
        await invoke("set_instrumental", { songId: playerStore.currentSong.id, isInstrumental: false });
        playerStore.currentSong.is_instrumental = false;
      }
      isEditing = false;
    } catch (e: any) {
      console.error("[LyricsView] Failed to save lyrics manually:", e);
      toastStore.show(i18n.t('lyrics.saveFailedPrefix') + e.toString(), "error");
    }
  }

  function startEditing() {
    editText = lyricsText;
    isEditing = true;
  }

  // Load lyrics when song changes
  $effect(() => {
    console.log("[LyricsView] Song changed. Reloading lyrics for song ID:", playerStore.currentSong?.id);
    loadLyrics(playerStore.currentSong?.id);
  });

  // Auto-scroll to active lyric line
  $effect(() => {
    if (activeLineIndex !== -1 && containerEl && !isEditing) {
      const activeEl = containerEl.querySelector(`[data-index="${activeLineIndex}"]`);
      if (activeEl) {
        console.log(`[LyricsView] Auto-scrolling to active index ${activeLineIndex}`);
        activeEl.scrollIntoView({
          behavior: "smooth",
          block: "center",
        });
      }
    }
  });
</script>

<div class="flex-1 flex flex-col h-full bg-brand-main text-brand-text-primary select-none overflow-hidden relative">
  <!-- Top Panel Toolbar -->
  <div class="h-16 flex items-center justify-between px-8 border-b border-brand-border bg-brand-main/40 backdrop-blur-md shrink-0">
    <div class="flex items-center gap-3">
      <FileText class="w-6 h-6 text-brand-accent-text" />
      <div>
        <h2 class="text-sm font-bold truncate max-w-xs md:max-w-md text-brand-text-primary py-0.5 leading-snug">
          {playerStore.currentSong ? playerStore.currentSong.title : i18n.t('playerBar.notPlaying')}
        </h2>
        <p class="text-[10px] text-brand-text-secondary/70 truncate max-w-xs md:max-w-md">
          {playerStore.currentSong ? `${playerStore.currentSong.artist || i18n.t('collection.unknownArtist')} — ${playerStore.currentSong.album || i18n.t('collection.unknownAlbum')}` : i18n.t('lyrics.lyricsHelpText')}
        </p>
      </div>
    </div>

    <!-- Actions -->
    {#if playerStore.currentSong}
      <div class="flex items-center gap-2">
        {#if playerStore.currentSong.is_instrumental}
          <Button onclick={() => toggleInstrumental(false)} variant="secondary" size="sm">
            <Music2 class="w-3.5 h-3.5" /> {i18n.t('lyrics.unmarkInstrumental', {}, "Unmark Instrumental")}
          </Button>
        {:else if !isEditing}
          <Button onclick={() => loadLyrics(playerStore.currentSong?.id, true)} variant="secondary" size="sm" title={i18n.t('lyrics.refetchTooltip', {}, "Refetch lyrics online")}>
            <RefreshCw class="w-3.5 h-3.5" /> {i18n.t('lyrics.refetchBtn', {}, "Refetch")}
          </Button>
          <Button onclick={startEditing} variant="primary" size="sm">
            <Edit3 class="w-3.5 h-3.5" /> {i18n.t('settings.editThemeShort')}
          </Button>
        {:else}
          <Button onclick={() => { isEditing = false; }} variant="secondary" size="sm">
            <X class="w-3.5 h-3.5" /> {i18n.t('settings.cancel')}
          </Button>
          <Button onclick={saveManualLyrics} variant="primary" size="sm">
            <Save class="w-3.5 h-3.5" /> {i18n.t('tagEditor.saveBtnShort')}
          </Button>
        {/if}
      </div>
    {/if}
  </div>

  <!-- Lyrics Container Viewport -->
  <div class="flex-1 overflow-y-auto px-6 py-12" class:pb-28={!!playerStore.currentSong} bind:this={containerEl} use:rememberScroll={"lyrics"}>
    {#if isLoading}
      <div class="w-full h-full flex flex-col items-center justify-center gap-3">
        <LoadingSpinner label={i18n.t('lyrics.fetching', {}, "Fetching lyrics...")} />
      </div>
    {:else if playerStore.currentSong?.is_instrumental}
      <!-- Instrumental View -->
      <div class="w-full h-full flex flex-col items-center justify-center gap-4 p-8 text-center">
        <div class="p-4 rounded-full bg-brand-sidebar border border-brand-border text-brand-accent shadow-inner">
          <Music2 class="w-12 h-12 stroke-[1.5]" />
        </div>
        <div class="space-y-1 max-w-sm">
          <h3 class="text-base font-bold text-brand-text-primary">
            {i18n.t('lyrics.instrumentalTitle', {}, "Instrumental Track")}
          </h3>
          <p class="text-xs text-brand-text-secondary/70">
            {i18n.t('lyrics.instrumentalDesc', {}, "This track is marked as instrumental. Online lyrics search is bypassed.")}
          </p>
        </div>
        <Button onclick={() => toggleInstrumental(false)} variant="secondary" size="sm" class="mt-2">
          {i18n.t('lyrics.unmarkInstrumental', {}, "Unmark Instrumental")}
        </Button>
      </div>
    {:else if isEditing}
      <!-- Editor Mode -->
      <div class="max-w-2xl mx-auto h-full flex flex-col gap-3">
        <label for="lyrics-editor" class="text-xs font-bold text-brand-text-secondary/65 uppercase tracking-wider">{i18n.t('lyrics.editorLabel', {}, "Lyrics Text (plain or LRC synced format)")}</label>
        <textarea
          id="lyrics-editor"
          bind:value={editText}
          class="flex-1 bg-brand-sidebar border border-brand-border rounded-xl p-4 text-sm font-mono text-brand-text-primary outline-none focus:border-brand-accent resize-none h-[calc(100vh-280px)] focus:ring-1 focus:ring-brand-accent"
          placeholder={i18n.t('lyrics.editorPlaceholder', {}, "Paste synced LRC or plain text lyrics here...")}
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
                class="text-xl md:text-2xl font-bold cursor-pointer transition-all duration-300 transform {isActive ? 'text-brand-text-primary scale-105 filter drop-shadow-[0_0_8px_var(--color-brand-accent)] font-extrabold' : 'text-brand-text-secondary/30 hover:text-brand-text-secondary/60'}"
              >
                {line.text || "•••"}
              </p>
            {/each}
          </div>
        {:else}
          <!-- Plain Text View -->
          <div class="mb-8 inline-flex items-center gap-2 px-3 py-1.5 rounded-full border border-brand-border bg-brand-sidebar text-[11px] font-semibold text-brand-text-secondary/60 select-none shadow-sm">
            <span class="w-1.5 h-1.5 rounded-full bg-amber-500 animate-pulse"></span>
            {i18n.t('lyrics.plainTextNotice', {}, "Synced lyrics not available. Showing plain text.")}
          </div>
          <div class="whitespace-pre-line text-lg leading-relaxed text-brand-text-secondary/80 select-text pb-20 font-medium font-sans">
            {lyricsText.startsWith("[synced:false]\n")
              ? lyricsText.substring("[synced:false]\n".length)
              : (lyricsText.startsWith("[synced:false]") ? lyricsText.substring("[synced:false]".length) : lyricsText)}
          </div>
        {/if}
      </div>
    {:else if errorMsg}
      <div class="w-full h-full flex flex-col items-center justify-center gap-3 p-8 text-center">
        <p class="text-sm font-semibold text-rose-400">{i18n.t('lyrics.lyricsNotFound')}</p>
        <p class="text-xs text-brand-text-secondary/50 max-w-sm">{errorMsg}</p>
        <div class="flex items-center gap-2 mt-2">
          <Button onclick={() => loadLyrics(playerStore.currentSong?.id)} variant="secondary" size="sm">
            {i18n.t('lyrics.retrySearch', {}, "Retry Search")}
          </Button>
          {#if playerStore.currentSong}
            <Button onclick={() => toggleInstrumental(true)} variant="accent-soft" size="sm">
              {i18n.t('lyrics.markInstrumental', {}, "Mark as Instrumental")}
            </Button>
          {/if}
        </div>
      </div>
    {:else}
      <div class="w-full h-full flex flex-col items-center justify-center gap-2 text-center text-brand-text-secondary/50">
        <FileText class="w-12 h-12 stroke-[1] text-brand-text-secondary/30 mb-2" />
        {#if playerStore.currentSong}
          <p class="text-sm font-semibold text-brand-text-secondary/80">{i18n.t('lyrics.lyricsNotFound')}</p>
          <p class="text-xs text-brand-text-secondary/50 max-w-xs mt-1">{i18n.t('lyrics.lyricsHelpText')}</p>
          <Button onclick={() => toggleInstrumental(true)} variant="accent-soft" size="sm" class="mt-3">
            {i18n.t('lyrics.markInstrumental', {}, "Mark as Instrumental")}
          </Button>
        {:else}
          <p class="text-sm font-semibold text-brand-text-secondary/80">{i18n.t('playerBar.notPlaying')}</p>
          <p class="text-xs text-brand-text-secondary/50 mt-1">{i18n.t('lyrics.lyricsHelpText')}</p>
        {/if}
      </div>
    {/if}
  </div>
</div>
