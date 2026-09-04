<script lang="ts">
  import { fly } from 'svelte/transition';
  import { cubicOut } from 'svelte/easing';
  import { playerStore } from "../stores/player.svelte";
  import { playlistsStore } from "../stores/playlists.svelte";
  import { collectionStore } from "../stores/collection.svelte";
  import { navigationStore } from "../stores/navigation.svelte";
  import { windowLayoutStore } from "../stores/windowLayout.svelte";
  import { themeStore } from "../stores/theme.svelte";
  import { isSmartPlaylistSpec } from "../utils/filterParser";
  import { formatDuration } from "../utils/formatters";
  import { prefs } from "../stores/prefs.svelte";
  import CoverArt from "./CoverArt.svelte";
  import SongRating from "./SongRating.svelte";
  import { i18n } from "../stores/i18n.svelte";
  import WaveformSeekBar from "./WaveformSeekBar.svelte";
  import SpectrumVisualizer from "./SpectrumVisualizer.svelte";
  import LinkButton from "./LinkButton.svelte";

  const isLinux = typeof navigator !== 'undefined' && navigator.userAgent.includes('Linux');

  // Responsive control trimming (issue #413, refined against real usage,
  // padding/seekbar fixed under #543): three named tiers as this floating
  // bar narrows toward the app's 320px minWidth — Full (>=700px), Compact
  // (400-700px), Minimal (<400px). Cover art, play/pause, and skip-next are
  // the constant core (shown in all three tiers); everything else drops out
  // in priority order: expand/shuffle/repeat/volume+mute/waveform seek bar
  // + time labels first (gone by Compact — the transport block takes the
  // freed space and sticks to the right edge via `ml-auto` rather than
  // re-centering in it), then prev (gone by Minimal). Horizontal padding on
  // the outer bar stays constant across all tiers so cover art never sits
  // flush against the rounded edges. 400/700 are hand-tuned breakpoints
  // specific to this bar, written as literal `min-[Npx]:` arbitrary-value
  // classes because Tailwind's class scanner can't resolve an interpolated
  // constants.ts value — don't try to centralize them.

  import {
    PlayIcon as Play,
    PauseIcon as Pause,
    SkipBackIcon as SkipBack,
    SkipForwardIcon as SkipForward,
    SpeakerHighIcon as Volume2,
    SpeakerSimpleSlashIcon as VolumeX,
    ShuffleIcon as Shuffle,
    RepeatIcon as Repeat,
    WaveformIcon as AudioWaveform,
    PaletteIcon as Palette,
    PictureInPictureIcon as PictureInPicture,
    DiscIcon as DiscAlbum,
    MicrophoneStageIcon as Mic2,
    PlaylistIcon as ListMusic,
    MusicNotesIcon as Music,
    InfoIcon as Info
  } from "phosphor-svelte";




  let volumePercent = $derived(playerStore.volume * 100);
  let volumeSliderStyle = $derived(
    `background: linear-gradient(to right, var(--color-accent) 0%, var(--color-accent) ${volumePercent}%, var(--color-border) ${volumePercent}%, var(--color-border) 100%)`
  );

  function handleSeek(e: Event) {
    const input = e.target as HTMLInputElement;
    const targetNs = parseFloat(input.value);
    playerStore.seek(targetNs);
  }

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
    const modes: import("../types").RepeatMode[] = ["off", "track", "album", "playlist"];
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

  // True only when the user is already looking at the Queue itself (not
  // immersive, on the Playlists tab's custom-playlists sub-tab, with the
  // Queue playlist selected) — distinguishes "viewing the Queue" from
  // "viewing any other collection/playlist", which the cover-art click
  // handler below needs in order to pick the right next state (#523).
  let isViewingQueue = $derived.by(() => {
    if (windowLayoutStore.immersiveMode) return false;
    const queuePl = playlistsStore.queuePlaylist;
    if (!queuePl) return false;
    return (
      navigationStore.activeTab === 'playlists' &&
      navigationStore.playlistsSubTab === 'custom' &&
      navigationStore.selectedPlaylistId === queuePl.id
    );
  });

  let coverTitle = $derived.by(() => {
    if (!playerStore.currentSong) return "";
    return isViewingQueue
      ? i18n.t('playerBar.immersiveTitle', {}, 'Immersive Mode')
      : i18n.t('playerBar.queueTitle', {}, 'Queue');
  });

  async function navigateToQueue() {
    const queuePl = await playlistsStore.requireQueue();
    playlistsStore.selectPlaylist(queuePl.id);
    navigationStore.viewPlaylist(queuePl.id);
  }

  // Three-state navigation flow (#523): from any collection/playlist view,
  // clicking goes to the Queue; from the Queue, it flips into Immersive
  // Mode; from Immersive Mode, it flips back to the Queue.
  async function handleCoverClick(e: MouseEvent) {
    if (!playerStore.currentSong) return;
    e.stopPropagation();

    // While the window is too narrow to show the front face at all
    // (isImmersiveForced), immersive is engaged regardless of what this
    // toggles — clicking through to "exit and view Queue" would silently
    // clear the user's own immersiveMode preference for a screen they can't
    // actually reach yet, so leave it alone until the window widens back out.
    if (windowLayoutStore.isImmersiveForced) return;

    if (windowLayoutStore.immersiveMode) {
      windowLayoutStore.exitImmersiveMode();
      await navigateToQueue();
    } else if (isViewingQueue) {
      windowLayoutStore.toggleImmersiveMode();
    } else {
      await navigateToQueue();
    }
  }
</script>

<footer transition:fly={{ y: 40, duration: 300, easing: cubicOut }} class="h-20 max-w-[1200px] mx-auto bg-brand-playerbar border border-brand-border rounded-[2rem] flex items-center justify-between gap-3 px-8 text-brand-text-secondary select-none {themeStore.isGlassTheme || isLinux ? 'glass-surface' : ''} {isLinux ? 'opaque-linux' : ''}">
  <div class="flex items-center gap-3 flex-1 min-[700px]:w-1/3 min-[700px]:flex-none min-w-[90px] min-[400px]:min-w-[140px] min-[700px]:min-w-[200px] max-w-sm">
    <button
      onclick={handleCoverClick}
      disabled={!playerStore.currentSong}
      class="group relative overflow-hidden focus:outline-hidden flex-shrink-0 disabled:cursor-default disabled:pointer-events-none active:scale-95 transition-transform duration-200"
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
        {#if playerStore.currentSong}
          <SongRating
            rating={playerStore.currentSong.rating}
            onRate={(r) => playerStore.rateCurrent(r)}
            size="sm"
          />
        {/if}
      </div>
      {#if playerStore.currentSong?.album}
        <LinkButton
          onclick={(e) => { e.stopPropagation(); navigationStore.viewAlbum(playerStore.currentSong?.album || ""); }}
          class="text-xs text-brand-text-secondary/70 truncate"
          title={i18n.t('collection.filterByAlbum', { album: playerStore.currentSong.album })}
        >
          {playerStore.currentSong.album}
        </LinkButton>
      {:else if playerStore.currentSong}
        <span class="text-xs text-brand-text-secondary/70 truncate">
          {i18n.t('collection.unknownAlbum')}
        </span>
      {/if}
      {#if playerStore.currentSong?.artist}
        <LinkButton
          onclick={(e) => { e.stopPropagation(); navigationStore.viewArtist(playerStore.currentSong?.album_artist?.trim() || playerStore.currentSong?.artist || ""); }}
          class="text-xs text-brand-text-secondary/70 truncate"
          title={i18n.t('collection.filterByArtist', { artist: playerStore.currentSong.artist })}
        >
          {playerStore.currentSong.artist}
        </LinkButton>
      {:else}
        <span class="text-xs text-brand-text-secondary/70 truncate">
          {playerStore.currentSong ? i18n.t('collection.unknownArtist') : ""}
        </span>
      {/if}
    </div>
  </div>

  <div class="flex flex-col items-center gap-1.5 min-[400px]:min-w-[220px] min-[400px]:ml-auto min-[700px]:w-1/3 min-[700px]:flex-none min-[700px]:ml-0 max-w-[600px]">
    <div class="flex items-center gap-3 min-[700px]:gap-5">
      <div class="hidden min-[700px]:block relative">
        {#if playerStore.shuffleMode !== 'off'}
          <button
            onclick={cycleShuffle}
            class="absolute right-full top-1/2 -translate-y-1/2 mr-1.5 text-[10px] font-semibold text-brand-accent-text hover:text-brand-text-primary transition-colors uppercase tracking-wide whitespace-nowrap"
            title={`${i18n.t('playerBar.shuffle')}: ${shuffleModeLabel(playerStore.shuffleMode)} — ${shuffleModeDescription(playerStore.shuffleMode)}`}
          >
            {i18n.t('playerBar.shuffle')} {shuffleModeLabel(playerStore.shuffleMode)}
          </button>
        {/if}
        <button
          onclick={cycleShuffle}
          class="text-xs transition-colors hover:text-brand-text-primary flex items-center gap-1 p-1 {playerStore.shuffleMode !== 'off' ? 'text-brand-accent-text font-bold' : 'text-brand-text-secondary/50'}"
          title={`${i18n.t('playerBar.shuffle')}: ${shuffleModeLabel(playerStore.shuffleMode)} — ${shuffleModeDescription(playerStore.shuffleMode)}`}
        >
          {#if shuffleTypeIcon(playerStore.shuffleMode)}
            {@const ShuffleTypeIcon = shuffleTypeIcon(playerStore.shuffleMode)}
            <ShuffleTypeIcon class="w-4 h-4" />
          {/if}
          <Shuffle class="w-4 h-4" />
        </button>
      </div>

      <button onclick={() => playerStore.previous()} class="hidden min-[400px]:block text-brand-text-secondary hover:text-brand-text-primary transition-colors" title={i18n.t('playerBar.previous')}>
        <SkipBack class="w-5 h-5 fill-current" />
      </button>

      {#if playerStore.state === 'playing'}
        <button
          onclick={() => playerStore.pause()}
          class="w-8 h-8 rounded-full bg-brand-accent hover:bg-brand-accent-hover text-brand-accent-contrast flex items-center justify-center transition-colors"
          title={i18n.t('playerBar.pause')}
        >
          <Pause class="w-4 h-4 fill-current" />
        </button>
      {:else}
        <button
          onclick={() => playerStore.resume()}
          class="w-8 h-8 rounded-full bg-brand-accent hover:bg-brand-accent-hover text-brand-accent-contrast flex items-center justify-center transition-colors"
          title={i18n.t('playerBar.play')}
        >
          <Play class="w-4 h-4 fill-current" />
        </button>
      {/if}

      <button onclick={() => playerStore.next()} class="text-brand-text-secondary hover:text-brand-text-primary transition-colors" title={i18n.t('playerBar.next')}>
        <SkipForward class="w-5 h-5 fill-current" />
      </button>

      <div class="hidden min-[700px]:block relative">
        <button
          onclick={cycleRepeat}
          class="text-xs transition-colors hover:text-brand-text-primary flex items-center gap-1 p-1 {playerStore.repeatMode !== 'off' ? 'text-brand-accent-text font-bold' : 'text-brand-text-secondary/50'}"
          title={`${i18n.t('playerBar.repeat')}: ${repeatModeLabel(playerStore.repeatMode)} — ${repeatModeDescription(playerStore.repeatMode)}`}
        >
          <Repeat class="w-4 h-4" />
          {#if repeatTypeIcon(playerStore.repeatMode)}
            {@const RepeatTypeIcon = repeatTypeIcon(playerStore.repeatMode)}
            <RepeatTypeIcon class="w-4 h-4" />
          {/if}
        </button>
        {#if playerStore.repeatMode !== 'off'}
          <button
            onclick={cycleRepeat}
            class="absolute left-full top-1/2 -translate-y-1/2 ml-1.5 text-[10px] font-semibold text-brand-accent-text hover:text-brand-text-primary transition-colors uppercase tracking-wide whitespace-nowrap"
            title={`${i18n.t('playerBar.repeat')}: ${repeatModeLabel(playerStore.repeatMode)} — ${repeatModeDescription(playerStore.repeatMode)}`}
          >
            {i18n.t('playerBar.repeat')} {repeatModeLabel(playerStore.repeatMode)}
          </button>
        {/if}
      </div>

      <button
        onclick={() => windowLayoutStore.toggleMiniplayerMode()}
        class="min-[700px]:hidden text-brand-text-secondary hover:text-brand-accent-text transition-colors flex-shrink-0"
        title={i18n.t('miniplayer.toggleTooltip', {}, 'Picture-in-Picture Mode (Ctrl+M)')}
      >
        <PictureInPicture class="w-4 h-4" />
      </button>

    </div>

    <div class="hidden min-[700px]:flex items-center gap-2.5 w-full text-[10px] text-brand-text-secondary/60">
      <!-- Invisible spacer matching the mode-toggle button's footprint, so the
           waveform + timers stay centered instead of skewing left toward it. -->
      <div class="w-4 h-4 flex-shrink-0" aria-hidden="true"></div>
      <span>{formatDuration(playerStore.positionNanosec)}</span>
      <div class="flex-1 flex flex-col gap-1">
        <WaveformSeekBar />
      </div>
      <span>{formatDuration(playerStore.currentSong?.length_nanosec)}</span>
      <button
        onclick={() => prefs.toggleSeekBarMode()}
        class="text-brand-text-secondary/50 hover:text-brand-text-primary transition-colors p-0.5 flex-shrink-0"
        title={prefs.seekBarMode === 'waveform'
          ? i18n.t('playerBar.seekbarModeWaveform', {}, 'Waveform mode — click to switch to frequency bands')
          : i18n.t('playerBar.seekbarModeBands', {}, 'Frequency bands mode — click to switch to waveform')}
      >
        {#if prefs.seekBarMode === 'waveform'}
          <AudioWaveform class="w-3 h-3" />
        {:else}
          <Palette class="w-3 h-3" />
        {/if}
      </button>
    </div>
  </div>

  <div class="hidden min-[700px]:flex items-center justify-end gap-1.5 min-[700px]:gap-3 w-1/3 min-w-[50px] min-[700px]:min-w-[200px] max-w-xs">
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
      class="volume-slider hidden min-[700px]:block w-20 h-1 rounded-lg outline-none"
      style={volumeSliderStyle}
      aria-label={i18n.t('playerBar.volumeSlider')}
      title={i18n.t('playerBar.volumeWithValue', { value: Math.round(volumePercent) })}
    />
    <div class="hidden min-[700px]:flex flex-col items-center justify-center gap-1 flex-shrink-0">
      {#if !windowLayoutStore.isRightPanelAutoHidden}
        <button
          onclick={() => windowLayoutStore.toggleRightPanel()}
          class="text-brand-text-secondary hover:text-brand-accent-text transition-colors p-1.5 rounded hover:bg-brand-main/60 {windowLayoutStore.rightPanelOpen ? 'text-brand-accent-text' : ''}"
          title={i18n.t('topNav.toggleRightPanel')}
        >
          <Info class="w-5 h-5" />
        </button>
      {/if}
      <button
        onclick={() => windowLayoutStore.toggleMiniplayerMode()}
        class="text-brand-text-secondary hover:text-brand-accent-text transition-colors p-1.5 rounded hover:bg-brand-main/60"
        title={i18n.t('miniplayer.toggleTooltip', {}, 'Picture-in-Picture Mode (Ctrl+M)')}
      >
        <PictureInPicture class="w-4.5 h-4.5" />
      </button>
    </div>
  </div>
</footer>

<style>
  /* Accent glow: only the PlayDock gets it, not the other glass panels —
     extends the shared --glass-shadow (elevation + highlight, app.css)
     with --glass-glow (theme.svelte.ts) rather than baking the glow into
     the shared variable itself. */
  :global(footer.glass-surface) {
    position: relative;
    -webkit-backdrop-filter: blur(20px) saturate(180%) !important;
    backdrop-filter: blur(20px) saturate(180%) !important;
    background-color: var(--glass-bg-playerbar) !important;
    border-color: var(--glass-border-color, var(--color-border)) !important;
    box-shadow: var(--glass-shadow, none), var(--glass-glow, none);
  }

  :global(footer.glass-surface.opaque-linux) {
    background-color: var(--bg-playerbar, #191b23) !important;
    -webkit-backdrop-filter: none !important;
    backdrop-filter: none !important;
    box-shadow: 0 8px 32px rgba(0, 0, 0, 0.5), 0 0 35px 4px var(--color-accent), 0 0 90px 12px var(--color-accent) !important;
  }

  /* Liquid-glass "shine": a light-catching specular highlight on top of the
     existing blur+tint (.glass-surface, app.css), modeled on the two-corner
     inset highlight from https://codepen.io/lassiterda/pen/vEOpqMa. Plain
     box-shadow — no backdrop-filter/SVG-filter interaction, so it renders
     the same regardless of whether the blur itself composites. */
  :global(footer.glass-surface::after) {
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
    transition: border-color 0.2s;
  }

  .volume-slider::-webkit-slider-thumb:hover {
    border-color: var(--color-accent-hover);
  }

  /* Firefox thumb */
  .volume-slider::-moz-range-thumb {
    width: 12px;
    height: 12px;
    border: 2px solid var(--color-accent);
    border-radius: 50%;
    background: #ffffff;
    transition: border-color 0.2s;
  }

  .volume-slider::-moz-range-thumb:hover {
    border-color: var(--color-accent-hover);
  }
</style>
