<script lang="ts">
  import { ChevronLeft, ChevronRight } from "lucide-svelte";
  import type { Snippet } from "svelte";
  import { i18n } from "../stores/i18n.svelte";

  interface Props {
    title?: string;
    headerExtra?: Snippet;
    children: Snippet;
  }

  let { title, headerExtra, children }: Props = $props();

  let scrollContainer = $state<HTMLDivElement | undefined>(undefined);
  let canScrollLeft = $state(false);
  let canScrollRight = $state(false);

  function updateScrollButtons() {
    if (!scrollContainer) return;
    canScrollLeft = scrollContainer.scrollLeft > 0;
    canScrollRight =
      scrollContainer.scrollLeft < scrollContainer.scrollWidth - scrollContainer.clientWidth - 10;
  }

  function scroll(direction: "left" | "right") {
    if (!scrollContainer) return;
    const scrollAmount = 200 * 3; // approximate card width including gap
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
        {#if title}
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
          class="p-1.5 rounded-full text-brand-text-primary hover:bg-brand-sidebar transition-colors cursor-pointer disabled:opacity-30 disabled:cursor-not-allowed"
          title={i18n.t('common.scrollLeft')}
          aria-label={i18n.t('common.scrollLeft')}
        >
          <ChevronLeft class="w-5 h-5" />
        </button>
        <button
          onclick={() => scroll("right")}
          disabled={!canScrollRight}
          class="p-1.5 rounded-full text-brand-text-primary hover:bg-brand-sidebar transition-colors cursor-pointer disabled:opacity-30 disabled:cursor-not-allowed"
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
