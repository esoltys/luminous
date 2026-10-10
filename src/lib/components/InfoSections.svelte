<script lang="ts">
  import { ArrowSquareOutIcon as ExternalLink, ArrowsClockwiseIcon as RefreshCw, CaretDownIcon as CaretDown } from "phosphor-svelte";
  import { i18n } from "../stores/i18n.svelte";
  import type { ContextView } from "../stores/context.svelte";
  import { openExternalUrl } from "../utils/openExternalUrl";
  import ArtistInformationPanel from "./ArtistInformationPanel.svelte";
  import ArtistEventsSection from "./ArtistEventsSection.svelte";
  import CommunityRating from "./CommunityRating.svelte";
  import MarkdownBio from "./MarkdownBio.svelte";
  import SocialIcon from "./SocialIcon.svelte";

  interface Props {
    /** What to render; see `contextStore.for()`. */
    view: ContextView;
    /** Wording of the empty state: an album/artist has no single track. */
    entity?: boolean;
    /** Skip the CritiqueBrainz block when the host already shows the rating (e.g. in its title). */
    hideRating?: boolean;
  }

  let { view, entity = false, hideRating = false }: Props = $props();

  let sections = $derived(view.sections);
</script>

<!-- A container, not a viewport query: this sits in the 288px sidebar and in the wide center pane. -->
<div class="@container space-y-6 text-xs">
  {#if view.offline}
    <p class="text-brand-text-secondary/60">{i18n.t('playerBar.contextOffline')}</p>
  {/if}

  {#each sections as section (section.id)}
    {#if section.id === "facts"}
      <ArtistInformationPanel
        sortName={section.context.artist_sort_name}
        gender={section.context.artist_gender}
        beginDate={section.context.artist_begin_date}
        endDate={section.context.artist_end_date}
        ended={section.context.artist_ended}
        artistType={section.context.artist_type}
        beginAreaName={section.context.artist_begin_area_name}
        beginAreaMbid={section.context.artist_begin_area_mbid}
        areaName={section.context.artist_area_name}
        areaMbid={section.context.artist_area_mbid}
        variant="card"
      />
    {:else if section.id === "bio"}
      {#if section.source === "wikipedia"}
        <details open class="group border border-brand-border/60 rounded-lg bg-brand-sidebar/40 overflow-hidden">
          <summary class="flex items-center justify-between px-3 py-2 text-xs font-semibold text-brand-text-secondary cursor-pointer select-none hover:text-brand-text-primary transition-colors">
            <div class="flex items-center gap-1 min-w-0">
              <span>{i18n.t('playerBar.wikipediaSectionLabel', {}, 'Wikipedia')}</span>
              {#if section.wikipediaUrl}
                {@const url = section.wikipediaUrl}
                <button
                  type="button"
                  onclick={(e) => {
                    e.stopPropagation();
                    openExternalUrl(url);
                  }}
                  class="inline-flex items-center text-brand-text-secondary/60 hover:text-brand-accent transition-colors ml-0.5 p-0.5"
                  title={i18n.t('playerBar.wikipediaSectionLabel', {}, 'Wikipedia')}
                >
                  <ExternalLink class="w-3 h-3" />
                </button>
              {/if}
            </div>
            <CaretDown class="w-3.5 h-3.5 text-brand-text-secondary/70 group-open:rotate-180 transition-transform shrink-0" />
          </summary>
          <div class="px-3 pb-3 pt-1 border-t border-brand-border/40">
            <p class="text-brand-text-secondary leading-relaxed whitespace-pre-line">{section.text}</p>
          </div>
        </details>
      {:else}
        <div class="text-brand-text-secondary leading-relaxed">
          <MarkdownBio text={section.text} disableClamp={true} />
        </div>
      {/if}
    {:else if section.id === "events"}
      <ArtistEventsSection
        events={section.events}
        loading={section.loading}
        artistName={section.artistName}
        songkickUrl={section.songkickUrl}
        setlistfmUrl={section.setlistfmUrl}
        bandsintownUrl={section.bandsintownUrl}
        musicbrainzUrl={section.musicbrainzUrl}
        onOpenUrl={openExternalUrl}
      />
    {:else if section.id === "links"}
      <div class="grid grid-cols-1 @sm:grid-cols-2 @2xl:grid-cols-3 gap-2.5">
        {#each section.items as item (item.key)}
          <button
            type="button"
            onclick={() => openExternalUrl(item.url)}
            title={item.url}
            class="flex items-center gap-2.5 group/link text-left transition-colors cursor-pointer min-w-0"
          >
            <div class="w-7 h-7 rounded-full bg-brand-main/60 {item.isOfficial ? 'border-[3px]' : 'border'} border-brand-border flex items-center justify-center text-brand-text-secondary group-hover/link:text-brand-accent group-hover/link:border-brand-accent/40 transition-colors shrink-0 shadow-2xs">
              <SocialIcon platform={item.platform} size={14} />
            </div>
            <div class="flex items-center gap-1 min-w-0 flex-1">
              <span class="text-xs font-medium text-brand-text-primary truncate transition-colors">{item.label}</span>
              <ExternalLink class="w-3 h-3 text-brand-text-secondary opacity-0 group-hover/link:opacity-100 transition-opacity shrink-0" />
            </div>
          </button>
        {/each}
      </div>
    {:else if section.id === "external"}
      {#if section.listenbrainz}
        {@const lb = section.listenbrainz}
        <div class="space-y-2">
          <button type="button" onclick={() => openExternalUrl(lb.url)} class="group relative inline-flex items-center gap-1 cursor-pointer">
            <img src="/listenbrainz-logo.png" alt={i18n.t('playerBar.listenbrainzSectionLabel', {}, 'ListenBrainz')} class="h-4.5 w-auto opacity-80 group-hover:opacity-100 transition-opacity" />
            <ExternalLink class="w-3 h-3 text-brand-text-secondary opacity-0 group-hover:opacity-100 transition-opacity" />
          </button>
          {#each lb.rows as row (row.label)}
            <div class="flex items-start justify-between gap-3">
              <span class="text-brand-text-secondary/60 shrink-0">{row.label}</span>
              <button type="button" onclick={() => openExternalUrl(row.url)} class="group relative text-right transition-colors cursor-pointer min-w-0">
                <span class="text-brand-text-primary underline decoration-brand-text-secondary/40 break-words transition-colors">{row.name || row.id}</span>
                <ExternalLink class="absolute -right-4 top-1/2 -translate-y-1/2 w-3 h-3 text-brand-text-secondary opacity-0 group-hover:opacity-100 transition-opacity" />
              </button>
            </div>
          {/each}
        </div>
      {/if}
      {#if section.critiquebrainz && !hideRating}
        {@const cb = section.critiquebrainz}
        <div class="space-y-1.5">
          {#if cb.releaseGroupMbid}
            <button
              type="button"
              onclick={() => openExternalUrl(`https://critiquebrainz.org/release-group/${cb.releaseGroupMbid}`)}
              class="group relative inline-flex items-center gap-1 cursor-pointer"
            >
              <img src="/critiquebrainz-logo.svg" alt={i18n.t('playerBar.critiquebrainzSectionLabel', {}, 'CritiqueBrainz')} class="h-5 w-auto opacity-80 group-hover:opacity-100 transition-opacity" />
              <ExternalLink class="w-3 h-3 text-brand-text-secondary opacity-0 group-hover:opacity-100 transition-opacity" />
            </button>
          {:else}
            <img src="/critiquebrainz-logo.svg" alt={i18n.t('playerBar.critiquebrainzSectionLabel', {}, 'CritiqueBrainz')} class="h-5 w-auto opacity-80" />
          {/if}
          <div class="text-brand-text-secondary">
            <CommunityRating rating={cb.rating} count={cb.count} releaseGroupMbid={cb.releaseGroupMbid} />
          </div>
        </div>
      {/if}
    {/if}
  {/each}

  {#if view.loading}
    <div class="flex items-center gap-2 text-brand-text-secondary/60">
      <RefreshCw class="w-3.5 h-3.5 animate-spin" />
      <span>{i18n.t('playerBar.contextLoading', {}, 'Fetching context…')}</span>
    </div>
  {:else if view.error}
    <div class="space-y-2">
      <p class="text-brand-text-secondary/60">{i18n.t('playerBar.contextFetchError', {}, "Couldn't fetch context data. Check your connection and retry.")}</p>
      <button type="button" onclick={() => view.refresh()} class="text-brand-accent hover:underline">
        {i18n.t('playerBar.contextRetry', {}, 'Retry')}
      </button>
    </div>
  {:else if sections.length === 0 && !view.offline}
    <p class="text-brand-text-secondary/60 py-2">
      {entity
        ? i18n.t('playerBar.contextEmptyStateEntity', {}, 'No enrichment data available.')
        : i18n.t('playerBar.contextEmptyState', {}, 'No enrichment data available for this track.')}
    </p>
  {/if}
</div>
