<script lang="ts">
  import { i18n, formatNumber } from "../stores/i18n.svelte";
  import StarRating from "./StarRating.svelte";

  interface Props {
    /** CritiqueBrainz or MusicBrainz average, 0.5–5.0. */
    rating: number;
    /** Number of ratings behind the average; omitted when unknown. */
    count?: number | null;
  }

  let { rating, count = null }: Props = $props();
</script>

<!-- Display only: it sits in headers where a stray click is easy, and the album menu already links to reviews. -->
<span class="inline-flex items-center gap-1.5">
  <span>{i18n.t("albumDetail.communityRating", {}, "Community Rating")}</span>
  <StarRating {rating} />
  {#if count != null && count > 0}<span>({formatNumber(count)})</span>{/if}
</span>
