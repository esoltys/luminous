<script lang="ts">
  import { i18n, formatNumber } from "../stores/i18n.svelte";
  import { openExternalUrl } from "../utils/openExternalUrl";
  import StarRating from "./StarRating.svelte";

  interface Props {
    /** CritiqueBrainz average, 0.5–5.0. */
    rating: number;
    /** Number of ratings behind the average; omitted when unknown. */
    count?: number | null;
    /** Release group the rating belongs to; without one the label isn't a link. */
    releaseGroupMbid?: string;
  }

  let { rating, count = null, releaseGroupMbid = "" }: Props = $props();

  function open(e: Event) {
    // May sit inside a <summary>: don't also toggle the enclosing <details>.
    e.preventDefault();
    e.stopPropagation();
    openExternalUrl(`https://critiquebrainz.org/release-group/${releaseGroupMbid}`);
  }
</script>

{#snippet content()}
  <span>{i18n.t("albumDetail.communityRating", {}, "Community Rating")}</span>
  <StarRating {rating} />
  {#if count != null}<span>({formatNumber(count)})</span>{/if}
{/snippet}

{#if releaseGroupMbid}
  <button
    type="button"
    class="inline-flex items-center gap-1.5 hover:text-brand-accent hover:underline cursor-pointer"
    title={i18n.t("albumDetail.reviewOnCritiqueBrainzTooltip", {}, "Open this album on CritiqueBrainz to read or write reviews")}
    onclick={open}
  >
    {@render content()}
  </button>
{:else}
  <span class="inline-flex items-center gap-1.5">{@render content()}</span>
{/if}
