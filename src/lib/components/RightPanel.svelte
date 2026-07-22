<script lang="ts">
  import { playerStore } from "../stores/player.svelte";
  import { themeStore } from "../stores/theme.svelte";
  import { collectionStore } from "../stores/collection.svelte";
  import { Music, Clock } from "lucide-svelte";
  import { i18n } from "../stores/i18n.svelte";
  import { lyricsStatus } from "../utils/lyrics";

  interface Props {
    isOpen?: boolean;
    width?: number;
    onClose?: () => void;
  }

  let { isOpen = true, width = 288, onClose }: Props = $props();

  let currentSong = $derived(playerStore.currentSong);

  // Loudness normalization (#77) — expanded detail for the right panel
  // (the player bar only has room for a compact "R128"/"RG" badge).
  function loudnessSourceLabel(): string {
    switch (playerStore.loudnessSource) {
      case "analyzed": return i18n.t('playerBar.loudnessSourceAnalyzed', {}, 'R128 analysis');
      case "replay_gain": return i18n.t('playerBar.loudnessSourceReplayGain', {}, 'ReplayGain tag');
      case "fallback": return i18n.t('playerBar.loudnessSourceFallback', {}, 'Fallback gain');
      default: return "";
    }
  }

  let loudnessGainText = $derived.by(() => {
    const gain = playerStore.loudnessGainDb;
    if (gain === undefined) return "";
    return `${gain > 0 ? "+" : ""}${gain.toFixed(1)} dB`;
  });

  function lyricsStatusLabel(): string {
    if (!currentSong) return "";
    switch (lyricsStatus(currentSong)) {
      case "synced": return i18n.t('playerBar.lyricsSynced', {}, 'Synced (LRC)');
      case "plain": return i18n.t('playerBar.lyricsPlain', {}, 'Plain text');
      default: return i18n.t('playerBar.lyricsNone', {}, 'Not downloaded');
    }
  }

  function formatChannels(channels?: number): string {
    if (!channels) return "";
    if (channels === 1) return i18n.t('playerBar.channelsMono', {}, 'Mono');
    if (channels === 2) return i18n.t('playerBar.channelsStereo', {}, 'Stereo');
    if (channels === 6) return i18n.t('playerBar.channels51', {}, '5.1 Surround');
    if (channels === 8) return i18n.t('playerBar.channels71', {}, '7.1 Surround');
    return i18n.t('playerBar.channelsCount', { count: channels }, `${channels} channels`);
  }
</script>

<aside
  style="width: {width}px;"
  class="relative bg-brand-sidebar flex flex-col h-full text-brand-text-secondary select-none flex-shrink-0 overflow-hidden {themeStore.isGlassTheme ? 'glass-surface' : ''}"
>
  <!-- Content -->
  <div class="flex-1 overflow-y-auto px-6 pt-6 pb-6 space-y-6">
    <!-- Current Song -->
    {#if currentSong}
      <div class="space-y-3">
        <div class="space-y-1">
          <p class="text-xs text-brand-text-secondary/60 uppercase tracking-wide">{i18n.t('playerBar.songLabel', {}, 'Song')}</p>
          <p class="text-sm font-semibold text-brand-text-primary break-words">{currentSong.title || i18n.t('collection.unknownSong')}</p>
        </div>

        <div class="space-y-1">
          <p class="text-xs text-brand-text-secondary/60 uppercase tracking-wide">{i18n.t('playerBar.artistLabel', {}, 'Artist')}</p>
          {#if currentSong.artist}
            <button
              onclick={() => collectionStore.viewArtist(currentSong.album_artist?.trim() || currentSong.artist || "")}
              class="text-sm text-brand-text-secondary hover:text-brand-accent-text hover:underline transition-all duration-150 break-words cursor-pointer text-left w-full"
              title={i18n.t('collection.filterByArtist', { artist: currentSong.artist })}
            >
              {currentSong.artist}
            </button>
          {:else}
            <p class="text-sm text-brand-text-secondary break-words">{i18n.t('collection.unknownArtist')}</p>
          {/if}
        </div>

        <div class="space-y-1">
          <p class="text-xs text-brand-text-secondary/60 uppercase tracking-wide">{i18n.t('playerBar.albumLabel', {}, 'Album')}</p>
          <p class="text-sm text-brand-text-secondary break-words">{currentSong.album || i18n.t('collection.unknownAlbum')}</p>
        </div>
      </div>

      <!-- Playback Info -->
      <div class="space-y-2">
        {#if currentSong.filetype}
          <div class="flex items-start justify-between gap-3 text-xs">
            <span class="text-brand-text-secondary/60 shrink-0">{i18n.t('playerBar.formatLabel', {}, 'Format')}</span>
            <span class="text-brand-text-primary uppercase text-right break-words min-w-0">{currentSong.filetype}</span>
          </div>
        {/if}
        {#if currentSong.bitrate}
          <div class="flex items-start justify-between gap-3 text-xs">
            <span class="text-brand-text-secondary/60 shrink-0">{i18n.t('playerBar.bitrateLabel', {}, 'Bitrate')}</span>
            <span class="text-brand-text-primary text-right break-words min-w-0">{currentSong.bitrate} kbps{currentSong.is_vbr ? ` (${i18n.t('playerBar.bitrateVbrSuffix', {}, 'avg')})` : ''}</span>
          </div>
        {/if}
        {#if currentSong.samplerate}
          <div class="flex items-start justify-between gap-3 text-xs">
            <span class="text-brand-text-secondary/60 shrink-0">{i18n.t('playerBar.sampleRateLabel', {}, 'Sample Rate')}</span>
            <span class="text-brand-text-primary text-right break-words min-w-0">{(currentSong.samplerate / 1000).toFixed(1)} kHz</span>
          </div>
        {/if}
        {#if currentSong.channels}
          <div class="flex items-start justify-between gap-3 text-xs">
            <span class="text-brand-text-secondary/60 shrink-0">{i18n.t('playerBar.channelsLabel', {}, 'Channels')}</span>
            <span class="text-brand-text-primary text-right break-words min-w-0">{formatChannels(currentSong.channels)}</span>
          </div>
        {/if}
        {#if playerStore.loudnessSource !== "disabled"}
          <div class="flex items-start justify-between gap-3 text-xs">
            <span class="text-brand-text-secondary/60 shrink-0">{i18n.t('playerBar.loudnessLabel', {}, 'Loudness')}</span>
            <span class="text-brand-text-primary text-right break-words min-w-0">{loudnessSourceLabel()}{loudnessGainText ? ` · ${loudnessGainText}` : ""}</span>
          </div>
        {/if}
        <div class="flex items-start justify-between gap-3 text-xs">
          <span class="text-brand-text-secondary/60 shrink-0">{i18n.t('playerBar.lyricsStatusLabel', {}, 'Lyrics')}</span>
          <span class="text-brand-text-primary text-right break-words min-w-0">{lyricsStatusLabel()}</span>
        </div>
      </div>

      <!-- Additional Metadata -->
      <div class="space-y-2 text-xs">
        {#if currentSong.year}
          <div class="flex items-start justify-between gap-3">
            <span class="text-brand-text-secondary/60 shrink-0">{i18n.t('playerBar.releasedLabel', {}, 'Released')}</span>
            <span class="text-brand-text-secondary text-right break-words min-w-0">{currentSong.year}</span>
          </div>
        {/if}
        {#if currentSong.genre}
          <div class="flex items-start justify-between gap-3">
            <span class="text-brand-text-secondary/60 shrink-0">{i18n.t('playerBar.genreLabel', {}, 'Genre')}</span>
            <span class="text-brand-text-secondary text-right break-words min-w-0">{currentSong.genre}</span>
          </div>
        {/if}
        {#if currentSong.composer}
          <div class="flex items-start justify-between gap-3">
            <span class="text-brand-text-secondary/60 shrink-0">{i18n.t('playerBar.composerLabel', {}, 'Composer')}</span>
            <span class="text-brand-text-secondary text-right break-words min-w-0">{currentSong.composer}</span>
          </div>
        {/if}
      </div>
    {:else}
      <div class="flex flex-col items-center justify-center h-full text-center">
        <Music class="w-12 h-12 text-brand-text-secondary/30 mb-3" />
        <p class="text-sm text-brand-text-secondary/60">{i18n.t('playerBar.notPlaying')}</p>
      </div>
    {/if}
  </div>


</aside>

<style>
  aside {
    scrollbar-width: thin;
    scrollbar-color: var(--color-border) transparent;
  }

  aside ::-webkit-scrollbar {
    width: 6px;
  }

  aside ::-webkit-scrollbar-track {
    background: transparent;
  }

  aside.glass-surface {
    position: relative;
    backdrop-filter: blur(20px) saturate(180%) !important;
    -webkit-backdrop-filter: blur(20px) saturate(180%) !important;
    background-color: var(--glass-bg-sidebar) !important;
    border-color: var(--glass-border-color, var(--color-border)) !important;
    box-shadow: var(--glass-shadow, none);
  }

  aside ::-webkit-scrollbar-thumb {
    background: var(--color-border);
    border-radius: 3px;
  }
</style>
