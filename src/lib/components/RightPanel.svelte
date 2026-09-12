<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { playerStore } from "../stores/player.svelte";
  import { themeStore } from "../stores/theme.svelte";
  import {
    MusicNotesIcon as Music,
    ClockIcon as Clock,
    ArrowSquareOutIcon as ExternalLink,
    ArrowsClockwiseIcon as RefreshCw
  } from "phosphor-svelte";
  import { i18n } from "../stores/i18n.svelte";
  import { lyricsStatus } from "../utils/lyrics";
  import { openExternalUrl } from "../utils/openExternalUrl";
  import GenreChips from "./GenreChips.svelte";
  import type { SongContextEnrichment } from "../types";

  interface Props {
    isOpen?: boolean;
    width?: number;
    onClose?: () => void;
  }

  let { isOpen = true, width = 288, onClose }: Props = $props();

  let currentSong = $derived(playerStore.currentSong);
  // Technicals is the default tab until the user's own choice loads from
  // app_state (below) — it's the pre-existing content users already relied
  // on seeing immediately (format/bitrate/etc.).
  let activeTab = $state<"context" | "technical">("technical");

  function setActiveTab(tab: "context" | "technical") {
    activeTab = tab;
    invoke("set_app_setting", { key: "right_panel_active_tab", value: tab });
  }

  $effect(() => {
    invoke<Record<string, string>>("get_all_app_settings")
      .then((settings) => {
        const saved = settings?.right_panel_active_tab;
        if (saved === "context" || saved === "technical") activeTab = saved;
      })
      .catch(() => {});
  });

  let contextData = $state<SongContextEnrichment | null>(null);
  let isLoadingContext = $state(false);
  let contextErrorMsg = $state("");
  let bioExpanded = $state(false);
  let bioParagraphEl = $state<HTMLParagraphElement | undefined>();
  let bioIsTruncated = $state(false);

  // Only show "Read more" when the clamp actually hides text — measured
  // once per bio while still clamped (bioExpanded resets to false on every
  // song/tab load), since a fixed character threshold would be wrong for
  // this panel's user-resizable width.
  $effect(() => {
    const text = contextData?.wikipedia_extract;
    const el = bioParagraphEl;
    if (!text || !el) {
      bioIsTruncated = false;
      return;
    }
    bioIsTruncated = el.scrollHeight > el.clientHeight + 1;
  });

  // A request-id guard, since switching tracks quickly can otherwise let an
  // earlier, slower fetch resolve after a newer one and overwrite
  // contextData with stale data for a track the user has already left.
  let contextRequestId = 0;

  async function loadContext(songId: number | undefined, forceRefresh = false) {
    bioExpanded = false;
    const requestId = ++contextRequestId;
    if (!songId) {
      contextData = null;
      contextErrorMsg = "";
      return;
    }
    isLoadingContext = true;
    contextErrorMsg = "";
    try {
      const data = await invoke<SongContextEnrichment>("get_song_context", { songId, forceRefresh });
      if (requestId !== contextRequestId) return;
      contextData = data;
    } catch (e) {
      if (requestId !== contextRequestId) return;
      contextErrorMsg = e instanceof Error ? e.message : String(e);
    } finally {
      if (requestId === contextRequestId) isLoadingContext = false;
    }
  }

  $effect(() => {
    loadContext(currentSong?.id);
  });

  let hasContextData = $derived.by(() => {
    if (!contextData) return false;
    return !!(
      contextData.wikipedia_extract ||
      (contextData.mb_tags?.length ?? 0) > 0 ||
      contextData.mb_rating != null ||
      contextData.critiquebrainz_rating != null ||
      (contextData.critiquebrainz_review_links?.length ?? 0) > 0
    );
  });

  // Loudness normalization (#77) — expanded detail for the right panel
  // (the player bar only has room for a compact "R128"/"RG" badge).
  function loudnessSourceLabel(): string {
    switch (playerStore.loudnessSource) {
      case "analyzed": return i18n.t('playerBar.loudnessSourceAnalyzed', {}, 'R128 analysis');
      case "replay_gain": return i18n.t('playerBar.loudnessSourceReplayGain', {}, 'ReplayGain tag');
      case "dynamic_range_log": return i18n.t('playerBar.loudnessSourceDynamicRangeLog', {}, 'DR Log');
      case "fallback": return i18n.t('playerBar.loudnessSourceFallback', {}, 'Fallback gain');
      default: return "";
    }
  }

  let loudnessGainText = $derived.by(() => {
    const gain = playerStore.loudnessGainDb;
    if (gain === undefined) return "";
    return `${gain > 0 ? "+" : ""}${gain.toFixed(1)} dB`;
  });

  // Per-track DR/Peak/RMS parsed from a foobar2000 foo_dr.txt log (#57) —
  // shown alongside Loudness since Peak/RMS feed that gain calculation as a
  // last-resort fallback source.
  let dynamicRangeText = $derived.by(() => {
    if (!currentSong?.dynamic_range) return "";
    const parts = [`DR${currentSong.dynamic_range}`];
    if (currentSong.dynamic_range_peak != null) {
      parts.push(i18n.t('playerBar.dynamicRangePeak', { value: currentSong.dynamic_range_peak.toFixed(1) }, `Peak ${currentSong.dynamic_range_peak.toFixed(1)} dB`));
    }
    if (currentSong.dynamic_range_rms != null) {
      parts.push(i18n.t('playerBar.dynamicRangeRms', { value: currentSong.dynamic_range_rms.toFixed(1) }, `RMS ${currentSong.dynamic_range_rms.toFixed(1)} dB`));
    }
    return parts.join(" · ");
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
    const entries: { label: string; id?: string; entityPath: string; name?: string }[] = [
      { label: i18n.t('playerBar.musicbrainzArtistLabel', {}, 'Artist'), id: currentSong.musicbrainz_artist_id, entityPath: "artist", name: currentSong.artist },
      // Skip Album Artist when it's the same MusicBrainz entity as Artist (the common case for a
      // non-compilation release) — showing the identical name/link twice is just noise.
      ...(currentSong.musicbrainz_album_artist_id && currentSong.musicbrainz_album_artist_id !== currentSong.musicbrainz_artist_id
        ? [{ label: i18n.t('playerBar.musicbrainzAlbumArtistLabel', {}, 'Album Artist'), id: currentSong.musicbrainz_album_artist_id, entityPath: "artist", name: currentSong.album_artist }]
        : []),
      { label: i18n.t('playerBar.musicbrainzReleaseLabel', {}, 'Release'), id: currentSong.musicbrainz_album_id, entityPath: "release", name: currentSong.album },
      { label: i18n.t('playerBar.musicbrainzReleaseGroupLabel', {}, 'Release Group'), id: currentSong.musicbrainz_release_group_id, entityPath: "release-group", name: currentSong.album },
      { label: i18n.t('playerBar.musicbrainzRecordingLabel', {}, 'Recording'), id: currentSong.musicbrainz_recording_id, entityPath: "recording", name: currentSong.title },
      { label: i18n.t('playerBar.musicbrainzTrackLabel', {}, 'Track'), id: currentSong.musicbrainz_track_id, entityPath: "track", name: currentSong.title },
      { label: i18n.t('playerBar.musicbrainzWorkLabel', {}, 'Work'), id: currentSong.musicbrainz_work_id, entityPath: "work", name: currentSong.title },
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
  data-walkthrough-target="right-panel"
  style="width: {width}px;"
  class="relative bg-brand-sidebar flex flex-col h-full text-brand-text-secondary select-none flex-shrink-0 overflow-hidden {themeStore.isGlassTheme ? 'glass-surface' : ''}"
>
  <!-- The floating PlayerBar dock (h-20 + bottom-4 inset = 96px = mb-24) overlays
       the bottom of the app on top of this panel — a bottom *margin* (rather than
       inner padding) actually shrinks this div's own box, so its scrollbar ends
       above the dock instead of running the full sidebar height behind it. -->
  <div class="flex-1 min-h-0 overflow-y-auto px-6 pt-6 pb-6 space-y-6 {currentSong ? 'mb-24' : ''}">
    {#if currentSong}
      <h2 class="text-xs font-bold text-brand-text-secondary uppercase tracking-wider">
        {i18n.t('playerBar.nowPlayingHeading', {}, 'Now Playing')}
      </h2>

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

      <div class="flex gap-1 p-1 rounded-lg bg-brand-bg/40 text-xs font-semibold">
        <button
          type="button"
          onclick={() => setActiveTab("context")}
          class="flex-1 px-3 py-1.5 rounded-md transition-all {activeTab === 'context' ? 'bg-brand-accent text-brand-accent-contrast shadow-md' : 'text-brand-text-secondary hover:text-brand-text-primary'}"
        >
          {i18n.t('playerBar.tabContextBio', {}, 'Information')}
        </button>
        <button
          type="button"
          onclick={() => setActiveTab("technical")}
          class="flex-1 px-3 py-1.5 rounded-md transition-all {activeTab === 'technical' ? 'bg-brand-accent text-brand-accent-contrast shadow-md' : 'text-brand-text-secondary hover:text-brand-text-primary'}"
        >
          {i18n.t('playerBar.tabAudioTechnicals', {}, 'Technical')}
        </button>
      </div>

      {#if activeTab === "context"}
        {#if isLoadingContext}
          <div class="flex items-center gap-2 text-xs text-brand-text-secondary/60 py-2">
            <RefreshCw class="w-3.5 h-3.5 animate-spin" />
            <span>{i18n.t('playerBar.contextLoading', {}, 'Fetching context…')}</span>
          </div>
        {:else if contextErrorMsg}
          <div class="space-y-2 py-2">
            <p class="text-xs text-brand-text-secondary/60">{i18n.t('playerBar.contextFetchError', {}, "Couldn't fetch context data. Check your connection and retry.")}</p>
            <button
              type="button"
              onclick={() => loadContext(currentSong?.id, true)}
              class="text-xs text-brand-accent hover:underline"
            >
              {i18n.t('playerBar.contextRetry', {}, 'Retry')}
            </button>
          </div>
        {:else}
          {#if contextData?.wikipedia_extract}
            <div class="space-y-1.5 text-xs">
              {#if contextData.wikipedia_page_url}
                <button
                  type="button"
                  onclick={() => contextData?.wikipedia_page_url && openExternalUrl(contextData.wikipedia_page_url)}
                  class="group relative inline-flex items-center gap-1 text-brand-text-secondary/60 hover:text-brand-accent transition-colors cursor-pointer"
                >
                  <span class="underline decoration-brand-text-secondary/40 group-hover:decoration-brand-accent transition-colors">{i18n.t('playerBar.wikipediaSectionLabel', {}, 'Wikipedia')}</span>
                  <ExternalLink class="w-3 h-3 opacity-0 group-hover:opacity-100 transition-opacity" />
                </button>
              {:else}
                <span class="text-brand-text-secondary/60">{i18n.t('playerBar.wikipediaSectionLabel', {}, 'Wikipedia')}</span>
              {/if}
              <p bind:this={bioParagraphEl} class="text-brand-text-secondary leading-relaxed {bioExpanded ? '' : 'line-clamp-3'}">{contextData.wikipedia_extract}</p>
              {#if bioIsTruncated || bioExpanded}
                <button type="button" onclick={() => bioExpanded = !bioExpanded} class="text-brand-accent hover:underline">
                  {bioExpanded ? i18n.t('playerBar.contextReadLess', {}, 'Read less') : i18n.t('playerBar.contextReadMore', {}, 'Read more')}
                </button>
              {/if}
            </div>
          {/if}

          {#if musicbrainzRows.length > 0 || musicbrainzMetaRows.length > 0 || (contextData?.mb_tags?.length ?? 0) > 0 || contextData?.mb_rating != null}
            <div class="space-y-2 text-xs">
              <img src="/musicbrainz-logo.svg" alt={i18n.t('playerBar.musicbrainzSectionLabel', {}, 'MusicBrainz')} class="h-3.5 w-auto" />

              {#if contextData && (contextData.mb_tags?.length ?? 0) > 0}
                <div class="space-y-1.5">
                  <span class="text-brand-text-secondary/60">{i18n.t('playerBar.mbTagsSectionLabel', {}, 'Community Tags')}:</span>
                  <div class="flex flex-wrap gap-x-1.5 gap-y-1 leading-relaxed">
                    {#each contextData.mb_tags as tag (tag)}
                      <span class="px-2 py-0.5 rounded-full bg-brand-bg/60 text-brand-text-secondary text-[11px]">{tag}</span>
                    {/each}
                  </div>
                </div>
              {/if}

              {#if contextData?.mb_rating != null}
                <div class="flex items-start justify-between gap-3">
                  <span class="text-brand-text-secondary/60 shrink-0">{i18n.t('playerBar.mbRatingLabel', {}, 'Community Rating')}</span>
                  <span class="text-brand-text-primary text-right">
                    {contextData.mb_rating.toFixed(2)} / 5
                    {#if contextData.mb_rating_votes}
                      <span class="text-brand-text-secondary/60">{i18n.t('playerBar.mbRatingVotes', { count: contextData.mb_rating_votes }, `(${contextData.mb_rating_votes} votes)`)}</span>
                    {/if}
                  </span>
                </div>
              {/if}

              {#each musicbrainzRows as row (row.label)}
                <div class="flex items-start justify-between gap-3">
                  <span class="text-brand-text-secondary/60 shrink-0">{row.label}</span>
                  <button
                    type="button"
                    onclick={() => openExternalUrl(`https://musicbrainz.org/${row.entityPath}/${row.id}`)}
                    class="group relative text-right transition-colors cursor-pointer min-w-0"
                  >
                    <span class="text-brand-text-primary group-hover:text-brand-accent underline decoration-brand-text-secondary/40 break-words transition-colors">{row.name || row.id}</span>
                    <ExternalLink class="absolute -right-4 top-1/2 -translate-y-1/2 w-3 h-3 text-brand-text-secondary opacity-0 group-hover:opacity-100 transition-opacity" />
                  </button>
                </div>
              {/each}
              {#each musicbrainzMetaRows as row (row.label)}
                <div class="flex items-start justify-between gap-3">
                  <span class="text-brand-text-secondary/60 shrink-0">{row.label}</span>
                  <span class="text-brand-text-primary text-right break-words min-w-0">{row.value}</span>
                </div>
              {/each}
            </div>
          {/if}

          {#if contextData?.critiquebrainz_rating != null || (contextData?.critiquebrainz_review_links?.length ?? 0) > 0}
            <div class="space-y-1.5 text-xs">
              {#if currentSong.musicbrainz_release_group_id}
                <button
                  type="button"
                  onclick={() => openExternalUrl(`https://critiquebrainz.org/release-group/${currentSong.musicbrainz_release_group_id}`)}
                  class="group relative inline-flex items-center gap-1 cursor-pointer"
                >
                  <img src="/critiquebrainz-logo.svg" alt={i18n.t('playerBar.critiquebrainzSectionLabel', {}, 'CritiqueBrainz')} class="h-5 w-auto opacity-80 group-hover:opacity-100 transition-opacity" />
                  <ExternalLink class="w-3 h-3 text-brand-text-secondary opacity-0 group-hover:opacity-100 transition-opacity" />
                </button>
              {:else}
                <img src="/critiquebrainz-logo.svg" alt={i18n.t('playerBar.critiquebrainzSectionLabel', {}, 'CritiqueBrainz')} class="h-5 w-auto opacity-80" />
              {/if}
              {#if contextData?.critiquebrainz_rating != null}
                <div class="flex items-start justify-between gap-3">
                  <span class="text-brand-text-secondary/60 shrink-0">{i18n.t('playerBar.critiquebrainzRatingLabel', {}, 'Community Rating')}</span>
                  <span class="text-brand-text-primary text-right">{contextData.critiquebrainz_rating.toFixed(1)} / 5</span>
                </div>
              {/if}
              {#each contextData?.critiquebrainz_review_links ?? [] as link, i (link)}
                <button
                  type="button"
                  onclick={() => openExternalUrl(link)}
                  class="group relative text-brand-accent hover:underline transition-colors cursor-pointer block"
                >
                  {i18n.t('playerBar.critiquebrainzReviewsLabel', {}, 'Review')} {i + 1}
                </button>
              {/each}
            </div>
          {/if}

          {#if !hasContextData && musicbrainzRows.length === 0 && musicbrainzMetaRows.length === 0}
            <div class="text-xs text-brand-text-secondary/60 py-2">
              {i18n.t('playerBar.contextEmptyState', {}, 'No enrichment data available for this track.')}
            </div>
          {/if}
        {/if}
      {:else}
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
          {#if currentSong.dynamic_range != null}
            <div class="flex items-start justify-between gap-3 text-xs">
              <span class="text-brand-text-secondary/60 shrink-0">{i18n.t('playerBar.dynamicRangeLabel', {}, 'Dynamic Range')}</span>
              <span class="text-brand-text-primary text-right break-words min-w-0">{dynamicRangeText}</span>
            </div>
          {/if}
          <div class="flex items-start justify-between gap-3 text-xs">
            <span class="text-brand-text-secondary/60 shrink-0">{i18n.t('playerBar.lyricsStatusLabel', {}, 'Lyrics')}</span>
            <span class="text-brand-text-primary text-right break-words min-w-0">{lyricsStatusLabel()}</span>
          </div>
          {#if currentSong.path}
            <div class="space-y-1 text-xs">
              <span class="text-brand-text-secondary/60">{i18n.t('playerBar.filePathLabel', {}, 'File Path')}:</span>
              <p class="text-brand-text-primary text-left break-words">{currentSong.path}</p>
            </div>
          {/if}
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
