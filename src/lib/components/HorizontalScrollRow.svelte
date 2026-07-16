<script lang="ts">
  import { ChevronLeft, ChevronRight } from "lucide-svelte";
  import type { Snippet } from "svelte";
  import { i18n } from "../stores/i18n.svelte";

  let { children }: { children: Snippet } = $props();

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

<div class="relative">
  {#if canScrollLeft}
    <button
      onclick={() => scroll("left")}
      class="absolute left-0 top-1/2 -translate-y-1/2 z-10 transition-colors bg-black/50 hover:bg-black/70 rounded-full p-2 cursor-pointer"
      title={i18n.t('common.scrollLeft')}
    >
      <ChevronLeft class="w-6 h-6 text-white" />
    </button>
  {/if}

  <div bind:this={scrollContainer} class="flex gap-4 overflow-x-auto scroll-smooth pb-2 carousel-scroll">
    {@render children()}
  </div>

  {#if canScrollRight}
    <button
      onclick={() => scroll("right")}
      class="absolute right-0 top-1/2 -translate-y-1/2 z-10 transition-colors bg-black/50 hover:bg-black/70 rounded-full p-2 cursor-pointer"
      title={i18n.t('common.scrollRight')}
    >
      <ChevronRight class="w-6 h-6 text-white" />
    </button>
  {/if}
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
