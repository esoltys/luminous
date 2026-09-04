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
    MusicNotesIcon as Music
  } from "phosphor-svelte";

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

  const isLinux = typeof navigator !== "undefined" && (/linux/i.test(navigator.userAgent) || /linux/i.test(navigator.platform));

  function handleStartDrag(e: PointerEvent) {
    invoke("start_window_drag").catch(() => {});
  }

  function handleStartResize(direction: string, e: PointerEvent) {
    if (isLinux) return;
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
  class="group relative w-full h-full flex flex-col justify-between overflow-hidden bg-brand-main select-none p-3 shadow-2xl {themeStore.isGlassTheme || isLinux ? 'glass-surface' : ''} {isLinux ? 'opaque-linux' : ''}"
>
  <!-- Edge and Corner Resize Handles for Frameless Window (non-Linux platforms) -->
  {#if !isLinux}
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
  {/if}
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

  <!-- IDLE STATIC LAYOUT -->
  <div class="relative z-10 w-full h-full flex flex-col items-center justify-between pointer-events-auto">
    <div class="flex-1 w-full flex items-center justify-center min-h-0 py-2">
      <div class="relative aspect-square h-full max-h-full max-w-[90%] rounded-none overflow-hidden border border-brand-border/30 bg-brand-sidebar flex items-center justify-center {isHovered ? 'scale-[0.98]' : ''} transition-transform duration-300">
        <CoverArt
          songId={playerStore.currentSong?.id}
          artEmbedded={playerStore.currentSong?.art_embedded}
          artAutomatic={playerStore.currentSong?.art_automatic}
          artManual={playerStore.currentSong?.art_manual}
          sizeClass="w-full h-full object-cover"
        />
      </div>
    </div>

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
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div
    onpointerenter={showHover}
    onpointermove={showHover}
    onpointerleave={hideHover}
    class="absolute inset-0 z-30 flex flex-col justify-between p-3 transition-opacity duration-200 {isHovered ? 'opacity-100 pointer-events-auto' : 'opacity-0 pointer-events-none'} {isLinux ? 'bg-brand-main' : 'bg-brand-main/85 backdrop-blur-md'}"
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

<style>
  :global(.glass-surface) {
    -webkit-backdrop-filter: blur(20px) saturate(180%);
    backdrop-filter: blur(20px) saturate(180%);
  }

  /* backdrop-filter doesn't composite reliably in WebKitGTK, rendering the
     panel see-through instead of frosted — same issue and same fix as
     PlayerBar's footer.opaque-linux (PlayerBar.svelte). */
  :global(.glass-surface.opaque-linux) {
    background-color: var(--bg-main, #08090c) !important;
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
</style>
