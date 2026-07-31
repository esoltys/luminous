<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { playerStore } from "../stores/player.svelte";
  import { collectionStore } from "../stores/collection.svelte";
  import { themeStore } from "../stores/theme.svelte";
  import { i18n } from "../stores/i18n.svelte";
  import CoverArt from "./CoverArt.svelte";
  import WaveformSeekBar from "./WaveformSeekBar.svelte";
  import SongRating from "./SongRating.svelte";
  import {
    Play,
    Pause,
    SkipBack,
    SkipForward,
    Shuffle,
    Repeat,
    Repeat1,
    Maximize2,
    Volume2,
    VolumeX
  } from "lucide-svelte";

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
    const modes: import("../types").RepeatMode[] = ["off", "track", "album", "playlist", "one_by_one"];
    const currentIdx = modes.indexOf(playerStore.repeatMode);
    const nextIdx = (currentIdx + 1) % modes.length;
    playerStore.setRepeatMode(modes[nextIdx]);
  }

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
      collectionStore.exitMiniplayerMode();
    }
  }

  function formatTime(nanosec: number | undefined): string {
    if (nanosec === undefined) return "0:00";
    const sec = Math.floor(nanosec / 1_000_000_000);
    const m = Math.floor(sec / 60);
    const s = sec % 60;
    return `${m}:${s < 10 ? "0" : ""}${s}`;
  }
</script>

<!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
<!-- svelte-ignore a11y_no_noninteractive_tabindex -->
<div
  role="group"
  aria-label={i18n.t('miniplayer.title')}
  onkeydown={handleKeyDown}
  tabindex="0"
  class="group relative w-full h-full flex flex-col justify-between overflow-hidden bg-brand-main select-none p-3 shadow-2xl {themeStore.isGlassTheme ? 'glass-surface' : ''}"
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
  <!-- Ambient Tint / Cover Art Glow Background -->
  {#if playerStore.currentSong}
    <div class="absolute inset-0 z-0 opacity-25 blur-2xl pointer-events-none scale-125">
      <CoverArt
        songId={playerStore.currentSong?.id}
        artEmbedded={playerStore.currentSong?.art_embedded}
        artAutomatic={playerStore.currentSong?.art_automatic}
        artManual={playerStore.currentSong?.art_manual}
        sizeClass="w-full h-full object-cover"
      />
    </div>
  {/if}

  <!-- IDLE STATIC LAYOUT -->
  <div class="relative z-10 w-full h-full flex flex-col items-center justify-between pointer-events-auto">
    <!-- Centered Sharp Active Album Art Card -->
    <div class="flex-1 w-full flex items-center justify-center min-h-0 py-2">
      <div class="relative aspect-square max-h-full max-w-[240px] rounded-none overflow-hidden shadow-xl border border-brand-border/30 bg-brand-sidebar flex items-center justify-center group-hover:scale-[0.98] transition-transform duration-300">
        <CoverArt
          songId={playerStore.currentSong?.id}
          artEmbedded={playerStore.currentSong?.art_embedded}
          artAutomatic={playerStore.currentSong?.art_automatic}
          artManual={playerStore.currentSong?.art_manual}
          sizeClass="w-full h-full object-cover"
        />
      </div>
    </div>

    <!-- Lower Text Description Card tracking active Title / Artist -->
    <div class="w-full text-center px-2 py-1 flex flex-col items-center justify-center flex-shrink-0">
      <span class="text-sm font-bold text-brand-text-primary truncate w-full" title={playerStore.currentSong?.title}>
        {playerStore.currentSong?.title || i18n.t('playerBar.notPlaying')}
      </span>
      <span class="text-xs text-brand-text-secondary/70 truncate w-full mt-0.5" title={playerStore.currentSong?.artist}>
        {playerStore.currentSong?.artist || (playerStore.currentSong ? i18n.t('collection.unknownArtist') : '')}
      </span>
    </div>
  </div>

  <!-- FOCUSED HOVER CONTROL MASK (Revealed on mouse hover) -->
  <div
    class="absolute inset-0 z-30 bg-brand-main/85 backdrop-blur-md flex flex-col justify-between p-3 opacity-0 group-hover:opacity-100 transition-opacity duration-200 pointer-events-none group-hover:pointer-events-auto"
  >
    <!-- Full-width Window Drag Region spanning the top edge, textured with a grabber dot pattern -->
    <div
      data-tauri-drag-region
      onpointerdown={handleStartDrag}
      role="button"
      tabindex="0"
      aria-label={i18n.t('miniplayer.dragHint', {}, 'Drag window')}
      class="drag-grabber w-full h-5 flex-shrink-0 z-40 cursor-grab active:cursor-grabbing text-brand-text-secondary/40 hover:text-brand-text-secondary/80 transition-colors"
      title={i18n.t('miniplayer.dragHint', {}, 'Drag window')}
    ></div>

    <!-- Song Metadata Info in Hover Mask -->
    <div class="w-full text-center px-1 py-0.5 flex flex-col items-center justify-center flex-shrink-0 mt-auto">
      <span class="text-sm font-bold text-brand-text-primary truncate w-full" title={playerStore.currentSong?.title}>
        {playerStore.currentSong?.title || i18n.t('playerBar.notPlaying')}
      </span>
      <span class="text-xs text-brand-text-secondary/80 truncate w-full" title={playerStore.currentSong?.artist}>
        {playerStore.currentSong?.artist || (playerStore.currentSong ? i18n.t('collection.unknownArtist') : '')}
      </span>
      {#if playerStore.currentSong}
        <div class="mt-1.5">
          <SongRating
            rating={playerStore.currentSong.rating}
            onRate={(r) => playerStore.rateCurrent(r)}
            size="md"
          />
        </div>
      {/if}
    </div>

    <!-- CENTER PLAYBACK CONTROLS & WAVEFORM PROGRESS (Grouped together with tight spacing) -->
    <div class="flex flex-col items-center justify-center w-full gap-2 my-auto flex-shrink-0">
      <!-- Transport Control Ring -->
      <div class="flex items-center justify-center gap-4 w-full">
        <!-- Shuffle Mode -->
        <button
          onclick={cycleShuffle}
          class="p-1.5 transition-colors hover:text-brand-text-primary cursor-pointer relative {playerStore.shuffleMode !== 'off' ? 'text-brand-accent-text font-bold' : 'text-brand-text-secondary/60'}"
          title={`${i18n.t('playerBar.shuffle')}: ${playerStore.shuffleMode}`}
        >
          <Shuffle class="w-4.5 h-4.5" />
          {#if playerStore.shuffleMode !== 'off' && playerStore.shuffleMode !== 'all'}
            <span class="absolute -bottom-1 -right-1 text-[8px] bg-brand-accent text-brand-accent-contrast rounded-full px-0.5 scale-75 font-bold">
              {playerStore.shuffleMode === 'inside_album' ? 'IA' : playerStore.shuffleMode === 'albums' ? 'AL' : 'AR'}
            </span>
          {/if}
        </button>

        <!-- Skip Previous -->
        <button
          onclick={() => playerStore.previous()}
          class="p-1.5 text-brand-text-secondary hover:text-brand-text-primary transition-colors cursor-pointer"
          title={i18n.t('playerBar.previous')}
        >
          <SkipBack class="w-5 h-5 fill-current" />
        </button>

        <!-- Play / Pause prominent ring button -->
        {#if playerStore.state === 'playing'}
          <button
            onclick={() => playerStore.pause()}
            class="w-10 h-10 rounded-full bg-brand-accent hover:bg-brand-accent-hover text-brand-accent-contrast flex items-center justify-center hover:scale-105 transition-transform shadow-lg cursor-pointer flex-shrink-0"
            title={i18n.t('playerBar.pause')}
          >
            <Pause class="w-5 h-5 fill-current" />
          </button>
        {:else}
          <button
            onclick={() => playerStore.resume()}
            class="w-10 h-10 rounded-full bg-brand-accent hover:bg-brand-accent-hover text-brand-accent-contrast flex items-center justify-center hover:scale-105 transition-transform shadow-lg cursor-pointer flex-shrink-0"
            title={i18n.t('playerBar.play')}
          >
            <Play class="w-5 h-5 fill-current ml-0.5" />
          </button>
        {/if}

        <!-- Skip Next -->
        <button
          onclick={() => playerStore.next()}
          class="p-1.5 text-brand-text-secondary hover:text-brand-text-primary transition-colors cursor-pointer"
          title={i18n.t('playerBar.next')}
        >
          <SkipForward class="w-5 h-5 fill-current" />
        </button>

        <!-- Repeat Mode -->
        <button
          onclick={cycleRepeat}
          class="p-1.5 transition-colors hover:text-brand-text-primary cursor-pointer relative {playerStore.repeatMode !== 'off' ? 'text-brand-accent-text font-bold' : 'text-brand-text-secondary/60'}"
          title={`${i18n.t('playerBar.repeat')}: ${playerStore.repeatMode}`}
        >
          {#if playerStore.repeatMode === 'track'}
            <Repeat1 class="w-4.5 h-4.5" />
          {:else}
            <Repeat class="w-4.5 h-4.5" />
          {/if}
          {#if playerStore.repeatMode !== 'off' && playerStore.repeatMode !== 'track'}
            <span class="absolute -bottom-1 -right-1 text-[8px] bg-brand-accent text-brand-accent-contrast rounded-full px-0.5 scale-75 font-bold">
              {playerStore.repeatMode === 'album' ? 'AL' : playerStore.repeatMode === 'playlist' ? 'PL' : '1x'}
            </span>
          {/if}
        </button>
      </div>

      <!-- Waveform Progress Timeline positioned directly under play controls -->
      <div class="flex flex-col gap-1 w-full text-[10px] text-brand-text-secondary/70 px-1">
        <WaveformSeekBar />
        <div class="flex items-center justify-between w-full px-0.5 font-mono text-[9px] opacity-80">
          <span>{formatTime(playerStore.positionNanosec)}</span>
          <span>{formatTime(playerStore.currentSong?.length_nanosec)}</span>
        </div>
      </div>
    </div>

    <!-- Bottom-aligned Volume (left) & Restore (right) Controls -->
    <div class="flex items-center justify-between w-full flex-shrink-0 z-50">
      <!-- Volume Control -->
      <div class="flex items-center gap-1.5">
        <button
          onclick={toggleMute}
          class="p-1 text-brand-text-secondary/70 hover:text-brand-text-primary transition-colors cursor-pointer"
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
          class="volume-slider w-14 h-1 rounded-lg cursor-pointer outline-none"
          style={volumeSliderStyle}
          aria-label={i18n.t('playerBar.volumeSlider')}
          title={i18n.t('playerBar.volumeWithValue', { value: Math.round(volumePercent) })}
        />
      </div>

      <!-- Restore -->
      <button
        onclick={() => collectionStore.exitMiniplayerMode()}
        class="p-1 text-brand-text-secondary hover:text-brand-text-primary hover:bg-brand-border/40 rounded transition-colors cursor-pointer"
        title={i18n.t('miniplayer.exit', {}, 'Restore Full Window (Ctrl+M)')}
      >
        <Maximize2 class="w-3.5 h-3.5" />
      </button>
    </div>
  </div>
</div>

<style>
  :global(.glass-surface) {
    backdrop-filter: blur(20px) saturate(180%);
    -webkit-backdrop-filter: blur(20px) saturate(180%);
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
    cursor: pointer;
    box-shadow: 0 1px 3px rgba(0, 0, 0, 0.4);
    transition: transform 0.1s, border-color 0.2s;
  }

  .volume-slider::-webkit-slider-thumb:hover {
    transform: scale(1.25);
    border-color: var(--color-accent-hover);
  }

  .volume-slider::-moz-range-thumb {
    width: 10px;
    height: 10px;
    border: 2px solid var(--color-accent);
    border-radius: 50%;
    background: #ffffff;
    cursor: pointer;
    box-shadow: 0 1px 3px rgba(0, 0, 0, 0.4);
    transition: transform 0.1s, border-color 0.2s;
  }

  .volume-slider::-moz-range-thumb:hover {
    transform: scale(1.25);
    border-color: var(--color-accent-hover);
  }
</style>
