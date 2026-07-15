<script lang="ts">
  import type { Song } from "../types";
  import CarouselCard from "./CarouselCard.svelte";
  import { ChevronLeft, ChevronRight } from "lucide-svelte";

  let { songs }: { songs: Song[] } = $props();

  let scrollContainer = $state<HTMLDivElement | undefined>(undefined);
  let canScrollLeft = $state(false);
  let canScrollRight = $state(false);

  function updateScrollButtons() {
    if (!scrollContainer) return;
    canScrollLeft = scrollContainer.scrollLeft > 0;
    canScrollRight =
      scrollContainer.scrollLeft <
      scrollContainer.scrollWidth - scrollContainer.clientWidth - 10;
  }

  function scroll(direction: "left" | "right") {
    if (!scrollContainer) return;
    const cardWidth = 200; // Approximate width of a card including gap
    const scrollAmount = cardWidth * 3;
    scrollContainer.scrollBy({
      left: direction === "left" ? -scrollAmount : scrollAmount,
      behavior: "smooth",
    });
  }

  $effect(() => {
    if (scrollContainer) {
      scrollContainer.addEventListener("scroll", updateScrollButtons);
      updateScrollButtons();
      return () => {
        scrollContainer?.removeEventListener("scroll", updateScrollButtons);
      };
    }
  });
</script>

<div class="relative group/carousel">
  <!-- Left arrow -->
  {#if canScrollLeft}
    <button
      onclick={() => scroll("left")}
      class="absolute left-0 top-1/2 -translate-y-1/2 z-10 opacity-0 group-carousel/hover:opacity-100 transition-opacity duration-200 bg-black/50 hover:bg-black/70 rounded-full p-2"
      title="Scroll left"
    >
      <ChevronLeft class="w-6 h-6 text-white" />
    </button>
  {/if}

  <!-- Carousel container -->
  <div
    bind:this={scrollContainer}
    class="flex gap-4 overflow-x-auto scroll-smooth pb-2 carousel-scroll"
  >
    {#each songs as song (song.id)}
      <CarouselCard {song} />
    {/each}
  </div>

  <!-- Right arrow -->
  {#if canScrollRight}
    <button
      onclick={() => scroll("right")}
      class="absolute right-0 top-1/2 -translate-y-1/2 z-10 opacity-0 group-carousel/hover:opacity-100 transition-opacity duration-200 bg-black/50 hover:bg-black/70 rounded-full p-2"
      title="Scroll right"
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
