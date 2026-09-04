<script lang="ts">
  import { playerStore } from "../stores/player.svelte";
  import { themeStore } from "../stores/theme.svelte";
  import {
    MusicNotesIcon as Music,
    ClockIcon as Clock,
    MicrophoneStageIcon as ArtistIcon,
    DiscIcon as ReleaseIcon
  } from "phosphor-svelte";
  import { i18n } from "../stores/i18n.svelte";
  import { lyricsStatus } from "../utils/lyrics";
  import { openExternalUrl } from "../utils/openExternalUrl";
  import GenreChips from "./GenreChips.svelte";

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

  const musicbrainzRows = $derived.by(() => {
    if (!currentSong) return [];
    const entries: { label: string; id?: string; entityPath: string; name?: string; icon: typeof ArtistIcon }[] = [
      { label: i18n.t('playerBar.musicbrainzArtistLabel', {}, 'Artist'), id: currentSong.musicbrainz_artist_id, entityPath: "artist", name: currentSong.artist, icon: ArtistIcon },
      // Skip Album Artist when it's the same MusicBrainz entity as Artist (the common case for a
      // non-compilation release) — showing the identical name/link twice is just noise.
      ...(currentSong.musicbrainz_album_artist_id && currentSong.musicbrainz_album_artist_id !== currentSong.musicbrainz_artist_id
        ? [{ label: i18n.t('playerBar.musicbrainzAlbumArtistLabel', {}, 'Album Artist'), id: currentSong.musicbrainz_album_artist_id, entityPath: "artist", name: currentSong.album_artist, icon: ArtistIcon }]
        : []),
      { label: i18n.t('playerBar.musicbrainzReleaseLabel', {}, 'Release'), id: currentSong.musicbrainz_album_id, entityPath: "release", name: currentSong.album, icon: ReleaseIcon },
      { label: i18n.t('playerBar.musicbrainzReleaseGroupLabel', {}, 'Release Group'), id: currentSong.musicbrainz_release_group_id, entityPath: "release-group", name: currentSong.album, icon: ReleaseIcon },
      { label: i18n.t('playerBar.musicbrainzRecordingLabel', {}, 'Recording'), id: currentSong.musicbrainz_recording_id, entityPath: "recording", name: currentSong.title, icon: Music },
      { label: i18n.t('playerBar.musicbrainzTrackLabel', {}, 'Track'), id: currentSong.musicbrainz_track_id, entityPath: "track", name: currentSong.title, icon: Music },
      { label: i18n.t('playerBar.musicbrainzWorkLabel', {}, 'Work'), id: currentSong.musicbrainz_work_id, entityPath: "work", name: currentSong.title, icon: Music },
    ];
    return entries.filter((e): e is typeof entries[number] & { id: string } => !!e.id);
  });

  /** Descriptive release metadata Picard writes alongside the MusicBrainz
      IDs, but not IDs themselves — no entity page to link to, so these
      render as plain text rows rather than clickable rows like `musicbrainzRows`. */
  const musicbrainzMetaRows = $derived.by(() => {
    if (!currentSong) return [];
    const entries: { label: string; value?: string }[] = [
      {
        label: i18n.t('playerBar.musicbrainzReleaseTypeLabel', {}, 'Type'),
        value: currentSong.musicbrainz_release_type
          ? currentSong.musicbrainz_release_type.charAt(0).toUpperCase() + currentSong.musicbrainz_release_type.slice(1)
          : undefined,
      },
      { label: i18n.t('playerBar.musicbrainzReleaseCountryLabel', {}, 'Country'), value: currentSong.musicbrainz_release_country },
      { label: i18n.t('playerBar.barcodeLabel', {}, 'Barcode'), value: currentSong.barcode },
      { label: i18n.t('playerBar.catalogNumberLabel', {}, 'Catalog #'), value: currentSong.catalog_number },
    ];
    return entries.filter((e): e is { label: string; value: string } => !!e.value);
  });

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
  <!-- The floating PlayerBar dock (h-20 + bottom-4 inset = 96px = mb-24) overlays
       the bottom of the app on top of this panel — a bottom *margin* (rather than
       inner padding) actually shrinks this div's own box, so its scrollbar ends
       above the dock instead of running the full sidebar height behind it. -->
  <div class="flex-1 min-h-0 overflow-y-auto px-6 pt-6 pb-6 space-y-6 {currentSong ? 'mb-24' : ''}">
    {#if currentSong}
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
            <div class="min-w-0 flex justify-end">
              <GenreChips genre={currentSong.genre} variant="full" />
            </div>
          </div>
        {/if}
        {#if currentSong.composer}
          <div class="flex items-start justify-between gap-3">
            <span class="text-brand-text-secondary/60 shrink-0">{i18n.t('playerBar.composerLabel', {}, 'Composer')}</span>
            <span class="text-brand-text-secondary text-right break-words min-w-0">{currentSong.composer}</span>
          </div>
        {/if}
      </div>

      {#if musicbrainzRows.length > 0 || musicbrainzMetaRows.length > 0}
        <div class="space-y-2 text-xs">
          <img src="/musicbrainz-logo.svg" alt={i18n.t('playerBar.musicbrainzSectionLabel', {}, 'MusicBrainz')} class="h-3.5 w-auto" />
          {#each musicbrainzRows as row (row.label)}
            <div class="flex items-start justify-between gap-3">
              <span class="text-brand-text-secondary/60 shrink-0">{row.label}</span>
              <button
                type="button"
                onclick={() => openExternalUrl(`https://musicbrainz.org/${row.entityPath}/${row.id}`)}
                class="flex items-center gap-1 text-brand-accent hover:underline text-right break-words min-w-0"
              >
                <row.icon size={12} class="shrink-0" />
                <span>{row.name || row.id}</span>
              </button>
            </div>
          {/each}
          {#each musicbrainzMetaRows as row (row.label)}
            <div class="flex items-start justify-between gap-3">
              <span class="text-brand-text-secondary/60 shrink-0">{row.label}</span>
              <span class="text-brand-text-secondary text-right break-words min-w-0">{row.value}</span>
            </div>
          {/each}
        </div>
      {/if}
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
    -webkit-backdrop-filter: blur(20px) saturate(180%) !important;
    backdrop-filter: blur(20px) saturate(180%) !important;
    background-color: var(--glass-bg-sidebar) !important;
    border-color: var(--glass-border-color, var(--color-border)) !important;
    box-shadow: var(--glass-shadow, none);
  }

  aside ::-webkit-scrollbar-thumb {
    background: var(--color-border);
    border-radius: 3px;
  }
</style>
