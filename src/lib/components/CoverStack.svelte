<script lang="ts">
  import CoverArt from "./CoverArt.svelte";
  import { getArtistGradient } from "../utils/artist";
  import { collectionStore } from "../stores/collection.svelte";
  import { extractLocalArtworkPath } from "../types";
  import { i18n } from "../stores/i18n.svelte";
  import { ImagesIcon } from "phosphor-svelte";
  import {
    COVER_STACK_OFFSET_X_PX,
    COVER_STACK_OFFSET_Y_PX,
    COVER_STACK_ROTATION_DEG,
    COVER_STACK_SCALE_STEP,
    COVER_STACK_OPACITY_STEP,
  } from "../constants";

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
    /**
     * When set, and exactly one cover is being shown (i.e. not the
     * multi-song box-set collage), CoverStack checks for additional
     * hierarchical local artwork (#98/#757) via
     * `collectionStore.getExtendedArtworkForSong` — if more than one file
     * was discovered it shows a count badge, and whenever at least one
     * local file was found it reveals an "Open Images" hover control that
     * opens it in the OS's default image viewer (#760). No in-app
     * lightbox/gallery is built here, per owner feedback on #98.
     */
    extendedArtworkSongId?: number;
  }

  let {
    covers = [],
    maxCovers = 6,
    sizeClass = "w-24 h-24",
    direction = "right",
    fallbackName = null,
    hoverEffect = false,
    extendedArtworkSongId = undefined,
  }: Props = $props();

  let extendedArtwork = $state<import("../types").ExtendedArtworkResponse | null>(null);

  $effect(() => {
    const songId = extendedArtworkSongId;
    if (songId === undefined) {
      extendedArtwork = null;
      return;
    }
    let cancelled = false;
    collectionStore.getExtendedArtworkForSong(songId).then((result) => {
      if (!cancelled) extendedArtwork = result;
    });
    return () => {
      cancelled = true;
    };
  });

  let openImagesPath = $derived(extractLocalArtworkPath(extendedArtwork?.primary_uri));
  let showExtendedArtworkUi = $derived(
    extendedArtworkSongId !== undefined && covers.length <= 1 && !!openImagesPath
  );
  let extraArtworkCount = $derived(extendedArtwork && extendedArtwork.count > 1 ? extendedArtwork.count : 0);

  async function handleOpenImages(e: MouseEvent) {
    e.stopPropagation();
    if (openImagesPath) {
      await collectionStore.openArtworkPath(openImagesPath);
    }
  }

  let activeCovers = $derived.by(() => {
    if (!covers || covers.length === 0) return [];
    return covers.slice(0, maxCovers);
  });

  function getTransform(i: number, count: number): string {
    if (count <= 1) return "none";
    if (direction === "right") {
      // Front cover (i = 0) stays centered at X=0 in the card container
      const x = i * 7;
      const y = i * -5;
      const rot = i * 5;
      const scale = 1 - i * 0.05;
      return `translate(${x}px, ${y}px) rotate(${rot}deg) scale(${scale})`;
    } else {
      // Left stack (used in ArtistDetailView hero header)
      const x = i * COVER_STACK_OFFSET_X_PX;
      const y = i * COVER_STACK_OFFSET_Y_PX;
      const rot = i * COVER_STACK_ROTATION_DEG;
      const scale = 1 - i * COVER_STACK_SCALE_STEP;
      return `translate(${x}px, ${y}px) rotate(${rot}deg) scale(${scale})`;
    }
  }

  function getOpacity(i: number, count: number): number {
    if (count <= 1) return 1;
    return 1 - i * (direction === "left" ? COVER_STACK_OPACITY_STEP : 0.09);
  }
</script>

<div class="flex items-center {direction === 'left' ? 'justify-end' : 'justify-center'} w-full h-full my-auto select-none">
  {#if activeCovers.length > 0}
    {#if activeCovers.length === 1}
      <div class="{sizeClass} overflow-hidden relative group/artwork">
        <CoverArt
          songId={activeCovers[0].songId}
          artEmbedded={activeCovers[0].artEmbedded}
          artAutomatic={activeCovers[0].artAutomatic}
          artManual={activeCovers[0].artManual}
          sizeClass="w-full h-full"
        />
        {#if showExtendedArtworkUi}
          {#if extraArtworkCount > 0}
            <div class="absolute top-1 right-1 z-10 px-1.5 py-0.5 rounded-full bg-black/70 text-white text-[10px] leading-none font-semibold flex items-center gap-0.5 pointer-events-none">
              <ImagesIcon class="w-3 h-3" weight="fill" />
              {extraArtworkCount}
            </div>
          {/if}
          <!-- svelte-ignore a11y_click_events_have_key_events -->
          <!-- svelte-ignore a11y_no_static_element_interactions -->
          <div
            class="absolute inset-0 z-10 flex items-center justify-center bg-black/50 opacity-0 group-hover/artwork:opacity-100 transition-opacity duration-200 cursor-pointer"
            onclick={handleOpenImages}
            title={extraArtworkCount > 0
              ? i18n.t("common.openImagesCount", { count: extraArtworkCount })
              : i18n.t("common.openImages")}
          >
            <span class="flex items-center gap-1 px-2 py-1 rounded-md bg-black/60 text-white text-xs font-medium">
              <ImagesIcon class="w-3.5 h-3.5" />
              {i18n.t("common.openImages")}
            </span>
          </div>
        {/if}
      </div>
    {:else}
      <div class="relative {sizeClass} flex items-center justify-center shrink-0">
        {#each activeCovers as cover, i (i)}
          <div
            class="cover-item absolute {direction === 'left' ? 'bottom-0 right-0 w-28 h-28' : 'inset-0'} overflow-hidden border border-brand-border/60 shadow-xl transition-all duration-300 {hoverEffect ? 'group-hover:scale-105' : ''}"
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
    {/if}
  {:else if fallbackName}
    <div class="w-24 h-24 bg-gradient-to-br {getArtistGradient(fallbackName)} rounded-full flex items-center justify-center text-white border border-brand-border/40 font-bold text-2xl shadow-md shrink-0 {direction === 'left' ? 'ml-auto' : 'mx-auto'}">
      {fallbackName ? fallbackName.charAt(0).toUpperCase() : "?"}
    </div>
  {:else}
    <div class="{sizeClass} bg-brand-main flex items-center justify-center text-brand-accent-text border border-brand-border overflow-hidden relative {direction === 'left' ? 'ml-auto' : 'mx-auto'}">
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
