<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { playerStore } from "../stores/player.svelte";
  import { collectionStore } from "../stores/collection.svelte";
  import { windowLayoutStore } from "../stores/windowLayout.svelte";
  import { themeStore } from "../stores/theme.svelte";
  import { i18n } from "../stores/i18n.svelte";
  import { formatDuration } from "../utils/formatters";
  import CoverArt from "./CoverArt.svelte";
  import WaveformSeekBar from "./WaveformSeekBar.svelte";
  import SongRating from "./SongRating.svelte";
  import {
    PlayIcon as Play,
    PauseIcon as Pause,
    SkipBackIcon as SkipBack,
    SkipForwardIcon as SkipForward,
    ShuffleIcon as Shuffle,
    RepeatIcon as Repeat,
    CornersOutIcon as Maximize2,
    SpeakerHighIcon as Volume2,
    SpeakerSimpleSlashIcon as VolumeX,
    DiscIcon as DiscAlbum,
    MicrophoneStageIcon as Mic2,
    PlaylistIcon as ListMusic,
    MusicNotesIcon as Music,
    SubtitlesIcon as Lyrics,
    CheckCircleIcon as CheckCircle,
    ArrowCounterClockwiseIcon as RotateCcw
  } from "phosphor-svelte";
  import LoadingSpinner from "./LoadingSpinner.svelte";
  import { parseLrc } from "../utils/lrc";

  let isSessionCompleted = $derived(!playerStore.currentSong && playerStore.completedSession !== null);
  let coverWidth = $state(0);

  async function handleShuffleLibrary() {
    let songs = collectionStore.songs;
    if (songs.length === 0) {
      await collectionStore.refreshLibrary();
      songs = collectionStore.songs;
    }
    if (songs.length > 0) {
      await playerStore.shuffleLibrary(songs);
    }
  }

  async function handleReplay() {
    await playerStore.replayCompletedSession();
  }

  // Volume slider gradient style (mirrors PlayerBar.svelte's volume control)
  let volumePercent = $derived(playerStore.volume * 100);
  let volumeSliderStyle = $derived(
    `background: linear-gradient(to right, var(--color-accent) 0%, var(--color-accent) ${volumePercent}%, var(--color-border) ${volumePercent}%, var(--color-border) 100%)`
  );

  function handleVolumeChange(e: Event) {
    const input = e.target as HTMLInputElement;
    playerStore.setVolume(parseFloat(input.value));
  }

  function releaseVolumeFocus(e: Event) {
    (e.currentTarget as HTMLInputElement).blur();
  }

  let isMuted = $state(false);
  let previousVolume = $state(1.0);

  function toggleMute() {
    if (isMuted) {
      playerStore.setVolume(previousVolume);
      isMuted = false;
    } else {
      previousVolume = playerStore.volume;
      playerStore.setVolume(0.0);
      isMuted = true;
    }
  }

  function cycleShuffle() {
    const modes: import("../types").ShuffleMode[] = ["off", "all", "inside_album", "albums", "artists"];
    const currentIdx = modes.indexOf(playerStore.shuffleMode);
    const nextIdx = (currentIdx + 1) % modes.length;
    playerStore.setShuffleMode(modes[nextIdx]);
  }

  function cycleRepeat() {
    const modes: import("../types").RepeatMode[] = ["off", "track", "album", "playlist"];
    const currentIdx = modes.indexOf(playerStore.repeatMode);
    const nextIdx = (currentIdx + 1) % modes.length;
    playerStore.setRepeatMode(modes[nextIdx]);
  }

  // A mode-type icon paired alongside the Shuffle/Repeat transport icon to
  // disambiguate modes that would otherwise share the same base icon (e.g.
  // "Shuffle Albums" and "Shuffle Inside Album" both pair with DiscAlbum) —
  // rendered full-size next to the icon rather than as a tiny overlay badge,
  // so it stays legible. Reuses the same Phosphor icons as the rest of the app.
  function shuffleTypeIcon(mode: import("../types").ShuffleMode) {
    switch (mode) {
      case "inside_album": return DiscAlbum;
      case "albums": return DiscAlbum;
      case "artists": return Mic2;
      default: return null;
    }
  }

  function repeatTypeIcon(mode: import("../types").RepeatMode) {
    switch (mode) {
      case "track": return Music;
      case "album": return DiscAlbum;
      case "playlist": return ListMusic;
      default: return null;
    }
  }

  function shuffleModeLabel(mode: import("../types").ShuffleMode): string {
    switch (mode) {
      case "off": return i18n.t('playerBar.shuffleOff');
      case "all": return i18n.t('playerBar.shuffleAll');
      case "inside_album": return i18n.t('playerBar.shuffleInsideAlbum');
      case "albums": return i18n.t('playerBar.shuffleAlbums');
      case "artists": return i18n.t('playerBar.shuffleArtists');
    }
  }

  function repeatModeLabel(mode: import("../types").RepeatMode): string {
    switch (mode) {
      case "off": return i18n.t('playerBar.repeatOff');
      case "track": return i18n.t('playerBar.repeatSong');
      case "album": return i18n.t('playerBar.repeatAlbum');
      case "playlist": return i18n.t('playerBar.repeatPlaylist');
      default: return mode;
    }
  }

  // Mirrors the mode descriptions from the user guide so the tooltip explains
  // what the mode does, not just its name.
  function shuffleModeDescription(mode: import("../types").ShuffleMode): string {
    switch (mode) {
      case "off": return i18n.t('playerBar.shuffleOffDesc');
      case "all": return i18n.t('playerBar.shuffleAllDesc');
      case "inside_album": return i18n.t('playerBar.shuffleInsideAlbumDesc');
      case "albums": return i18n.t('playerBar.shuffleAlbumsDesc');
      case "artists": return i18n.t('playerBar.shuffleArtistsDesc');
    }
  }

  function repeatModeDescription(mode: import("../types").RepeatMode): string {
    switch (mode) {
      case "off": return i18n.t('playerBar.repeatOffDesc');
      case "track": return i18n.t('playerBar.repeatSongDesc');
      case "album": return i18n.t('playerBar.repeatAlbumDesc');
      case "playlist": return i18n.t('playerBar.repeatPlaylistDesc');
      default: return "";
    }
  }

  // backdrop-filter fallback for webviews without GPU compositing — see
  // ThemeStore.gpuCompositing.
  let noBackdrop = $derived(themeStore.gpuCompositing === false);

  function handleStartDrag(e: PointerEvent) {
    invoke("start_window_drag").catch(() => {});
  }

  function handleStartResize(direction: string, e: PointerEvent) {
    e.stopPropagation();
    invoke("start_window_resize", { direction }).catch(() => {});
  }

  function handleKeyDown(e: KeyboardEvent) {
    // Ctrl/Cmd+M is handled globally by +layout.svelte's toggleMiniplayerMode
    // listener. Handling it here too would double-fire on every press (this
    // handler exits, then the still-bubbling event reaches the global one,
    // which sees the just-cleared isMiniplayer flag and re-enters) — so only
    // Escape, which has no global handler, belongs here.
    if (e.key === "Escape") {
      e.preventDefault();
      windowLayoutStore.exitMiniplayerMode();
    }
  }

  let isHovered = $state(false);

  function showHover(e?: MouseEvent | PointerEvent) {
    if (e) {
      const margin = 4;
      if (
        e.clientX <= margin ||
        e.clientY <= margin ||
        e.clientX >= window.innerWidth - margin ||
        e.clientY >= window.innerHeight - margin
      ) {
        hideHover();
        return;
      }
    }

    isHovered = true;
  }

  function hideHover() {
    isHovered = false;
  }

  $effect(() => {
    const handleBlur = () => hideHover();
    const handleMouseLeave = () => hideHover();
    const handleMouseOut = (e: MouseEvent) => {
      if (!e.relatedTarget) {
        hideHover();
      }
    };

    window.addEventListener("blur", handleBlur);
    document.addEventListener("mouseleave", handleMouseLeave);
    window.addEventListener("mouseout", handleMouseOut);

    return () => {
      window.removeEventListener("blur", handleBlur);
      document.removeEventListener("mouseleave", handleMouseLeave);
      window.removeEventListener("mouseout", handleMouseOut);
    };
  });

  // View toggle between album artwork and live lyrics
  let showLyrics = $state<boolean>(
    typeof window !== "undefined" && localStorage.getItem("miniplayer_show_lyrics") === "true"
  );

  function toggleLyrics() {
    showLyrics = !showLyrics;
    if (typeof window !== "undefined") {
      localStorage.setItem("miniplayer_show_lyrics", showLyrics ? "true" : "false");
    }
  }

  let lyricsText = $state("");
  let userOffsetMs = $state(0);
  let isLoading = $state(false);
  let lyricsContainerEl = $state<HTMLDivElement | null>(null);

  let parsed = $derived(parseLrc(lyricsText, userOffsetMs));
  let parsedLines = $derived(parsed.lines);
  let isSynced = $derived(parsedLines.length > 0);
  let currentMs = $derived(playerStore.positionNanosec / 1_000_000);

  let activeLineIndex = $derived.by(() => {
    if (!isSynced || parsedLines.length === 0) return -1;
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

  async function loadLyrics(songId: number | undefined) {
    if (songId === undefined || playerStore.currentSong?.is_instrumental) {
      lyricsText = "";
      return;
    }

    if (playerStore.currentSong?.lyrics) {
      lyricsText = playerStore.currentSong.lyrics;
      return;
    }

    lyricsText = "";
    isLoading = true;
    try {
      const lyrics = await invoke<string>("get_lyrics", { songId, forceRefresh: false });
      if (playerStore.currentSong?.id === songId) {
        lyricsText = lyrics || "";
        playerStore.currentSong.lyrics = lyrics;
      }
    } catch {
      if (playerStore.currentSong?.id === songId) {
        lyricsText = "";
      }
    } finally {
      isLoading = false;
    }
  }

  async function loadOffset(songId: number | undefined) {
    userOffsetMs = 0;
    if (songId === undefined) return;
    try {
      const offset = await invoke<number>("get_lyrics_offset", { songId });
      if (playerStore.currentSong?.id === songId) {
        userOffsetMs = offset;
      }
    } catch {
      userOffsetMs = 0;
    }
  }

  $effect(() => {
    const songId = playerStore.currentSong?.id;
    if (songId !== undefined && showLyrics) {
      loadLyrics(songId);
      loadOffset(songId);
    } else if (songId === undefined) {
      lyricsText = "";
      userOffsetMs = 0;
    }
  });

  // Keep lyricsText in sync if currentSong.lyrics is updated externally while miniplayer is open
  $effect(() => {
    if (playerStore.currentSong?.lyrics && playerStore.currentSong.lyrics !== lyricsText) {
      lyricsText = playerStore.currentSong.lyrics;
    }
  });

  let didInitialScroll = false;
  $effect(() => {
    playerStore.currentSong?.id;
    didInitialScroll = false;
  });

  $effect(() => {
    if (showLyrics && activeLineIndex !== -1 && lyricsContainerEl) {
      const activeEl = lyricsContainerEl.querySelector(`[data-index="${activeLineIndex}"]`);
      if (activeEl) {
        const behavior = didInitialScroll ? "smooth" : "auto";
        activeEl.scrollIntoView({ behavior, block: "center" });
        didInitialScroll = true;
      }
    }
  });

</script>

<!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
<!-- svelte-ignore a11y_no_noninteractive_tabindex -->
<div
  role="group"
  aria-label={i18n.t('miniplayer.title')}
  onkeydown={handleKeyDown}
  onpointerenter={showHover}
  onpointermove={showHover}
  onpointerleave={hideHover}
  onmouseleave={hideHover}
  tabindex="0"
  class="group relative w-full h-full flex flex-col justify-between overflow-hidden bg-brand-main select-none p-3 shadow-2xl {themeStore.isGlassTheme ? 'glass-surface' : ''} {noBackdrop ? 'no-backdrop' : ''}"
>
  <!-- Edge and Corner Resize Handles for Frameless Window -->
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div class="absolute top-0 left-0 right-0 h-2 cursor-n-resize z-50" onpointerdown={(e) => handleStartResize("north", e)}></div>
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div class="absolute bottom-0 left-0 right-0 h-2 cursor-s-resize z-50" onpointerdown={(e) => handleStartResize("south", e)}></div>
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div class="absolute top-0 bottom-0 left-0 w-2 cursor-w-resize z-50" onpointerdown={(e) => handleStartResize("west", e)}></div>
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div class="absolute top-0 bottom-0 right-0 w-2 cursor-e-resize z-50" onpointerdown={(e) => handleStartResize("east", e)}></div>
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div class="absolute top-0 left-0 w-4 h-4 cursor-nw-resize z-50" onpointerdown={(e) => handleStartResize("north-west", e)}></div>
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div class="absolute top-0 right-0 w-4 h-4 cursor-ne-resize z-50" onpointerdown={(e) => handleStartResize("north-east", e)}></div>
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div class="absolute bottom-0 left-0 w-4 h-4 cursor-sw-resize z-50" onpointerdown={(e) => handleStartResize("south-west", e)}></div>
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div class="absolute bottom-0 right-0 w-4 h-4 cursor-se-resize z-50" onpointerdown={(e) => handleStartResize("south-east", e)}></div>
  {#if playerStore.currentSong}
    <div
      class="absolute inset-0 z-0 opacity-25 blur-2xl pointer-events-none"
      style="will-change: filter; transform: translateZ(0) scale(1.25);"
    >
      <CoverArt
        songId={playerStore.currentSong?.id}
        artEmbedded={playerStore.currentSong?.art_embedded}
        artAutomatic={playerStore.currentSong?.art_automatic}
        artManual={playerStore.currentSong?.art_manual}
        sizeClass="w-full h-full object-cover"
      />
    </div>
  {/if}

  {#if isSessionCompleted}
    {@const session = playerStore.completedSession}
    <!-- SESSION WRAP COMPLETION SCREEN (#1379) -->
    <div class="relative z-10 w-full h-full flex flex-col items-center justify-between pointer-events-auto">
      <!-- Drag handle at top -->
      <div
        data-tauri-drag-region
        onpointerdown={handleStartDrag}
        role="button"
        tabindex="0"
        aria-label={i18n.t('miniplayer.dragHint', {}, 'Drag window')}
        class="drag-grabber w-full h-4 flex-shrink-0 cursor-grab active:cursor-grabbing text-brand-text-secondary/40 hover:text-brand-text-secondary/80 transition-colors"
        title={i18n.t('miniplayer.dragHint', {}, 'Drag window')}
      ></div>

      <!-- Center: Celebration Badge & Action Pills -->
      <div class="flex-1 w-full flex flex-col items-center justify-center my-auto px-2 min-h-0">
        <!-- Milestone Gold Ring & Checkmark Pop -->
        <div class="relative mb-3 flex items-center justify-center">
          <div class="w-14 h-14 rounded-full bg-brand-gold/15 border border-brand-gold/40 flex items-center justify-center text-brand-gold shadow-[0_0_20px_rgba(245,158,11,0.2)] anim-gold-ring">
            <CheckCircle class="w-8 h-8 anim-check-pop" weight="fill" />
          </div>
        </div>

        <!-- Completion Title -->
        <h2 class="text-sm font-bold text-brand-text-primary tracking-tight px-2 truncate w-full text-center">
          {session?.isQueue
            ? i18n.t('miniplayer.queueComplete')
            : i18n.t('miniplayer.contextComplete', { name: session?.contextName })}
        </h2>

        <!-- Subtitle detail (tracks played) -->
        <p class="text-xs text-brand-text-secondary/70 mt-0.5 text-center truncate max-w-full px-2">
          {#if (session?.trackCount ?? 0) >= 1}
            {i18n.plural("miniplayer.tracksPlayed", session?.trackCount ?? 0)}
          {:else}
            {i18n.t('celebrations.queueComplete')}
          {/if}
        </p>

        <!-- Continuation Action Buttons -->
        <div class="flex flex-col gap-1.5 w-full max-w-[210px] mt-3.5">
          <button
            onclick={handleShuffleLibrary}
            class="w-full py-1.5 px-3 rounded-lg bg-brand-accent hover:bg-brand-accent-hover text-brand-accent-contrast font-semibold text-xs flex items-center justify-center gap-2 transition-colors shadow-sm cursor-pointer"
            title={i18n.t('miniplayer.shuffleLibrary')}
          >
            <Shuffle class="w-3.5 h-3.5" />
            <span>{i18n.t('miniplayer.shuffleLibrary')}</span>
          </button>

          <div class="grid grid-cols-2 gap-1.5 w-full">
            {#if (session?.songIds.length ?? 0) > 0}
              <button
                onclick={handleReplay}
                class="py-1 px-2 rounded-lg bg-brand-sidebar hover:bg-brand-sidebar/80 text-brand-text-primary font-medium text-[11px] flex items-center justify-center gap-1.5 transition-colors border border-brand-border/40 cursor-pointer"
                title={i18n.t('miniplayer.replay')}
              >
                <RotateCcw class="w-3 h-3" />
                <span>{i18n.t('miniplayer.replay')}</span>
              </button>
            {/if}

            <button
              onclick={() => windowLayoutStore.exitMiniplayerMode()}
              class="py-1 px-2 rounded-lg bg-brand-sidebar hover:bg-brand-sidebar/80 text-brand-text-primary font-medium text-[11px] flex items-center justify-center gap-1.5 transition-colors border border-brand-border/40 cursor-pointer {(session?.songIds.length ?? 0) === 0 ? 'col-span-2' : ''}"
              title={i18n.t('miniplayer.exit')}
            >
              <Maximize2 class="w-3 h-3" />
              <span>{i18n.t('miniplayer.library')}</span>
            </button>
          </div>
        </div>

        <!-- Drop hint -->
        <span class="text-[10px] text-brand-text-secondary/40 mt-3 select-none">
          {i18n.t('miniplayer.dropToPlay')}
        </span>
      </div>

      <!-- Bottom mini window utilities -->
      <div class="w-full flex items-center justify-between pt-1 border-t border-brand-border/20 text-brand-text-secondary/50 text-xs flex-shrink-0">
        <div class="flex items-center gap-1.5">
          <button
            onclick={toggleMute}
            class="p-1 hover:text-brand-text-primary transition-colors cursor-pointer"
            title={i18n.t('playerBar.volume')}
          >
            {#if isMuted || playerStore.volume === 0}
              <VolumeX class="w-3.5 h-3.5" />
            {:else}
              <Volume2 class="w-3.5 h-3.5" />
            {/if}
          </button>
          <input
            type="range"
            min="0"
            max="1"
            step="0.01"
            value={playerStore.volume}
            oninput={handleVolumeChange}
            onchange={releaseVolumeFocus}
            onpointerup={releaseVolumeFocus}
            onkeyup={releaseVolumeFocus}
            class="volume-slider w-14 h-1 rounded-lg outline-none"
            style={volumeSliderStyle}
            aria-label={i18n.t('playerBar.volumeSlider')}
            title={i18n.t('playerBar.volumeWithValue', { value: Math.round(volumePercent) })}
          />
        </div>
        <button
          onclick={() => windowLayoutStore.exitMiniplayerMode()}
          class="p-1 hover:text-brand-text-primary transition-colors cursor-pointer"
          title={i18n.t('miniplayer.exit')}
        >
          <Maximize2 class="w-3.5 h-3.5" />
        </button>
      </div>
    </div>
  {:else}
    <!-- IDLE STATIC LAYOUT -->
    <div class="relative z-10 w-full h-full flex flex-col items-center justify-between pointer-events-auto">
      <div class="flex-1 w-full flex items-center justify-center min-h-0 py-2 overflow-hidden">
        {#if showLyrics}
          <div
            bind:this={lyricsContainerEl}
            class="lyrics-container w-full h-full overflow-y-auto px-3 py-6 flex flex-col items-center text-center select-none"
          >
            {#if isLoading}
              <div class="flex-1 flex flex-col items-center justify-center gap-2 text-brand-text-secondary/60">
                <LoadingSpinner label={i18n.t('lyrics.fetching', {}, 'Fetching lyrics...')} />
              </div>
            {:else if playerStore.currentSong?.is_instrumental}
              <div class="flex-1 flex flex-col items-center justify-center gap-2 p-4 text-center">
                <div class="p-3 rounded-full bg-brand-sidebar/70 border border-brand-border/40 text-brand-accent">
                  <Music class="w-6 h-6" />
                </div>
                <p class="text-xs font-semibold text-brand-text-primary">
                  {i18n.t('lyrics.instrumentalTitle', {}, 'Instrumental Track')}
                </p>
                <p class="text-[11px] text-brand-text-secondary/70 max-w-xs">
                  {i18n.t('lyrics.instrumentalDesc', {}, 'This track is marked as instrumental. Online lyrics search is bypassed.')}
                </p>
              </div>
            {:else if isSynced}
              <div class="flex flex-col gap-4 py-16 w-full">
                {#each parsedLines as line, idx}
                  {@const isActive = idx === activeLineIndex}
                  <!-- svelte-ignore a11y_click_events_have_key_events -->
                  <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
                  <p
                    data-index={idx}
                    dir="auto"
                    onclick={() => playerStore.seek(line.timeMs * 1_000_000)}
                    class="text-sm font-bold transition-all duration-300 transform px-2 text-balance leading-relaxed cursor-pointer {isActive ? 'text-brand-text-primary scale-105 filter drop-shadow-[0_0_8px_var(--color-brand-accent)] font-extrabold' : 'text-brand-text-secondary/35 hover:text-brand-text-secondary/60'}"
                  >
                    {#if isActive && line.words && line.words.length > 0}
                      {#each line.words as word}
                        {@const isWordSung = currentMs >= word.timeMs}
                        <span
                          class="inline-block whitespace-pre-wrap transition-all duration-150 {isWordSung ? 'text-brand-text-primary opacity-100 filter drop-shadow-[0_0_6px_var(--color-brand-accent)]' : 'text-brand-text-primary/40 opacity-40'}"
                        >{word.text}</span>
                      {/each}
                    {:else}
                      {line.text || "•••"}
                    {/if}
                  </p>
                {/each}
              </div>
            {:else}
              <div class="flex-1 flex flex-col items-center justify-center gap-2 p-4 text-center">
                <div class="p-3 rounded-full bg-brand-sidebar/50 border border-brand-border/30 text-brand-text-secondary/50">
                  <Lyrics class="w-6 h-6 stroke-[1.5]" />
                </div>
                <p class="text-xs font-medium text-brand-text-secondary/70">
                  {i18n.t('miniplayer.noLiveLyrics')}
                </p>
              </div>
            {/if}
          </div>
        {:else}
          <div
            bind:clientWidth={coverWidth}
            class="relative aspect-square h-full max-h-full max-w-[90%] rounded-none overflow-hidden border border-brand-border/30 bg-brand-sidebar flex items-center justify-center {isHovered ? 'scale-[0.98]' : ''} transition-transform duration-300"
          >
            <CoverArt
              songId={playerStore.currentSong?.id}
              artEmbedded={playerStore.currentSong?.art_embedded}
              artAutomatic={playerStore.currentSong?.art_automatic}
              artManual={playerStore.currentSong?.art_manual}
              sizeClass="w-full h-full object-cover"
            />
          </div>
        {/if}
      </div>

      <div
        class="relative w-full max-w-[90%] text-center py-1 flex flex-col items-center justify-center flex-shrink-0"
        style={coverWidth > 0 ? `width: ${coverWidth}px` : undefined}
      >
        <span class="text-sm font-bold text-brand-text-primary truncate w-full px-2" title={playerStore.currentSong?.title}>
          {playerStore.currentSongDisplayTitle}
        </span>
        <div class="relative w-full flex items-center justify-center mt-0.5">
          <span class="text-xs text-brand-text-secondary/70 truncate w-full px-8" title={playerStore.currentSong?.artist}>
            {playerStore.currentSong?.artist || (playerStore.currentSong ? i18n.t('collection.unknownArtist') : '')}
          </span>
          {#if playerStore.queueProgressText}
            <span
              dir="ltr"
              class="absolute right-0 top-1/2 -translate-y-1/2 text-[10px] font-mono tabular-nums text-brand-text-secondary/50 pointer-events-none select-none tracking-tight leading-none"
              aria-label={i18n.t('miniplayer.queueProgressAria', { current: playerStore.playlistIndex, total: playerStore.playlistTotal }, `Track ${playerStore.playlistIndex} of ${playerStore.playlistTotal}`)}
              title={i18n.t('miniplayer.queueProgressAria', { current: playerStore.playlistIndex, total: playerStore.playlistTotal }, `Track ${playerStore.playlistIndex} of ${playerStore.playlistTotal}`)}
            >
              {playerStore.queueProgressText}
            </span>
          {/if}
        </div>
      </div>
    </div>
  {/if}

  {#if !isSessionCompleted}
    <!-- FOCUSED HOVER CONTROL MASK (Revealed on mouse hover) -->
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <div
      onpointerenter={showHover}
      onpointermove={showHover}
    onpointerleave={hideHover}
    class="absolute inset-0 z-30 flex flex-col justify-between p-3 transition-opacity duration-200 {isHovered ? 'opacity-100 pointer-events-auto' : 'opacity-0 pointer-events-none'} {noBackdrop ? 'bg-brand-main' : 'bg-brand-main/85 backdrop-blur-md'}"
  >
    <div
      data-tauri-drag-region
      onpointerdown={handleStartDrag}
      role="button"
      tabindex="0"
      aria-label={i18n.t('miniplayer.dragHint', {}, 'Drag window')}
      class="drag-grabber w-full h-5 flex-shrink-0 z-40 cursor-grab active:cursor-grabbing text-brand-text-secondary/40 hover:text-brand-text-secondary/80 transition-colors"
      title={i18n.t('miniplayer.dragHint', {}, 'Drag window')}
    ></div>

    <div class="w-full text-center px-1 py-0.5 flex flex-col items-center justify-center flex-shrink-0 mt-auto">
      <span class="text-sm font-bold text-brand-text-primary truncate w-full" title={playerStore.currentSong?.title}>
        {playerStore.currentSongDisplayTitle}
      </span>
      <span class="text-xs text-brand-text-secondary/80 truncate w-full" title={playerStore.currentSong?.artist}>
        {playerStore.currentSong?.artist || (playerStore.currentSong ? i18n.t('collection.unknownArtist') : '')}
      </span>
      {#if playerStore.currentSong}
        <div class="mt-1.5">
          <SongRating
            rating={playerStore.currentSong.rating}
            loved={playerStore.currentSong.loved}
            onRate={(r) => playerStore.rateCurrent(r)}
            onSetLoved={(l) => playerStore.setLovedCurrent(l)}
            size="md"
          />
        </div>
      {/if}
    </div>

    <div class="flex flex-col items-center justify-center w-full gap-2 my-auto flex-shrink-0">
      <div class="flex items-center justify-center gap-4 w-full">
        <button
          onclick={cycleShuffle}
          class="p-1.5 transition-colors hover:text-brand-text-primary flex items-center gap-1 {playerStore.shuffleMode !== 'off' ? 'text-brand-accent-text font-bold' : 'text-brand-text-secondary/60'}"
          title={`${i18n.t('playerBar.shuffle')}: ${shuffleModeLabel(playerStore.shuffleMode)} — ${shuffleModeDescription(playerStore.shuffleMode)}`}
        >
          {#if shuffleTypeIcon(playerStore.shuffleMode)}
            {@const ShuffleTypeIcon = shuffleTypeIcon(playerStore.shuffleMode)}
            <ShuffleTypeIcon class="w-4.5 h-4.5" />
          {/if}
          <Shuffle class="w-4.5 h-4.5" />
        </button>

        <button
          onclick={() => playerStore.previous()}
          class="p-1.5 text-brand-text-secondary hover:text-brand-text-primary transition-colors"
          title={i18n.t('playerBar.previous')}
        >
          <SkipBack class="w-5 h-5 fill-current" />
        </button>

        {#if playerStore.state === 'playing'}
          <button
            onclick={() => playerStore.pause()}
            class="w-10 h-10 rounded-full bg-brand-accent hover:bg-brand-accent-hover text-brand-accent-contrast flex items-center justify-center transition-colors flex-shrink-0"
            title={i18n.t('playerBar.pause')}
          >
            <Pause class="w-5 h-5 fill-current" />
          </button>
        {:else}
          <button
            onclick={() => playerStore.resume()}
            class="w-10 h-10 rounded-full bg-brand-accent hover:bg-brand-accent-hover text-brand-accent-contrast flex items-center justify-center transition-colors flex-shrink-0"
            title={i18n.t('playerBar.play')}
          >
            <Play class="w-5 h-5 fill-current" />
          </button>
        {/if}

        <button
          onclick={() => playerStore.next()}
          class="p-1.5 text-brand-text-secondary hover:text-brand-text-primary transition-colors"
          title={i18n.t('playerBar.next')}
        >
          <SkipForward class="w-5 h-5 fill-current" />
        </button>

        <button
          onclick={cycleRepeat}
          class="p-1.5 transition-colors hover:text-brand-text-primary flex items-center gap-1 {playerStore.repeatMode !== 'off' ? 'text-brand-accent-text font-bold' : 'text-brand-text-secondary/60'}"
          title={`${i18n.t('playerBar.repeat')}: ${repeatModeLabel(playerStore.repeatMode)} — ${repeatModeDescription(playerStore.repeatMode)}`}
        >
          <Repeat class="w-4.5 h-4.5" />
          {#if repeatTypeIcon(playerStore.repeatMode)}
            {@const RepeatTypeIcon = repeatTypeIcon(playerStore.repeatMode)}
            <RepeatTypeIcon class="w-4.5 h-4.5" />
          {/if}
        </button>
      </div>

      <div class="flex flex-col gap-1 w-full text-[10px] text-brand-text-secondary/70 px-1">
        <WaveformSeekBar />
        <div class="flex items-center justify-between w-full px-0.5 font-mono text-[9px] opacity-80">
          <span>{formatDuration(playerStore.positionNanosec)}</span>
          <span>{formatDuration(playerStore.currentSong?.length_nanosec)}</span>
        </div>
      </div>
    </div>

    <div class="flex items-center justify-between w-full flex-shrink-0 z-50">
      <div class="flex items-center gap-1.5">
        <button
          onclick={toggleMute}
          class="p-1 text-brand-text-secondary/70 hover:text-brand-text-primary transition-colors"
          title={i18n.t('playerBar.volume')}
        >
          {#if isMuted || playerStore.volume === 0}
            <VolumeX class="w-3.5 h-3.5" />
          {:else}
            <Volume2 class="w-3.5 h-3.5" />
          {/if}
        </button>
        <input
          type="range"
          min="0"
          max="1"
          step="0.01"
          value={playerStore.volume}
          oninput={handleVolumeChange}
          onchange={releaseVolumeFocus}
          onpointerup={releaseVolumeFocus}
          onkeyup={releaseVolumeFocus}
          class="volume-slider w-14 h-1 rounded-lg outline-none"
          style={volumeSliderStyle}
          aria-label={i18n.t('playerBar.volumeSlider')}
          title={i18n.t('playerBar.volumeWithValue', { value: Math.round(volumePercent) })}
        />
      </div>

      <div class="flex items-center gap-1">
        <button
          onclick={toggleLyrics}
          class="p-1 rounded transition-colors {showLyrics ? 'text-brand-accent-text hover:text-brand-accent-text bg-brand-border/40 font-bold' : 'text-brand-text-secondary hover:text-brand-text-primary hover:bg-brand-border/40'}"
          title={showLyrics ? i18n.t('miniplayer.showCoverArt') : i18n.t('miniplayer.showLyrics')}
          aria-label={showLyrics ? i18n.t('miniplayer.showCoverArt') : i18n.t('miniplayer.showLyrics')}
        >
          <Lyrics class="w-3.5 h-3.5" />
        </button>

        <button
          onclick={() => windowLayoutStore.exitMiniplayerMode()}
          class="p-1 text-brand-text-secondary hover:text-brand-text-primary hover:bg-brand-border/40 rounded transition-colors"
          title={i18n.t('miniplayer.exit', {}, 'Restore Full Window (Ctrl+M)')}
        >
          <Maximize2 class="w-3.5 h-3.5" />
        </button>
      </div>
    </div>
  </div>
  {/if}
</div>

<style>
  :global(.glass-surface) {
    -webkit-backdrop-filter: blur(20px) saturate(180%);
    backdrop-filter: blur(20px) saturate(180%);
  }

  /* backdrop-filter doesn't render without GPU compositing (WebKitGTK with
     its GPU rendering disabled by env var), leaving the panel see-through instead of frosted — same
     fallback as PlayerBar's footer.no-backdrop. */
  :global(.glass-surface.no-backdrop) {
    background-color: var(--bg-main, #191918) !important;
    -webkit-backdrop-filter: none !important;
    backdrop-filter: none !important;
  }

  /* Grabber texture: a repeating dot pattern spanning the full drag region,
     rather than a single centered grip icon — signals the whole top edge
     is draggable, not just its midpoint. */
  .drag-grabber {
    background-image: radial-gradient(circle, currentColor 1px, transparent 1.5px);
    background-size: 8px 8px;
    background-position: center;
  }

  .volume-slider {
    -webkit-appearance: none;
    appearance: none;
    transition: background 0.15s ease;
  }

  .volume-slider::-webkit-slider-thumb {
    -webkit-appearance: none;
    appearance: none;
    width: 10px;
    height: 10px;
    border-radius: 50%;
    background: #ffffff;
    border: 2px solid var(--color-accent);
    transition: border-color 0.2s;
  }

  .volume-slider::-webkit-slider-thumb:hover {
    border-color: var(--color-accent-hover);
  }

  .volume-slider::-moz-range-thumb {
    width: 10px;
    height: 10px;
    border: 2px solid var(--color-accent);
    border-radius: 50%;
    background: #ffffff;
    transition: border-color 0.2s;
  }

  .volume-slider::-moz-range-thumb:hover {
    border-color: var(--color-accent-hover);
  }

  .lyrics-container {
    scrollbar-width: none;
    -ms-overflow-style: none;
  }

  .lyrics-container::-webkit-scrollbar {
    display: none;
  }
</style>
