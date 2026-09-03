<script lang="ts">
  import { CaretLeftIcon as ChevronLeft, CaretRightIcon as ChevronRight } from "phosphor-svelte";
  import type { Snippet } from "svelte";
  import { i18n } from "../stores/i18n.svelte";

  interface Props {
    title?: string;
    headerExtra?: Snippet;
    /** When provided, the title becomes a clickable button that navigates to
     * the category's full expanded view (see #169). */
    onHeaderClick?: () => void;
    children: Snippet;
  }

  let { title, headerExtra, onHeaderClick, children }: Props = $props();

  const SCROLL_END_BUFFER_PX = 10;
  const SCROLL_STEP_PX = 600; // approximate 3 card-widths including gap

  let scrollContainer = $state<HTMLDivElement | undefined>(undefined);
  let canScrollLeft = $state(false);
  let canScrollRight = $state(false);

  function updateScrollButtons() {
    if (!scrollContainer) return;
    canScrollLeft = scrollContainer.scrollLeft > 0;
    canScrollRight =
      scrollContainer.scrollLeft < scrollContainer.scrollWidth - scrollContainer.clientWidth - SCROLL_END_BUFFER_PX;
  }

  function scroll(direction: "left" | "right") {
    if (!scrollContainer) return;
    const scrollAmount = SCROLL_STEP_PX;
    scrollContainer.scrollBy({
      left: direction === "left" ? -scrollAmount : scrollAmount,
      behavior: "smooth",
    });
  }

  $effect(() => {
    if (scrollContainer) {
      const el = scrollContainer;
      el.addEventListener("scroll", updateScrollButtons);
      // ResizeObserver catches layout changes a one-shot check on mount can
      // miss: cover art images loading in, window resize, sidebar toggling.
      const observer = new ResizeObserver(updateScrollButtons);
      observer.observe(el);
      updateScrollButtons();
      return () => {
        el.removeEventListener("scroll", updateScrollButtons);
        observer.disconnect();
      };
    }
  });
</script>

<div class="space-y-4">
  {#if title || headerExtra || canScrollLeft || canScrollRight}
    <div class="flex items-center justify-between min-h-[32px]">
      <div class="flex items-center gap-4">
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
      </div>

      <div class="flex items-center gap-1.5 ml-auto">
        <button
          onclick={() => scroll("left")}
          disabled={!canScrollLeft}
          class="p-1.5 rounded-full text-brand-text-primary hover:bg-brand-sidebar transition-colors disabled:opacity-30 disabled:cursor-not-allowed"
          title={i18n.t('common.scrollLeft')}
          aria-label={i18n.t('common.scrollLeft')}
        >
          <ChevronLeft class="w-5 h-5" />
        </button>
        <button
          onclick={() => scroll("right")}
          disabled={!canScrollRight}
          class="p-1.5 rounded-full text-brand-text-primary hover:bg-brand-sidebar transition-colors disabled:opacity-30 disabled:cursor-not-allowed"
          title={i18n.t('common.scrollRight')}
          aria-label={i18n.t('common.scrollRight')}
        >
          <ChevronRight class="w-5 h-5" />
        </button>
      </div>
    </div>
  {/if}

  <div bind:this={scrollContainer} class="flex gap-4 overflow-x-auto scroll-smooth snap-x snap-mandatory pb-2 carousel-scroll">
    {@render children()}
  </div>
</div>

<style>
  :global(.carousel-scroll) {
    scrollbar-width: none;
    -ms-overflow-style: none;
  }
  :global(.carousel-scroll::-webkit-scrollbar) {
    display: none;
  }
</style>
