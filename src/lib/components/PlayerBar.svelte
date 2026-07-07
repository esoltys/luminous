<script lang="ts">
  import { playerStore } from "../stores/player.svelte";
  import { playlistsStore } from "../stores/playlists.svelte";
  import CoverArt from "./CoverArt.svelte";
  import WaveformSeekBar from "./WaveformSeekBar.svelte";
  import MoodBar from "./MoodBar.svelte";
  import SpectrumVisualizer from "./SpectrumVisualizer.svelte";
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
    Disc
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

<footer class="h-20 bg-brand-playerbar border-t border-brand-border flex items-center justify-between px-6 text-brand-text-secondary select-none">
  <!-- Track Metadata & Art -->
  <div class="flex items-center gap-3 w-1/3 min-w-[200px]">
    <CoverArt
      songId={playerStore.currentSong?.id}
      artEmbedded={playerStore.currentSong?.art_embedded}
      artAutomatic={playerStore.currentSong?.art_automatic}
      artManual={playerStore.currentSong?.art_manual}
      sizeClass="w-12 h-12"
      animateSpin={playerStore.state === 'playing'}
    />
    <div class="flex flex-col truncate">
      <div class="flex items-center gap-2">
        <span class="text-sm font-semibold text-brand-text-primary truncate">
          {playerStore.currentSong?.title || "Not Playing"}
        </span>
        {#if playerStore.currentSong}
          <span class="px-1.5 py-0.5 text-[9px] font-bold tracking-wider rounded uppercase bg-brand-accent/10 text-brand-accent border border-brand-accent/20 shadow-sm shrink-0">
            {playerStore.currentSong.filetype}
          </span>
        {/if}
      </div>
      <span class="text-xs text-brand-text-secondary/70 truncate">
        {playerStore.currentSong?.artist || "Unknown Artist"}
      </span>
    </div>
  </div>

  <!-- Player controls / Playback engine controller -->
  <div class="flex flex-col items-center gap-1.5 w-1/3 max-w-[600px]">
    <div class="flex items-center gap-5">
      <button
        onclick={cycleShuffle}
        class="text-xs transition-colors hover:text-brand-text-primary relative p-1 {playerStore.shuffleMode !== 'off' ? 'text-brand-accent font-bold' : 'text-brand-text-secondary/50'}"
        title="Shuffle Mode: {playerStore.shuffleMode}"
      >
        <Shuffle class="w-4 h-4" />
        {#if playerStore.shuffleMode !== 'off' && playerStore.shuffleMode !== 'all'}
          <span class="absolute -bottom-1 -right-1 text-[8px] bg-brand-accent text-brand-text-primary rounded-full px-0.5 scale-75">
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
          class="w-8 h-8 rounded-full bg-brand-accent hover:bg-brand-accent-hover text-brand-text-primary flex items-center justify-center hover:scale-105 transition-all shadow-md"
        >
          <Pause class="w-4 h-4 fill-current text-white" />
        </button>
      {:else}
        <button
          onclick={() => playerStore.resume()}
          class="w-8 h-8 rounded-full bg-brand-accent hover:bg-brand-accent-hover text-brand-text-primary flex items-center justify-center hover:scale-105 transition-all shadow-md"
        >
          <Play class="w-4 h-4 fill-current text-white ml-0.5" />
        </button>
      {/if}

      <button onclick={() => playerStore.next()} class="text-brand-text-secondary hover:text-brand-text-primary transition-colors">
        <SkipForward class="w-5 h-5 fill-current" />
      </button>

      <button
        onclick={cycleRepeat}
        class="text-xs transition-colors hover:text-brand-text-primary relative p-1 {playerStore.repeatMode !== 'off' ? 'text-brand-accent font-bold' : 'text-brand-text-secondary/50'}"
        title="Repeat Mode: {playerStore.repeatMode}"
      >
        {#if playerStore.repeatMode === 'track'}
          <Repeat1 class="w-4 h-4" />
        {:else}
          <Repeat class="w-4 h-4" />
        {/if}
        {#if playerStore.repeatMode !== 'off' && playerStore.repeatMode !== 'track' && playerStore.repeatMode !== 'playlist'}
          <span class="absolute -bottom-1 -right-1 text-[8px] bg-brand-accent text-brand-text-primary rounded-full px-0.5 scale-75">
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
        <MoodBar />
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
      class="w-24 accent-brand-accent h-1 bg-brand-border rounded-lg appearance-none cursor-pointer"
    />
  </div>
</footer>
