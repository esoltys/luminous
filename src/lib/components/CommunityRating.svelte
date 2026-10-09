<script lang="ts">
  import { i18n, formatNumber } from "../stores/i18n.svelte";
  import { openExternalUrl } from "../utils/openExternalUrl";
  import StarRating from "./StarRating.svelte";

  export type CommunityRatingSource = "critiquebrainz" | "musicbrainz";

  interface Props {
    /** CritiqueBrainz or MusicBrainz average, 0.5–5.0. */
    rating: number;
    /** Number of ratings behind the average; omitted when unknown. */
    count?: number | null;
    /** Release group the rating belongs to; without one the label isn't a link. */
    releaseGroupMbid?: string;
    /** Source platform providing the rating. Defaults to CritiqueBrainz. */
    source?: CommunityRatingSource;
  }

  let { rating, count = null, releaseGroupMbid = "", source = "critiquebrainz" }: Props = $props();

  let targetUrl = $derived(
    source === "musicbrainz"
      ? `https://musicbrainz.org/release-group/${releaseGroupMbid}`
      : `https://critiquebrainz.org/release-group/${releaseGroupMbid}`
  );

  let tooltip = $derived(
    source === "musicbrainz"
      ? i18n.t("albumDetail.viewOnMusicBrainzTooltip", {}, "Open this album on MusicBrainz")
      : i18n.t("albumDetail.reviewOnCritiqueBrainzTooltip", {}, "Open this album on CritiqueBrainz to read or write reviews")
  );

  function open(e: Event) {
    // May sit inside a <summary>: don't also toggle the enclosing <details>.
    e.preventDefault();
    e.stopPropagation();
    openExternalUrl(targetUrl);
  }
</script>

{#snippet content()}
  <span>{i18n.t("albumDetail.communityRating", {}, "Community Rating")}</span>
  <StarRating {rating} />
  {#if count != null && count > 0}<span>({formatNumber(count)})</span>{/if}
{/snippet}

{#if releaseGroupMbid}
  <button
    type="button"
    class="inline-flex items-center gap-1.5 hover:text-brand-accent hover:underline cursor-pointer"
    title={tooltip}
    onclick={open}
  >
    {@render content()}
  </button>
{:else}
  <span class="inline-flex items-center gap-1.5">{@render content()}</span>
{/if}
