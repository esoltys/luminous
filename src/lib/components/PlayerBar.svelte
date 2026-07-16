<script lang="ts">
  import { playerStore } from "../stores/player.svelte";
  import { playlistsStore } from "../stores/playlists.svelte";
  import { collectionStore } from "../stores/collection.svelte";
  import { themeStore } from "../stores/theme.svelte";
  import CoverArt from "./CoverArt.svelte";
  import WaveformSeekBar from "./WaveformSeekBar.svelte";
  import MoodBar from "./MoodBar.svelte";
  import SpectrumVisualizer from "./SpectrumVisualizer.svelte";
  import { fade } from "svelte/transition";


  import {
    Play,
    Pause,
    SkipBack,
    SkipForward,
    Volume2,
    VolumeX,
    Shuffle,
    Repeat,
    Repeat1,
    Disc,
    PanelBottomOpen
  } from "lucide-svelte";

  // Helper to format nanoseconds to M:SS
  function formatTime(nanosec: number | undefined): string {
    if (nanosec === undefined) return "0:00";
    const sec = Math.floor(nanosec / 1_000_000_000);
    const m = Math.floor(sec / 60);
    const s = sec % 60;
    return `${m}:${s < 10 ? "0" : ""}${s}`;
  }

  // Handle seek progress bar click
  function handleSeek(e: Event) {
    const input = e.target as HTMLInputElement;
    const targetNs = parseFloat(input.value);
    playerStore.seek(targetNs);
  }

  // Handle volume bar click
  function handleVolumeChange(e: Event) {
    const input = e.target as HTMLInputElement;
    const vol = parseFloat(input.value);
    playerStore.setVolume(vol);
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
</script>

<footer in:fade={{ duration: 600 }} class="h-20 bg-brand-playerbar border border-brand-border rounded-[2rem] flex items-center justify-between px-8 text-brand-text-secondary select-none" class:glass-surface={themeStore.isGlassTheme}>
  <!-- Track Metadata & Art -->
  <div class="flex items-center gap-3 w-1/3 min-w-[200px]">
    <CoverArt
      songId={playerStore.currentSong?.id}
      artEmbedded={playerStore.currentSong?.art_embedded}
      artAutomatic={playerStore.currentSong?.art_automatic}
      artManual={playerStore.currentSong?.art_manual}
      sizeClass="w-12 h-12"
    />
    <div class="flex flex-col truncate">
      <div class="flex items-center gap-2">
        {#if playerStore.currentSong?.title}
          <button
            onclick={(e) => { e.stopPropagation(); collectionStore.navigateTo("collection", "songs", playerStore.currentSong?.title || ""); }}
            class="text-sm font-semibold text-brand-text-primary hover:text-brand-accent-text hover:underline transition-all duration-150 text-left truncate cursor-pointer"
            title="Filter by title: {playerStore.currentSong.title}"
          >
            {playerStore.currentSong.title}
          </button>
        {:else}
          <span class="text-sm font-semibold text-brand-text-primary truncate">
            Not Playing
          </span>
        {/if}
        {#if playerStore.currentSong}
          <span class="px-1.5 py-0.5 text-[9px] font-bold tracking-wider rounded uppercase bg-brand-accent/10 text-brand-accent-text border border-brand-accent/20 shadow-sm shrink-0">
            {playerStore.currentSong.filetype}
          </span>
        {/if}
      </div>
      {#if playerStore.currentSong?.artist}
        <button
          onclick={(e) => { e.stopPropagation(); collectionStore.viewArtist(playerStore.currentSong?.album_artist?.trim() || playerStore.currentSong?.artist || ""); }}
          class="text-xs text-brand-text-secondary/70 hover:text-brand-accent-text hover:underline transition-all duration-150 text-left truncate cursor-pointer"
          title="View artist: {playerStore.currentSong.artist}"
        >
          {playerStore.currentSong.artist}
        </button>
      {:else}
        <span class="text-xs text-brand-text-secondary/70 truncate">
          {playerStore.currentSong ? "Unknown Artist" : ""}
        </span>
      {/if}
    </div>
  </div>

  <!-- Player controls / Playback engine controller -->
  <div class="flex flex-col items-center gap-1.5 w-1/3 max-w-[600px]">
    <div class="flex items-center gap-5">
      <button
        onclick={cycleShuffle}
        class="text-xs transition-colors hover:text-brand-text-primary relative p-1 {playerStore.shuffleMode !== 'off' ? 'text-brand-accent-text font-bold' : 'text-brand-text-secondary/50'}"
        title="Shuffle Mode: {playerStore.shuffleMode}"
      >
        <Shuffle class="w-4 h-4" />
        {#if playerStore.shuffleMode !== 'off' && playerStore.shuffleMode !== 'all'}
          <span class="absolute -bottom-1 -right-1 text-[8px] bg-brand-accent text-brand-accent-contrast rounded-full px-0.5 scale-75">
            {playerStore.shuffleMode === 'inside_album' ? 'IA' : playerStore.shuffleMode === 'albums' ? 'AL' : 'AR'}
          </span>
        {/if}
      </button>

      <button onclick={() => playerStore.previous()} class="text-brand-text-secondary hover:text-brand-text-primary transition-colors">
        <SkipBack class="w-5 h-5 fill-current" />
      </button>

      {#if playerStore.state === 'playing'}
        <button
          onclick={() => playerStore.pause()}
          class="w-8 h-8 rounded-full bg-brand-accent hover:bg-brand-accent-hover text-brand-accent-contrast flex items-center justify-center hover:scale-105 transition-all shadow-md"
        >
          <Pause class="w-4 h-4 fill-current" />
        </button>
      {:else}
        <button
          onclick={() => playerStore.resume()}
          class="w-8 h-8 rounded-full bg-brand-accent hover:bg-brand-accent-hover text-brand-accent-contrast flex items-center justify-center hover:scale-105 transition-all shadow-md"
        >
          <Play class="w-4 h-4 fill-current ml-0.5" />
        </button>
      {/if}

      <button onclick={() => playerStore.next()} class="text-brand-text-secondary hover:text-brand-text-primary transition-colors">
        <SkipForward class="w-5 h-5 fill-current" />
      </button>

      <button
        onclick={cycleRepeat}
        class="text-xs transition-colors hover:text-brand-text-primary relative p-1 {playerStore.repeatMode !== 'off' ? 'text-brand-accent-text font-bold' : 'text-brand-text-secondary/50'}"
        title="Repeat Mode: {playerStore.repeatMode}"
      >
        {#if playerStore.repeatMode === 'track'}
          <Repeat1 class="w-4 h-4" />
        {:else}
          <Repeat class="w-4 h-4" />
        {/if}
        {#if playerStore.repeatMode !== 'off' && playerStore.repeatMode !== 'track' && playerStore.repeatMode !== 'playlist'}
          <span class="absolute -bottom-1 -right-1 text-[8px] bg-brand-accent text-brand-accent-contrast rounded-full px-0.5 scale-75">
            {playerStore.repeatMode === 'album' ? 'AL' : '1x'}
          </span>
        {/if}
      </button>
    </div>

    <!-- Scrubber -->
    <div class="flex items-center gap-2.5 w-full text-[10px] text-brand-text-secondary/60">
      <span>{formatTime(playerStore.positionNanosec)}</span>
      <div class="flex-1 flex flex-col gap-1">
        <WaveformSeekBar />
        <!-- <MoodBar /> -->
      </div>
      <span>{formatTime(playerStore.currentSong?.length_nanosec)}</span>
    </div>
  </div>

  <!-- Auxiliary (Volume & Visualizers) -->
  <div class="flex items-center justify-end gap-3 w-1/3 min-w-[200px]">
    <div class="w-24 h-7 mr-2 hidden md:block">
      <SpectrumVisualizer />
    </div>
    <button onclick={toggleMute} class="text-brand-text-secondary hover:text-brand-text-primary transition-colors">
      {#if isMuted || playerStore.volume === 0}
        <VolumeX class="w-4 h-4" />
      {:else}
        <Volume2 class="w-4 h-4" />
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
      class="volume-slider w-20 h-1 rounded-lg cursor-pointer outline-none"
      style="background: linear-gradient(to right, var(--color-accent) 0%, var(--color-accent) {playerStore.volume * 100}%, var(--color-border) {playerStore.volume * 100}%, var(--color-border) 100%)"
      aria-label="Volume Slider"
    />
    {#if collectionStore.immersiveMode}
      <button 
        onclick={() => collectionStore.toggleImmersiveMode()}
        class="text-brand-text-secondary hover:text-brand-accent-text transition-colors ml-2 p-1.5 rounded hover:bg-brand-main flex-shrink-0 cursor-pointer"
        title="Restore Full Interface"
      >
        <PanelBottomOpen class="w-4.5 h-4.5" />
      </button>
    {/if}
  </div>
</footer>

<style>
  /* Accent glow: only the PlayDock gets it, not the other glass panels —
     extends the shared --glass-shadow (elevation + highlight, app.css)
     with --glass-glow (theme.svelte.ts) rather than baking the glow into
     the shared variable itself. */
  footer.glass-surface {
    box-shadow: var(--glass-shadow, none), var(--glass-glow, none);
  }

  /* Liquid-glass "shine": a light-catching specular highlight on top of the
     existing blur+tint (.glass-surface, app.css), modeled on the two-corner
     inset highlight from https://codepen.io/lassiterda/pen/vEOpqMa. Plain
     box-shadow — no backdrop-filter/SVG-filter interaction, so it renders
     the same regardless of whether the blur itself composites. */
  footer.glass-surface::after {
    content: "";
    position: absolute;
    inset: 0;
    border-radius: inherit;
    pointer-events: none;
    box-shadow:
      inset 1.5px 1.5px 1px 0 rgba(255, 255, 255, 0.45),
      inset -1px -1px 1px 0 rgba(255, 255, 255, 0.18);
  }

  .volume-slider {
    -webkit-appearance: none;
    appearance: none;
    transition: background 0.15s ease;
  }

  /* Webkit thumb (Chrome, Safari, Edge, Opera) */
  .volume-slider::-webkit-slider-thumb {
    -webkit-appearance: none;
    appearance: none;
    width: 12px;
    height: 12px;
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

  /* Firefox thumb */
  .volume-slider::-moz-range-thumb {
    width: 12px;
    height: 12px;
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
