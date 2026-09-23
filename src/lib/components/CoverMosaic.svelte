<script lang="ts">
  import CoverArt from "./CoverArt.svelte";
  import { getArtistGradient } from "../utils/artist";
  import { i18n } from "../stores/i18n.svelte";
  import type { CoverStackItem } from "../utils/covers";

  interface Props {
    covers?: CoverStackItem[];
    maxCovers?: number;
    /**
     * A raw image URL (e.g. an artist portrait) to use as the big tile
     * instead of `covers[0]`. When set, every entry in `covers` (up to 4)
     * fills the quarter slots — none of it is "consumed" as the big tile
     * the way `covers[0]` is when this is unset.
     */
    heroImageUrl?: string | null;
    heroImageAlt?: string;
    /**
     * Sets the mosaic's height (e.g. "h-24"); width is derived from it (see
     * `measuredWidth` below) from a ratio computed from the tile count, not
     * set here. The big tile (hero image or covers[0]) is always a full
     * square (edge = the mosaic's height, H); every additional cover (up to
     * 4) is always a quarter tile (H/2 x H/2), never stretched to share the
     * full square with fewer siblings — so 4 total tiles is [full][3x
     * quarter], not four equal tiles.
     *
     * No width class should be set here — `measuredWidth` needs a definite
     * height (from `sizeClass`) to measure and derive a definite pixel width
     * from; grid cells then fill that fixed box with plain fr tracks.
     * Deriving square tiles bottom-up instead (`h-full`/`aspect-square` on
     * each leaf, nested inside width:auto flex containers) hits a circular
     * sizing dependency in Tailwind's webview renderer where the browser
     * falls back to each `<img>`'s natural pixel size, ballooning the
     * mosaic to ~1000px wide. Don't reintroduce that pattern.
     */
    sizeClass?: string;
    fallbackName?: string | null;
    hoverEffect?: boolean;
  }

  let {
    covers = [],
    maxCovers = 5,
    heroImageUrl = null,
    heroImageAlt = "",
    sizeClass = "h-24",
    fallbackName = null,
    hoverEffect = false,
  }: Props = $props();

  let hasHero = $derived(!!heroImageUrl);
  let bigCover = $derived(hasHero ? null : (covers ?? [])[0] ?? null);
  let hasBigTile = $derived(hasHero || !!bigCover);
  // When a hero image supplies the big tile, every cover is a quarter-tile
  // candidate; otherwise covers[0] was already used as the big tile above.
  let quarterCovers = $derived.by(() => {
    const list = covers ?? [];
    return hasHero ? list.slice(0, Math.min(maxCovers, 4)) : list.slice(1, Math.min(maxCovers, 5));
  });
  // 1 or 2 columns of H/2-wide quarter tiles, just enough to hold them
  // (1-2 fit a single column; 3-4 need two, with the last cell left empty
  // for 3).
  let quarterCols = $derived(quarterCovers.length <= 2 ? 1 : 2);
  // Big tile (2 units wide) + the quarter columns (1 unit each) => overall
  // width/height ratio, e.g. 2 quarters => (2+1)/2 = 1.5, 4 quarters => (2+2)/2 = 2.
  let ratio = $derived(quarterCovers.length === 0 ? 1 : (2 + quarterCols) / 2);

  let tileClass = $derived(`w-full h-full ${hoverEffect ? "group-hover:scale-105 transition-transform duration-300" : ""}`);

  /** Width is derived from the rendered height via JS (ResizeObserver), not CSS
   * `aspect-ratio` (see the sizeClass doc comment above for why the bottom-up
   * nested-aspect-ratio approach was already rejected once). WebKitGTK's
   * handling of `aspect-ratio`-driven cross-axis sizing on an auto-width flex
   * item has proven inconsistent across engine versions -- it renders fine on
   * one WebKitGTK build and balloons to ~full container width (overlapping
   * sibling content) on another, e.g. the newer WebKitGTK bundled by the
   * Flatpak's GNOME runtime vs. the host system's — so don't reintroduce a
   * CSS-only width derivation here even if it looks fine on whatever engine
   * you're testing against. */
  let rootEl = $state<HTMLDivElement | undefined>();
  let measuredHeight = $state(0);

  $effect(() => {
    if (!rootEl) return;
    const el = rootEl;
    const update = () => { measuredHeight = el.getBoundingClientRect().height; };
    update();
    const ro = new ResizeObserver(update);
    ro.observe(el);
    return () => ro.disconnect();
  });

  let measuredWidth = $derived(measuredHeight > 0 ? measuredHeight * ratio : 0);
</script>

{#snippet bigTile()}
  {#if hasHero}
    <div class="{tileClass} relative overflow-hidden bg-brand-sidebar border border-brand-border flex items-center justify-center text-brand-text-secondary shrink-0">
      <img src={heroImageUrl} alt={heroImageAlt || i18n.t('common.albumArtAlt')} loading="lazy" class="w-full h-full object-cover object-top" />
    </div>
  {:else if bigCover}
    <CoverArt
      songId={bigCover.songId}
      artEmbedded={bigCover.artEmbedded}
      artAutomatic={bigCover.artAutomatic}
      artManual={bigCover.artManual}
      sizeClass={tileClass}
    />
  {/if}
{/snippet}

<!--
  No `overflow-hidden` here: at a fractional display scale (e.g. Windows
  150%), a tile's percentage-derived box can round to a device pixel taller
  than this exact-height container, and clipping at this level then slices a
  hairline off the *image content itself*. Each tile already clips its own
  `<img>` via CoverArt's own `overflow-hidden`, so nothing needs a second
  clip here — any stray device-pixel just bleeds harmlessly outside the box.
-->
<div
  bind:this={rootEl}
  class="{sizeClass} shrink-0 select-none"
  style={measuredWidth > 0 ? `width: ${measuredWidth}px;` : `aspect-ratio: ${ratio};`}
>
  {#if !hasBigTile}
    {#if fallbackName}
      <div class="w-full h-full bg-gradient-to-br {getArtistGradient(fallbackName)} rounded-full flex items-center justify-center text-white border border-brand-border/40 font-bold text-2xl shadow-md">
        {fallbackName.charAt(0).toUpperCase()}
      </div>
    {:else}
      <CoverArt songId={undefined} artEmbedded={false} artAutomatic={null} artManual={null} sizeClass={tileClass} />
    {/if}
  {:else if quarterCovers.length === 0}
    {@render bigTile()}
  {:else}
    <div class="grid gap-0.5 w-full h-full" style="grid-template-columns: 2fr {quarterCols}fr;">
      {@render bigTile()}
      <div
        class="grid grid-rows-2 gap-0.5 h-full {quarterCols === 2 ? 'grid-cols-2' : 'grid-cols-1'}"
      >
        {#each quarterCovers as cover, i (i)}
          <CoverArt
            songId={cover.songId}
            artEmbedded={cover.artEmbedded}
            artAutomatic={cover.artAutomatic}
            artManual={cover.artManual}
            sizeClass={tileClass}
          />
        {/each}
      </div>
    </div>
  {/if}
</div>
