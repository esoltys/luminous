<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { i18n } from "../stores/i18n.svelte";
  import { openExternalUrl } from "../utils/openExternalUrl";
  import type { SongContextEnrichment } from "../types";
  import {
    ArrowSquareOutIcon as ExternalLink,
    StarIcon as Star
  } from "phosphor-svelte";

  /** `songId` is a representative track — one whose MusicBrainz IDs the
      backend resolves into a release-group (album) or artist ID. Neither
      `AlbumItem` nor `ArtistProfile` carry a MusicBrainz ID of their own
      (see #23), so callers pass e.g. `songs[0]?.id`. */
  interface Props {
    songId: number | undefined;
    releaseGroupId?: string;
    /** "artist" shows only the Wikipedia bio (release-group-scoped fields
        like MB tags/rating would misrepresent an artist's whole discography
        if sourced from just one album). "album" shows MB tags/rating and
        CritiqueBrainz, all correctly release-group-scoped, but not the
        artist's Wikipedia bio (that belongs on the artist page). */
    variant: "artist" | "album";
  }

  let { songId, releaseGroupId, variant }: Props = $props();

  let contextData = $state<SongContextEnrichment | null>(null);
  let bioExpanded = $state(false);

  $effect(() => {
    const id = songId;
    bioExpanded = false;
    if (!id) {
      contextData = null;
      return;
    }
    let cancelled = false;
    invoke<SongContextEnrichment>("get_song_context", { songId: id })
      .then((data) => {
        if (!cancelled) contextData = data;
      })
      .catch(() => {
        // Supplementary content on a browsing page — fail silently rather
        // than surfacing an error banner every time an untagged/offline
        // album or artist is opened.
        if (!cancelled) contextData = null;
      });
    return () => {
      cancelled = true;
    };
  });

  let hasArtistContent = $derived(variant === "artist" && !!contextData?.wikipedia_extract);
  let hasAlbumContent = $derived(
    variant === "album" &&
    !!contextData &&
    (((contextData.mb_tags?.length ?? 0) > 0) || contextData.mb_rating != null || contextData.critiquebrainz_rating != null || (contextData.critiquebrainz_review_links?.length ?? 0) > 0)
  );
</script>

{#if hasArtistContent && contextData}
  <div class="border border-brand-border rounded-xl bg-brand-sidebar/40 backdrop-blur-md p-4 sm:p-5 md:p-6 shadow-xs flex flex-col gap-3">
    {#if contextData.wikipedia_page_url}
      <button
        type="button"
        onclick={() => contextData?.wikipedia_page_url && openExternalUrl(contextData.wikipedia_page_url)}
        class="group relative inline-flex items-center gap-1.5 self-start text-sm font-bold text-brand-text-primary hover:text-brand-accent transition-colors cursor-pointer font-heading"
      >
        <span>{i18n.t('playerBar.wikipediaSectionLabel', {}, 'Wikipedia')}</span>
        <ExternalLink class="w-3.5 h-3.5 opacity-0 group-hover:opacity-100 transition-opacity" />
      </button>
    {:else}
      <h2 class="text-sm font-bold text-brand-text-primary font-heading">{i18n.t('playerBar.wikipediaSectionLabel', {}, 'Wikipedia')}</h2>
    {/if}
    <div class="text-xs text-brand-text-secondary leading-relaxed">
      <p class="{bioExpanded ? '' : 'line-clamp-3'} whitespace-pre-line">{contextData.wikipedia_extract}</p>
      <button type="button" onclick={() => bioExpanded = !bioExpanded} class="mt-1 text-xs font-semibold text-brand-accent hover:underline cursor-pointer">
        {bioExpanded ? i18n.t('playerBar.contextReadLess', {}, 'Read less') : i18n.t('playerBar.contextReadMore', {}, 'Read more')}
      </button>
    </div>
  </div>
{:else if hasAlbumContent && contextData}
  <div class="border border-brand-border rounded-xl bg-brand-sidebar/40 backdrop-blur-md p-4 sm:p-5 md:p-6 shadow-xs flex flex-col gap-3">
    {#if (contextData.mb_tags?.length ?? 0) > 0}
      <div class="flex flex-wrap gap-1.5">
        {#each contextData.mb_tags as tag (tag)}
          <span class="px-2.5 py-1 bg-brand-accent/10 text-brand-text-primary rounded-full text-xs font-medium border border-brand-border/60">{tag}</span>
        {/each}
      </div>
    {/if}
    <div class="flex flex-wrap items-center gap-x-6 gap-y-2 text-xs">
      {#if contextData.mb_rating != null}
        <span class="inline-flex items-center gap-1.5 text-brand-text-secondary">
          <Star class="w-3.5 h-3.5" />
          {i18n.t('playerBar.mbRatingLabel', {}, 'MusicBrainz Rating')}:
          <span class="text-brand-text-primary font-medium">{contextData.mb_rating.toFixed(2)} / 5</span>
        </span>
      {/if}
      {#if contextData.critiquebrainz_rating != null}
        <button
          type="button"
          onclick={() => releaseGroupId && openExternalUrl(`https://critiquebrainz.org/release-group/${releaseGroupId}`)}
          class="group relative inline-flex items-center gap-1.5 text-brand-text-secondary hover:text-brand-accent transition-colors cursor-pointer"
        >
          <img src="/critiquebrainz-logo.svg" alt={i18n.t('playerBar.critiquebrainzSectionLabel', {}, 'CritiqueBrainz')} class="h-3.5 w-auto opacity-80 group-hover:opacity-100 transition-opacity" />
          {i18n.t('playerBar.critiquebrainzRatingLabel', {}, 'Community Rating')}:
          <span class="text-brand-text-primary font-medium group-hover:text-brand-accent">{contextData.critiquebrainz_rating.toFixed(1)} / 5</span>
          <ExternalLink class="w-3 h-3 opacity-0 group-hover:opacity-100 transition-opacity" />
        </button>
      {/if}
    </div>
  </div>
{/if}
