<script lang="ts">
  import { Star, CircleSlash } from "lucide-svelte";
  import { i18n } from "../stores/i18n.svelte";

  interface Props {
    /** Current rating: -1 (unrated) or 0.5–5.0 in half-star steps. */
    rating: number;
    /** When provided, the stars are interactive; omitted = read-only display. */
    onRate?: (rating: number) => void;
    size?: "sm" | "md";
  }

  let { rating, onRate, size = "sm" }: Props = $props();

  let hoverValue = $state<number | null>(null);

  const starClass = $derived(size === "md" ? "w-4 h-4" : "w-3.5 h-3.5");
  const clearClass = $derived(size === "md" ? "w-3.5 h-3.5" : "w-3 h-3");
  const shown = $derived(hoverValue ?? (rating > 0 ? rating : 0));

  // Left half of a star = half-star value, right half = full-star value.
  function valueAt(star: number, e: MouseEvent): number {
    const rect = (e.currentTarget as HTMLElement).getBoundingClientRect();
    const inLeftHalf = rect.width > 0 && e.clientX - rect.left < rect.width / 2;
    return star - (inLeftHalf ? 0.5 : 0);
  }

  function handleClick(star: number, e: MouseEvent) {
    e.stopPropagation();
    if (!onRate) return;
    const value = valueAt(star, e);
    // Clicking the current rating again clears it.
    onRate(value === rating ? -1 : value);
  }

  function handleClear(e: MouseEvent) {
    e.stopPropagation();
    onRate?.(-1);
  }

  function fillFraction(star: number): number {
    return Math.max(0, Math.min(1, shown - (star - 1)));
  }
</script>

<div
  class="flex items-center gap-0.5 {rating <= 0 && !hoverValue ? 'opacity-40' : ''} transition-opacity"
  role="group"
  aria-label={i18n.t('rating.label')}
  onmouseleave={() => (hoverValue = null)}
>
  {#if onRate}
    <button
      type="button"
      disabled={rating <= 0}
      onclick={handleClear}
      class="block text-brand-text-secondary/50 hover:text-brand-text-secondary transition-colors disabled:cursor-default disabled:pointer-events-none"
      title={i18n.t('rating.clearTooltip')}
      aria-label={i18n.t('rating.clearTooltip')}
    >
      <CircleSlash class={clearClass} />
    </button>
  {/if}
  {#each [1, 2, 3, 4, 5] as star}
    <button
      type="button"
      disabled={!onRate}
      onclick={(e) => handleClick(star, e)}
      onmousemove={(e) => {
        if (onRate) hoverValue = valueAt(star, e);
      }}
      class="relative block {onRate ? '' : 'cursor-default'} disabled:pointer-events-none"
      title={onRate ? i18n.t('rating.setTooltip', { value: hoverValue ?? star }) : undefined}
      aria-label={i18n.t('rating.setTooltip', { value: star })}
    >
      <Star class="{starClass} text-brand-text-secondary/50" />
      {#if fillFraction(star) > 0}
        <span
          class="absolute inset-0 overflow-hidden pointer-events-none"
          style="width: {fillFraction(star) * 100}%"
        >
          <Star class="{starClass} fill-current text-brand-accent-text" />
        </span>
      {/if}
    </button>
  {/each}
</div>
