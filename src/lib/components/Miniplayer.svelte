<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { getCurrentWindow } from "@tauri-apps/api/window";
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
    GripHorizontal,
    Scaling
  } from "lucide-svelte";

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

  function handleStartResize(e: PointerEvent) {
    e.preventDefault();
    e.stopPropagation();

    // Native OS resize and the manual pointer-drag fallback below must be
    // mutually exclusive: if both run, they fight over the window size
    // (the fallback computes deltas from a stale start size while the OS
    // resizes live), producing an unpredictable final size that doesn't
    // match what the user actually dragged to.
    try {
      const appWindow = getCurrentWindow() as any;
      if (appWindow && typeof appWindow.startResizing === "function") {
        appWindow.startResizing("south-east").catch(() => {});
        return;
      }
    } catch {
      // Fall through to manual pointer drag resize
    }

    const startX = e.clientX;
    const startY = e.clientY;
    const startWidth = window.innerWidth;
    const startHeight = window.innerHeight;

    function onPointerMove(moveEvent: PointerEvent) {
      const deltaX = moveEvent.clientX - startX;
      const deltaY = moveEvent.clientY - startY;
      const newWidth = Math.max(220, Math.min(650, startWidth + deltaX));
      const newHeight = Math.max(220, Math.min(650, startHeight + deltaY));
      invoke("resize_miniplayer", { width: newWidth, height: newHeight }).catch(() => {});
    }

    function onPointerUp() {
      window.removeEventListener("pointermove", onPointerMove);
      window.removeEventListener("pointerup", onPointerUp);
    }

    window.addEventListener("pointermove", onPointerMove);
    window.addEventListener("pointerup", onPointerUp);
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

<!-- svelte-ignore a11y_no_noninteractive_element_interactions a11y_no_noninteractive_tabindex -->
<div
  role="region"
  aria-label="Miniplayer"
  onkeydown={handleKeyDown}
  tabindex="0"
  class="group relative w-full h-full flex flex-col justify-between overflow-hidden bg-brand-main select-none p-3 shadow-2xl {themeStore.isGlassTheme ? 'glass-surface' : ''}"
>
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
    <!-- Top-aligned Window Drag Region and Close/Restore boundary (`X`) -->
    <div class="flex items-center justify-between w-full flex-shrink-0 z-40">
      <!-- Clean Drag Handle icon -->
      <div
        data-tauri-drag-region
        onpointerdown={handleStartDrag}
        role="button"
        tabindex="0"
        aria-label={i18n.t('miniplayer.dragHint', {}, 'Drag window')}
        class="flex items-center justify-center cursor-grab active:cursor-grabbing text-brand-text-secondary/70 hover:text-brand-text-primary transition-colors p-1.5 rounded bg-brand-sidebar/40 hover:bg-brand-sidebar/80"
        title={i18n.t('miniplayer.dragHint', {}, 'Drag window')}
      >
        <GripHorizontal class="w-4 h-4" />
      </div>

      <!-- Title / App Label -->
      <span class="text-[10px] font-semibold tracking-wider uppercase text-brand-accent-text opacity-90 truncate max-w-[120px]">
        Luminous
      </span>

      <!-- Restore / Exit Miniplayer Button -->
      <button
        onclick={() => collectionStore.exitMiniplayerMode()}
        class="p-1.5 text-brand-text-secondary hover:text-brand-text-primary hover:bg-brand-border/40 rounded transition-colors cursor-pointer"
        title={i18n.t('miniplayer.exit', {}, 'Restore Full Window (Esc)')}
      >
        <Maximize2 class="w-4 h-4" />
      </button>
    </div>

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

    <!-- Pixel-Resize Grab Handle anchored in absolute bottom-right corner -->
    <div
      onpointerdown={handleStartResize}
      role="button"
      tabindex="0"
      aria-label={i18n.t('miniplayer.resizeHint', {}, 'Resize Window')}
      class="absolute bottom-1 right-1 p-1 text-brand-text-secondary/60 hover:text-brand-text-primary cursor-se-resize transition-colors z-50 rounded hover:bg-brand-sidebar/40"
      title={i18n.t('miniplayer.resizeHint', {}, 'Resize Window')}
    >
      <Scaling class="w-3.5 h-3.5 rotate-90" />
    </div>
  </div>
</div>

<style>
  :global(.glass-surface) {
    backdrop-filter: blur(20px) saturate(180%);
    -webkit-backdrop-filter: blur(20px) saturate(180%);
  }
</style>
