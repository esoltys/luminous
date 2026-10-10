<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { playerStore } from "../stores/player.svelte";
  import { inspectorStore } from "../stores/inspector.svelte";
  import { themeStore } from "../stores/theme.svelte";
  import {
    MusicNotesIcon as Music,
    ClockIcon as Clock,
    ArrowSquareOutIcon as ExternalLink,
    ArrowsClockwiseIcon as RefreshCw,
    CaretDownIcon as CaretDown
  } from "phosphor-svelte";
  import { i18n, formatNumber } from "../stores/i18n.svelte";
  import { prefs } from "../stores/prefs.svelte";
  import { lyricsStatus } from "../utils/lyrics";
  import { openExternalUrl } from "../utils/openExternalUrl";
  import GenreChips from "./GenreChips.svelte";
  import AudioPipelineStages from "./AudioPipelineStages.svelte";
  import InfoSections from "./InfoSections.svelte";
  import { contextStore } from "../stores/context.svelte";

  interface Props {
    isOpen?: boolean;
    width?: number;
    onClose?: () => void;
  }

  let { isOpen = true, width = 288, onClose }: Props = $props();

  // What the panel describes: the selected song, the viewed album/artist, or the
  // playing song (see inspectorStore). "currentSong" below is that subject's song.
  let subject = $derived(inspectorStore.subject);
  let currentSong = $derived(subject?.song);
  let isPlayingSubject = $derived(subject?.source === "playing");
  // An album/artist has no single track, so track-level detail (recording IDs,
  // file path, pipeline) is omitted and only the Information tab applies.
  let isEntitySubject = $derived(subject?.kind === "album" || subject?.kind === "artist");
  let headingLabel = $derived.by(() => {
    switch (subject?.kind === "song" ? subject.source : subject?.kind) {
      case "selection": return i18n.t('playerBar.selectedTrackHeading', {}, 'Selected Track');
      case "album": return i18n.t('playerBar.albumHeading', {}, 'Album');
      case "artist": return i18n.t('playerBar.artistHeading', {}, 'Artist');
      default: return i18n.t('playerBar.nowPlayingHeading', {}, 'Now Playing');
    }
  });
  // Technicals is the default tab until the user's own choice loads from
  // app_state (below) — it's the pre-existing content users already relied
  // on seeing immediately (format/bitrate/etc.).
  let activeTab = $state<"context" | "technical">("technical");
  let effectiveTab = $derived(isEntitySubject ? "context" : activeTab);

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

  // Selection changes far more often than tracks do (arrowing through rows); the store debounces
  // its fetches. Events are only worth fetching for a song while the Information tab is showing.
  const info = contextStore.for(() => subject ?? null, { songEvents: () => effectiveTab === "context" });
  let contextData = $derived(info.context);

  // Per-track DR/Peak/RMS parsed from a foobar2000 foo_dr.txt log (#57) —
  // shown alongside Loudness since Peak/RMS feed that gain calculation as a
  // last-resort fallback source.
  let dynamicRangeText = $derived.by(() => {
    if (!currentSong?.dynamic_range) return "";
    const parts = [`DR${currentSong.dynamic_range}`];
    if (currentSong.dynamic_range_peak != null) {
      const peakFormatted = formatNumber(currentSong.dynamic_range_peak, { minimumFractionDigits: 1, maximumFractionDigits: 1 });
      parts.push(i18n.t('playerBar.dynamicRangePeak', { value: peakFormatted }, `Peak ${peakFormatted} dB`));
    }
    if (currentSong.dynamic_range_rms != null) {
      const rmsFormatted = formatNumber(currentSong.dynamic_range_rms, { minimumFractionDigits: 1, maximumFractionDigits: 1 });
      parts.push(i18n.t('playerBar.dynamicRangeRms', { value: rmsFormatted }, `RMS ${rmsFormatted} dB`));
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
    if (!currentSong || isEntitySubject) return [];
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
    if (!currentSong || isEntitySubject) return [];
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
      <div class="flex items-center justify-between gap-3">
        <h2 class="text-xs font-bold text-brand-text-secondary uppercase tracking-wider">
          {headingLabel}
        </h2>
        <!-- The panel follows what you look at; this is the way back to what is playing. -->
        {#if !isPlayingSubject && playerStore.currentSong}
          <button
            type="button"
            onclick={() => inspectorStore.showPlaying()}
            class="inline-flex items-center gap-1 px-2 py-0.5 rounded-full border border-brand-border text-xs font-medium text-brand-text-secondary hover:text-brand-text-primary hover:border-brand-accent/40 transition-colors cursor-pointer"
          >
            <Music class="w-3 h-3" />
            <span>{i18n.t('playerBar.nowPlayingHeading', {}, 'Now Playing')}</span>
          </button>
        {/if}
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

      <div class="flex gap-1 p-1 rounded-lg bg-brand-bg/40 text-xs font-semibold">
        <button
          type="button"
          onclick={() => setActiveTab("context")}
          class="flex-1 px-3 py-1.5 rounded-md transition-all {effectiveTab === 'context' ? 'bg-brand-accent text-brand-accent-contrast shadow-md' : 'text-brand-text-secondary hover:text-brand-text-primary'}"
        >
          {i18n.t('playerBar.tabContextBio', {}, 'Information')}
        </button>
        {#if !isEntitySubject}
        <button
          type="button"
          onclick={() => setActiveTab("technical")}
          class="flex-1 px-3 py-1.5 rounded-md transition-all {effectiveTab === 'technical' ? 'bg-brand-accent text-brand-accent-contrast shadow-md' : 'text-brand-text-secondary hover:text-brand-text-primary'}"
        >
          {i18n.t('playerBar.tabAudioTechnicals', {}, 'Technical')}
        </button>
        {/if}
      </div>

      {#if effectiveTab === "context"}
        <InfoSections view={info} entity={isEntitySubject} />
      {:else}
        <div class="space-y-3">
          {#if isPlayingSubject}
            <AudioPipelineStages pipeline={playerStore.audioPipeline} class="pb-3 border-b border-brand-border/40" />
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

          {#if musicbrainzRows.length > 0 || musicbrainzMetaRows.length > 0 || (contextData?.mb_tags?.length ?? 0) > 0 || contextData?.mb_rating != null}
            <div class="space-y-2 text-xs pt-4 border-t border-brand-border/40">
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
                    {formatNumber(contextData.mb_rating, { minimumFractionDigits: 2, maximumFractionDigits: 2 })} / 5
                    {#if contextData.mb_rating_votes}
                      <span class="text-brand-text-secondary/60">{i18n.plural("playerBar.mbRatingVotes", contextData.mb_rating_votes)}</span>
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
                    <span class="text-brand-text-primary underline decoration-brand-text-secondary/40 break-words transition-colors">{row.name || row.id}</span>
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

  /* Nothing ever renders behind this panel but the flat bg-main canvas,
     so it paints the glass result as a solid color instead of running a
     backdrop-filter (see flatGlassColor() in theme.svelte.ts). */
  aside.glass-surface {
    position: relative;
    -webkit-backdrop-filter: none !important;
    backdrop-filter: none !important;
    background-color: var(--glass-solid-sidebar) !important;
    border-color: var(--glass-border-color, var(--color-border)) !important;
    box-shadow: var(--glass-shadow, none);
  }

  aside ::-webkit-scrollbar-thumb {
    background: var(--color-border);
    border-radius: 3px;
  }
</style>
