<script lang="ts">
  import { CaretRightIcon as ChevronRight } from "phosphor-svelte";
  import type { Snippet } from "svelte";
  import type { CollectionViewMode } from "../stores/prefs.svelte";
  import ViewModeToggle from "./ViewModeToggle.svelte";

  interface Props {
    title?: string;
    headerExtra?: Snippet;
    /** When provided, the title becomes a clickable button that navigates to
     * the category's full expanded view (see #169). */
    onHeaderClick?: () => void;
    /** Cards or rows layout; the grid tracks follow it. */
    viewMode?: CollectionViewMode;
    /** When provided, a Cards/Rows toggle appears in the header. Omit it on
     * sections that share a mode with a sibling that owns the toggle. */
    onViewModeChange?: (mode: CollectionViewMode) => void;
    /** Items for the current layout; each must fill its grid cell (`w-full`). */
    children: Snippet<[CollectionViewMode]>;
  }

  let { title, headerExtra, onHeaderClick, viewMode = "cards", onViewModeChange, children }: Props = $props();
</script>

<!-- A titled grid of cards or rows. The column count follows the container's
     width (sidebar and right panel included), matching the Albums and Artists
     grids in CollectionView, so callers never size items or handle overflow. -->
<div class="space-y-4">
  {#if title || headerExtra || onViewModeChange}
    <div class="flex items-center gap-4 min-h-[32px]">
      {#if title && onHeaderClick}
        <button
          type="button"
          onclick={onHeaderClick}
          class="group flex items-center gap-1 text-xl font-semibold text-brand-text-primary hover:text-brand-accent-text transition-colors"
        >
          {title}
          <ChevronRight class="w-5 h-5 opacity-0 group-hover:opacity-100 transition-opacity" />
        </button>
      {:else if title}
        <h2 class="text-xl font-semibold text-brand-text-primary">{title}</h2>
      {/if}
      {#if headerExtra}
        {@render headerExtra()}
      {/if}
      {#if onViewModeChange}
        <div class="ml-auto">
          <ViewModeToggle mode={viewMode} onChange={onViewModeChange} />
        </div>
      {/if}
    </div>
  {/if}

  <div
    class="grid {viewMode === 'rows' ? 'grid-cols-[repeat(auto-fill,minmax(280px,1fr))] gap-2' : 'grid-cols-[repeat(auto-fill,minmax(180px,1fr))] gap-6'}"
    data-testid="card-section-grid"
  >
    {@render children(viewMode)}
  </div>
</div>
