<script lang="ts">
  import { prefs } from "../stores/prefs.svelte";
  import HeartToggle from "./HeartToggle.svelte";
  import StarRating from "./StarRating.svelte";

  interface Props {
    /** Current rating: -1 (unrated) or 0.5–5.0 in half-star steps. */
    rating: number;
    onRate: (rating: number) => void;
    size?: "sm" | "md";
  }

  let { rating, onRate, size = "sm" }: Props = $props();
</script>

{#if prefs.ratingStyle === "heart"}
  <HeartToggle
    favorite={rating === 5}
    onToggle={() => onRate(rating === 5 ? -1 : 5)}
    sizeClass={size === "md" ? "w-5 h-5" : "w-4 h-4"}
  />
{:else}
  <StarRating {rating} {onRate} {size} />
{/if}
