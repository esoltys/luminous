<script lang="ts">
  import CoverArt from "./CoverArt.svelte";
  import { getArtistGradient } from "../utils/artist";

  export interface CoverItem {
    songId?: number;
    artEmbedded?: boolean;
    artAutomatic?: string | null;
    artManual?: string | null;
  }

  interface Props {
    covers?: CoverItem[];
    maxCovers?: number;
    sizeClass?: string;
    direction?: "right" | "left";
    fallbackName?: string | null;
    hoverEffect?: boolean;
  }

  let {
    covers = [],
    maxCovers = 6,
    sizeClass = "w-24 h-24",
    direction = "right",
    fallbackName = null,
    hoverEffect = false,
  }: Props = $props();

  let activeCovers = $derived.by(() => {
    if (!covers || covers.length === 0) return [];
    return covers.slice(0, maxCovers);
  });

  function getTransform(i: number, count: number): string {
    if (count <= 1) return "none";
    if (direction === "right") {
      // Offset center adjustment so the whole stack stays horizontally centered
      const centerOffsetX = -((count - 1) * 6);
      const x = centerOffsetX + i * 12;
      const y = i * -8;
      const rot = i * 6;
      const scale = 1 - i * 0.06;
      return `translate(${x}px, ${y}px) rotate(${rot}deg) scale(${scale})`;
    } else {
      // Left stack (used in headers)
      const x = i * -18;
      const y = i * -10;
      const rot = i * -5;
      const scale = 1 - i * 0.05;
      return `translate(${x}px, ${y}px) rotate(${rot}deg) scale(${scale})`;
    }
  }

  function getOpacity(i: number, count: number): number {
    if (count <= 1) return 1;
    return 1 - i * (direction === "left" ? 0.07 : 0.09);
  }
</script>

<div class="flex items-center justify-center w-full my-auto select-none">
  {#if activeCovers.length > 0}
    <div class="relative {sizeClass} flex items-center justify-center shrink-0">
      {#each activeCovers as cover, i (i)}
        <div
          class="cover-item absolute inset-0 rounded-lg overflow-hidden border border-brand-border/50 shadow-lg transition-transform duration-200 {hoverEffect ? 'group-hover:scale-105' : ''}"
          style="z-index: {10 - i}; transform: {getTransform(i, activeCovers.length)}; opacity: {getOpacity(i, activeCovers.length)};"
        >
          <CoverArt
            songId={cover.songId}
            artEmbedded={cover.artEmbedded}
            artAutomatic={cover.artAutomatic}
            artManual={cover.artManual}
            sizeClass="w-full h-full"
          />
        </div>
      {/each}
    </div>
  {:else if fallbackName}
    <div class="w-24 h-24 bg-gradient-to-br {getArtistGradient(fallbackName)} rounded-full flex items-center justify-center text-white border border-brand-border/40 font-bold text-2xl shadow-md shrink-0 mx-auto">
      {fallbackName ? fallbackName.charAt(0).toUpperCase() : "?"}
    </div>
  {:else}
    <div class="{sizeClass} bg-brand-main rounded-lg flex items-center justify-center text-brand-accent-text border border-brand-border overflow-hidden relative mx-auto">
      <CoverArt
        songId={undefined}
        artEmbedded={false}
        artAutomatic={null}
        artManual={null}
        sizeClass="w-full h-full"
      />
    </div>
  {/if}
</div>

<style>
  .cover-item:nth-child(n + 4) {
    display: none;
  }
  @container (min-width: 150px) {
    .cover-item:nth-child(4) {
      display: block;
    }
  }
  @container (min-width: 180px) {
    .cover-item:nth-child(5) {
      display: block;
    }
  }
  @container (min-width: 210px) {
    .cover-item:nth-child(6) {
      display: block;
    }
  }
</style>
