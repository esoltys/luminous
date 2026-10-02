<script lang="ts">
  import { prefs } from "../stores/prefs.svelte";
  import HeartToggle from "./HeartToggle.svelte";
  import StarRating from "./StarRating.svelte";

  interface Props {
    /** Current rating: -1 (unrated) or 0.5–5.0 in half-star steps. */
    rating: number;
    onRate?: (rating: number) => void;
    /** Current loved state for tracks: 1 (loved), 0 (neutral), -1 (hated). */
    loved?: number;
    onSetLoved?: (loved: number) => void;
    size?: "sm" | "md";
    /** If true, strictly render stars (albums don't have a love flag). */
    isAlbum?: boolean;
  }

  let {
    rating,
    onRate = () => {},
    loved,
    onSetLoved,
    size = "sm",
    isAlbum = false,
  }: Props = $props();

  let showStars = $derived(isAlbum || prefs.ratingStyle === "stars" || prefs.ratingStyle === "both");
  let showHeart = $derived(!isAlbum && (prefs.ratingStyle === "heart" || prefs.ratingStyle === "both"));

  function handleHeartToggle() {
    if (onSetLoved) {
      const current = loved ?? (rating === 5 ? 1 : 0);
      onSetLoved(current === 1 ? 0 : 1);
    } else {
      onRate(rating === 5 ? -1 : 5);
    }
  }

  function handleHeartSetLoved(nextLoved: number) {
    if (onSetLoved) {
      onSetLoved(nextLoved);
    } else {
      onRate(nextLoved === 1 ? 5 : -1);
    }
  }
</script>

{#if showHeart && showStars}
  <div class="inline-flex items-center gap-1.5 shrink-0">
    <HeartToggle
      {loved}
      favorite={loved === 1 || (loved === undefined && rating === 5)}
      onToggle={handleHeartToggle}
      onSetLoved={handleHeartSetLoved}
      sizeClass={size === "md" ? "w-5 h-5" : "w-4 h-4"}
    />
    <StarRating {rating} {onRate} {size} />
  </div>
{:else if showHeart}
  <HeartToggle
    {loved}
    favorite={loved === 1 || (loved === undefined && rating === 5)}
    onToggle={handleHeartToggle}
    onSetLoved={handleHeartSetLoved}
    sizeClass={size === "md" ? "w-5 h-5" : "w-4 h-4"}
  />
{:else}
  <StarRating {rating} {onRate} {size} />
{/if}
