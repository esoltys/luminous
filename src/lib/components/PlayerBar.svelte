<script lang="ts">
  import { playerStore } from "../stores/player.svelte";
  import { playlistsStore } from "../stores/playlists.svelte";
  import { collectionStore } from "../stores/collection.svelte";
  import { themeStore } from "../stores/theme.svelte";
  import { isSmartPlaylistSpec } from "../utils/filterParser";
  import { prefs } from "../stores/prefs.svelte";
  import CoverArt from "./CoverArt.svelte";
  import SongRating from "./SongRating.svelte";
  import { i18n } from "../stores/i18n.svelte";
  import WaveformSeekBar from "./WaveformSeekBar.svelte";
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
    Disc,
    PanelBottomOpen,
    AudioWaveform,
    Palette
  } from "lucide-svelte";

  // Helper to format nanoseconds to M:SS
  function formatTime(nanosec: number | undefined): string {
    if (nanosec === undefined) return "0:00";
    const sec = Math.floor(nanosec / 1_000_000_000);
    const m = Math.floor(sec / 60);
    const s = sec % 60;
    return `${m}:${s < 10 ? "0" : ""}${s}`;
  }

  // Volume slider gradient style
  let volumePercent = $derived(playerStore.volume * 100);
  let volumeSliderStyle = $derived(
    `background: linear-gradient(to right, var(--color-accent) 0%, var(--color-accent) ${volumePercent}%, var(--color-border) ${volumePercent}%, var(--color-border) 100%)`
  );

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
      case "one_by_one": return i18n.t('playerBar.repeatOneByOne');
      default: return mode;
    }
  }

  let coverTitle = $derived.by(() => {
    if (!playerStore.currentSong) return "";
    const pid = playerStore.playlistId;
    if (pid && pid > 0) {
      const pl = playlistsStore.playlists.find((p) => p.id === pid);
      if (pl) {
        return `Go to playlist "${pl.name}"`;
      }
    }
    return playerStore.currentSong.album ? i18n.t('collection.filterByAlbum', { album: playerStore.currentSong.album }) : "";
  });

  function handleCoverClick(e: MouseEvent) {
    if (!playerStore.currentSong) return;
    e.stopPropagation();

    const pid = playerStore.playlistId;
    if (pid && pid > 0) {
      const pl = playlistsStore.playlists.find((p) => p.id === pid);
      if (pl) {
        if (pl.dynamic_enabled && !isSmartPlaylistSpec(pl.dynamic_spec)) {
          const spec = pl.dynamic_spec ?? "";
          if (spec.startsWith("decade:")) {
            const decade = spec.replace(/^decade:/, "");
            collectionStore.viewAutoPlaylist({
              kind: "decade",
              decade,
              playlistId: pl.id,
              updated: pl.updated,
            });
          } else {
            collectionStore.viewAutoPlaylist({
              kind: "genre",
              genre: spec || pl.name,
              playlistId: pl.id,
              updated: pl.updated,
            });
          }
          return;
        } else {
          playlistsStore.selectPlaylist(pl.id);
          collectionStore.viewPlaylist(pl.id);
          return;
        }
      }
    }

    if (playerStore.currentSong.album) {
      collectionStore.viewAlbum(playerStore.currentSong.album);
    }
  }
</script>

<footer class="h-20 bg-brand-playerbar border border-brand-border rounded-[2rem] flex items-center justify-between px-8 text-brand-text-secondary select-none" class:glass-surface={themeStore.isGlassTheme}>
  <!-- Track Metadata & Art -->
  <div class="flex items-center gap-3 w-1/3 min-w-[200px]">
    <button
      onclick={handleCoverClick}
      disabled={!playerStore.currentSong}
      class="group relative overflow-hidden focus:outline-hidden flex-shrink-0 cursor-pointer disabled:cursor-default disabled:pointer-events-none active:scale-95 transition-transform duration-200"
      title={coverTitle}
    >
      <CoverArt
        songId={playerStore.currentSong?.id}
        artEmbedded={playerStore.currentSong?.art_embedded}
        artAutomatic={playerStore.currentSong?.art_automatic}
        artManual={playerStore.currentSong?.art_manual}
        sizeClass="w-12 h-12 transition-all duration-300 group-hover:scale-105"
      />
    </button>
    <div class="flex flex-col truncate">
      <div class="flex items-center gap-2">
        <span class="text-sm font-semibold text-brand-text-primary truncate" title={playerStore.currentSong?.title}>
          {playerStore.currentSong?.title || i18n.t('playerBar.notPlaying')}
        </span>
      </div>
      {#if playerStore.currentSong?.artist}
        <button
          onclick={(e) => { e.stopPropagation(); collectionStore.viewArtist(playerStore.currentSong?.album_artist?.trim() || playerStore.currentSong?.artist || ""); }}
          class="text-xs text-brand-text-secondary/70 hover:text-brand-accent-text hover:underline transition-all duration-150 text-left truncate cursor-pointer"
          title={i18n.t('collection.filterByArtist', { artist: playerStore.currentSong.artist })}
        >
          {playerStore.currentSong.artist}
        </button>
      {:else}
        <span class="text-xs text-brand-text-secondary/70 truncate">
          {playerStore.currentSong ? i18n.t('collection.unknownArtist') : ""}
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
        title={`${i18n.t('playerBar.shuffle')}: ${shuffleModeLabel(playerStore.shuffleMode)}`}
      >
        <Shuffle class="w-4 h-4" />
        {#if playerStore.shuffleMode !== 'off' && playerStore.shuffleMode !== 'all'}
          <span class="absolute -bottom-1 -right-1 text-[8px] bg-brand-accent text-brand-accent-contrast rounded-full px-0.5 scale-75">
            {playerStore.shuffleMode === 'inside_album' ? 'IA' : playerStore.shuffleMode === 'albums' ? 'AL' : 'AR'}
          </span>
        {/if}
      </button>

      <button onclick={() => playerStore.previous()} class="text-brand-text-secondary hover:text-brand-text-primary transition-colors" title={i18n.t('playerBar.previous')}>
        <SkipBack class="w-5 h-5 fill-current" />
      </button>

      {#if playerStore.state === 'playing'}
        <button
          onclick={() => playerStore.pause()}
          class="w-8 h-8 rounded-full bg-brand-accent hover:bg-brand-accent-hover text-brand-accent-contrast flex items-center justify-center hover:scale-105 transition-all shadow-md"
          title={i18n.t('playerBar.pause')}
        >
          <Pause class="w-4 h-4 fill-current" />
        </button>
      {:else}
        <button
          onclick={() => playerStore.resume()}
          class="w-8 h-8 rounded-full bg-brand-accent hover:bg-brand-accent-hover text-brand-accent-contrast flex items-center justify-center hover:scale-105 transition-all shadow-md"
          title={i18n.t('playerBar.play')}
        >
          <Play class="w-4 h-4 fill-current ml-0.5" />
        </button>
      {/if}

      <button onclick={() => playerStore.next()} class="text-brand-text-secondary hover:text-brand-text-primary transition-colors" title={i18n.t('playerBar.next')}>
        <SkipForward class="w-5 h-5 fill-current" />
      </button>

      <button
        onclick={cycleRepeat}
        class="text-xs transition-colors hover:text-brand-text-primary relative p-1 {playerStore.repeatMode !== 'off' ? 'text-brand-accent-text font-bold' : 'text-brand-text-secondary/50'}"
        title={`${i18n.t('playerBar.repeat')}: ${repeatModeLabel(playerStore.repeatMode)}`}
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

      {#if playerStore.currentSong}
        <SongRating
          rating={playerStore.currentSong.rating}
          onRate={(r) => playerStore.rateCurrent(r)}
        />
      {/if}
    </div>

    <!-- Scrubber -->
    <div class="flex items-center gap-2.5 w-full text-[10px] text-brand-text-secondary/60">
      <span>{formatTime(playerStore.positionNanosec)}</span>
      <div class="flex-1 flex flex-col gap-1">
        <WaveformSeekBar />
      </div>
      <button
        onclick={() => prefs.toggleSeekBarMode()}
        class="text-brand-text-secondary/50 hover:text-brand-text-primary transition-colors p-0.5 flex-shrink-0"
        title={prefs.seekBarMode === 'waveform'
          ? i18n.t('playerBar.seekbarModeWaveform', {}, 'Waveform mode — click to switch to moodbar')
          : i18n.t('playerBar.seekbarModeMoodbar', {}, 'Moodbar mode — click to switch to waveform')}
      >
        {#if prefs.seekBarMode === 'waveform'}
          <AudioWaveform class="w-3 h-3" />
        {:else}
          <Palette class="w-3 h-3" />
        {/if}
      </button>
      <span>{formatTime(playerStore.currentSong?.length_nanosec)}</span>
    </div>
  </div>

  <!-- Auxiliary (Volume & Visualizers) -->
  <div class="flex items-center justify-end gap-3 w-1/3 min-w-[200px]">
    <div class="w-24 h-7 mr-2 hidden md:block">
      <SpectrumVisualizer />
    </div>
    <button onclick={toggleMute} class="text-brand-text-secondary hover:text-brand-text-primary transition-colors" title={i18n.t('playerBar.volume')}>
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
      style={volumeSliderStyle}
      aria-label={i18n.t('playerBar.volumeSlider')}
      title={i18n.t('playerBar.volume')}
    />
    {#if collectionStore.immersiveMode}
      <button 
        onclick={() => collectionStore.toggleImmersiveMode()}
        class="text-brand-text-secondary hover:text-brand-accent-text transition-colors ml-2 p-1.5 rounded hover:bg-brand-main flex-shrink-0 cursor-pointer"
        title={i18n.t('playerBar.restoreInterface', {}, 'Restore Full Interface')}
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
    /* Unlike the other glass panels (Sidebar, TopNavigation, RightPanel),
       the PlayDock floats as a *sibling* of +layout.svelte's
       .flip-perspective 3D-transform container rather than living inside
       it, sampling that container's content from outside for its blur.
       Chromium-family engines (confirmed on Windows WebView2; already
       worked around for Linux/WebKitGTK via .no-3d) can fail to composite
       backdrop-filter correctly for an element outside a 3D-transformed
       subtree it needs to sample — the tint renders but the blur silently
       drops. Forcing this element onto its own explicit GPU layer
       sidesteps the buggy automatic layer assignment. */
    isolation: isolate;
    will-change: backdrop-filter;
    transform: translateZ(0);
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
