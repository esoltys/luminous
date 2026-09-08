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
      backend resolves into a release-group. Neither `AlbumItem` nor
      `ArtistProfile` carry a MusicBrainz ID of their own (see #23), so the
      caller passes e.g. `songs[0]?.id`. Album-scoped only: fetched MB
      tags/rating and CritiqueBrainz reviews are correctly release-group-
      scoped here, unlike an artist's whole discography (the artist page
      instead folds the fetched Wikipedia bio into its own local About
      card — see ArtistDetailView.svelte). */
  interface Props {
    songId: number | undefined;
    releaseGroupId?: string;
  }

  let { songId, releaseGroupId }: Props = $props();

  let contextData = $state<SongContextEnrichment | null>(null);

  $effect(() => {
    const id = songId;
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
        // album is opened.
        if (!cancelled) contextData = null;
      });
    return () => {
      cancelled = true;
    };
  });

  let hasContent = $derived(
    !!contextData &&
    (((contextData.mb_tags?.length ?? 0) > 0) || contextData.mb_rating != null || contextData.critiquebrainz_rating != null)
  );
</script>

{#if hasContent && contextData}
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
          {i18n.t('playerBar.mbRatingLabel', {}, 'Community Rating')}:
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
