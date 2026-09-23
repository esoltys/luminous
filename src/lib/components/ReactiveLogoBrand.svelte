<script lang="ts">
  import { themeStore } from "../stores/theme.svelte";
  import { playerStore } from "../stores/player.svelte";
  import { listen } from "@tauri-apps/api/event";
  import { onMount, onDestroy } from "svelte";
  import { i18n } from "../stores/i18n.svelte";
  import { acquireSpectrum, releaseSpectrum } from "../utils/spectrumEnable";

  interface Props {
    size?: "sm" | "md" | "lg" | "xl";
    className?: string;
  }

  let { size = "md", className = "" }: Props = $props();

  const sizeMap = {
    sm: "w-8 h-8",
    md: "w-12 h-12",
    lg: "w-16 h-16",
    xl: "w-20 h-20"
  };

  let isPulsingEnabled = $state(true);
  let bassIntensity = $state(0);
  let midIntensity = $state(0);
  let coronalIntensity = $state(0);
  let unlisten: (() => void) | null = null;
  // Tracks whether this component currently holds a spectrum acquisition, so
  // onDestroy/togglePulsing release exactly the acquisitions this instance
  // made — never more (double-release) or fewer (a leaked hold that keeps
  // the backend computing spectrum data nobody's listening for anymore).
  let hasAcquiredSpectrum = false;

  // Below this delta, a new intensity reading is visually indistinguishable
  // from the current one, so skipping it saves a style update and compositor
  // frame at up to ~30fps (spectrum-data cadence) during quiet/steady
  // passages without touching how peaks render.
  const INTENSITY_EPSILON = 0.015;
  function settle(current: number, next: number): number {
    return Math.abs(next - current) < INTENSITY_EPSILON ? current : next;
  }

  onMount(async () => {
    const stored = localStorage.getItem("logo_pulsing");
    if (stored !== null) {
      isPulsingEnabled = stored === "true";
    }

    if (isPulsingEnabled) {
      acquireSpectrum();
      hasAcquiredSpectrum = true;
    }

    try {
      unlisten = await listen<number[]>("spectrum-data", (event) => {
        if (typeof document !== "undefined" && document.hidden) return;
        if (!isPulsingEnabled) {
          bassIntensity = 0;
          midIntensity = 0;
          coronalIntensity = 0;
          return;
        }
        const data = event.payload;
        if (data && data.length > 0) {
          // Segment and average the 32 spectrum bins into Bass, Mids, and Coronal/Treble
          // to prevent single-bin noise/leakage from driving unrelated components
          const bassAvg = data.slice(0, 8).reduce((sum, v) => sum + v, 0) / 8;
          const midAvg = data.slice(8, 20).reduce((sum, v) => sum + v, 0) / 12;
          const coronalAvg = data.slice(20, 32).reduce((sum, v) => sum + v, 0) / 12;

          // Backend bins already arrive normalized to the frame's own peak
          // (see analyzer::calculate_spectrum), so only a modest boost is
          // needed for visual punch — a large fixed multiplier here would
          // just re-saturate every band back to the ceiling.
          bassIntensity = settle(bassIntensity, Math.min(1.0, bassAvg * 1.4));
          midIntensity = settle(midIntensity, Math.min(1.0, midAvg * 1.4));
          coronalIntensity = settle(coronalIntensity, Math.min(1.0, coronalAvg * 1.6));
        } else {
          bassIntensity = 0;
          midIntensity = 0;
          coronalIntensity = 0;
        }
      });
    } catch (e) {
      console.error("Failed to listen to spectrum-data in logo:", e);
    }
  });

  onDestroy(() => {
    if (unlisten) unlisten();
    if (hasAcquiredSpectrum) {
      releaseSpectrum();
      hasAcquiredSpectrum = false;
    }
  });

  function togglePulsing() {
    isPulsingEnabled = !isPulsingEnabled;
    localStorage.setItem("logo_pulsing", String(isPulsingEnabled));
    if (!isPulsingEnabled) {
      bassIntensity = 0;
      midIntensity = 0;
      coronalIntensity = 0;
      if (hasAcquiredSpectrum) {
        releaseSpectrum();
        hasAcquiredSpectrum = false;
      }
    } else if (!hasAcquiredSpectrum) {
      acquireSpectrum();
      hasAcquiredSpectrum = true;
    }
  }

  // Authentic Luminous Brand Colors for resting state (matches docs/luminous-mark.svg and static/app-icon.svg)
  const BRAND_INDIGO = "#626FE8";
  const BRAND_GOLD = "#FFB648";
  const BRAND_SILHOUETTE = "#0A0A0D";
  const BRAND_BURST = "#FFFFFF";

  let isPlaying = $derived(playerStore.state === "playing");
  let isResting = $derived(!isPlaying || !isPulsingEnabled);

  // In resting state (when playback is stopped/paused, or pulsing is disabled),
  // the logo renders the authentic, crisp Luminous brand mark (docs/luminous-mark.svg)
  // with no blur filters, authentic brand indigo (#626FE8) and gold (#FFB648),
  // zero ambient glow, and sharp vector geometry.
  //
  // When music is playing and pulsing is enabled, it transitions into the audio-reactive
  // visualizer driven live by the current track's frequency bands:
  // - Mid frequencies drive the ambient glow halo and inner rim (re-targeting to active theme accent)
  // - Treble frequencies drive the outer eclipse ring (re-targeting to theme accent-hover)
  // - Bass frequencies drive the coronal burst flare halo

  // Rendering: each blurred element is its own layer whose SVG never changes
  // while playing — its blur is rasterized once — and the audio drives only
  // that layer's CSS transform/opacity, which the GPU compositor applies
  // without repainting. Driving SVG geometry (r, stroke-width) directly made
  // WebKit re-run every feGaussianBlur on the CPU on each spectrum update
  // (up to ~60 times a second with the old 50ms transitions): ~45% of the
  // app's CPU while playing. Radius changes become a scale around the element's
  // center (drawn at a mid-range size so the blur stays close to the
  // original at both extremes); stroke-width changes become a cross-fade
  // between a thin and a thick copy.
  const uid = $props.id();

  // Ambient glow (mid-driven): radius 95–130, drawn at GLOW_BASE_R
  const GLOW_BASE_R = 112;
  let glowScale = $derived((95 + midIntensity * 35) / GLOW_BASE_R);
  let glowOpacity = $derived(0.2 + midIntensity * 0.7);

  // Inner rim (mid-driven): stroke 12–24, as a thin/thick cross-fade
  let innerRimOpacity = $derived(0.7 + midIntensity * 0.3);

  // Eclipse ring (treble-driven): stroke 5–21, as a thin/thick cross-fade
  let ringOpacity = $derived(0.5 + coronalIntensity * 0.5);

  // Coronal burst halo (bass-driven): radius 18–44, drawn at BURST_BASE_R
  const BURST_BASE_R = 31;
  let burstScale = $derived((18 + bassIntensity * 26) / BURST_BASE_R);
  let burstOpacity = $derived(0.15 + bassIntensity * 0.7);

  let maxIntensity = $derived(Math.max(bassIntensity, midIntensity, coronalIntensity));
  let saturationVal = $derived(0.15 + maxIntensity * 2.35);

  // The SVG viewBox is -40..240, so the mark's center (100,100) sits at 50%
  // and the burst's (150.6,59.6) at these percentages of the layer box.
  const BURST_ORIGIN = `${((150.6 + 40) / 280) * 100}% ${((59.6 + 40) / 280) * 100}%`;
</script>

<button
  type="button"
  onclick={togglePulsing}
  class="bg-transparent border-none p-0 focus:outline-none focus-visible:ring-2 focus-visible:ring-brand-accent rounded-full overflow-hidden isolate"
  title={isPulsingEnabled ? i18n.t('common.disableLogoPulse') : i18n.t('common.enableLogoPulse')}
  aria-label={i18n.t('common.toggleLogoPulsing')}
>
  {#if isResting}
    <!-- Resting: the crisp brand mark (docs/luminous-mark.svg) — no blur,
         brand indigo/gold, no glow. -->
    <svg
      xmlns="http://www.w3.org/2000/svg"
      viewBox="-40 -40 280 280"
      class="{sizeMap[size]} {className} select-none"
      aria-hidden="true"
    >
      <circle cx="100" cy="100" r="77" stroke={BRAND_INDIGO} stroke-width="14" fill="none" opacity="1" />
      <circle cx="100" cy="100" r="92" stroke={BRAND_GOLD} stroke-width="8" fill="none" opacity="1" />
      <circle cx="100" cy="100" r="68" fill={BRAND_SILHOUETTE} />
      <circle cx="150.6" cy="59.6" r="17.4" fill={BRAND_BURST} />
    </svg>
  {:else}
    <!-- Playing: audio-reactive layers (see the rendering note above).
         Filters mirror docs/luminous-mark-reactive.svg's three blur passes. -->
    <div
      class="reactive-logo relative {sizeMap[size]} {className} select-none"
      style="filter: saturate({saturationVal});"
      aria-hidden="true"
    >
      <svg class="layer" viewBox="-40 -40 280 280" style="transform: scale({glowScale}); opacity: {glowOpacity};">
        <defs>
          <filter id="glowBlurOuter-{uid}" x="-30%" y="-30%" width="160%" height="160%">
            <feGaussianBlur stdDeviation="22" />
          </filter>
        </defs>
        <circle data-layer="glow" cx="100" cy="100" r={GLOW_BASE_R} fill="var(--color-accent)" filter="url(#glowBlurOuter-{uid})" />
      </svg>

      {#each [{ width: 12, weight: 1 - midIntensity }, { width: 24, weight: midIntensity }] as rim (rim.width)}
        <svg class="layer" viewBox="-40 -40 280 280" style="opacity: {innerRimOpacity * rim.weight};">
          <defs>
            <filter id="rimBlur-{uid}-{rim.width}" x="-15%" y="-15%" width="130%" height="130%">
              <feGaussianBlur stdDeviation="3" />
            </filter>
          </defs>
          <circle data-layer="rim" cx="100" cy="100" r="77" stroke="var(--color-accent)" stroke-width={rim.width} fill="none" filter="url(#rimBlur-{uid}-{rim.width})" />
        </svg>
      {/each}

      {#each [{ width: 5, weight: 1 - coronalIntensity }, { width: 21, weight: coronalIntensity }] as ring (ring.width)}
        <svg class="layer" viewBox="-40 -40 280 280" style="opacity: {ringOpacity * ring.weight};">
          <defs>
            <filter id="ringBlur-{uid}-{ring.width}" x="-15%" y="-15%" width="130%" height="130%">
              <feGaussianBlur stdDeviation="3" />
            </filter>
          </defs>
          <circle data-layer="ring" cx="100" cy="100" r="92" stroke="var(--color-accent-hover)" stroke-width={ring.width} fill="none" filter="url(#ringBlur-{uid}-{ring.width})" />
        </svg>
      {/each}

      <svg class="layer" viewBox="-40 -40 280 280">
        <circle cx="100" cy="100" r="68" fill={BRAND_SILHOUETTE} />
      </svg>

      <svg class="layer" viewBox="-40 -40 280 280" style="transform-origin: {BURST_ORIGIN}; transform: scale({burstScale}); opacity: {burstOpacity};">
        <defs>
          <filter id="burstBlur-{uid}" x="-40%" y="-40%" width="180%" height="180%">
            <feGaussianBlur stdDeviation="10" />
          </filter>
        </defs>
        <circle data-layer="burst" cx="150.6" cy="59.6" r={BURST_BASE_R} fill={BRAND_BURST} filter="url(#burstBlur-{uid})" />
      </svg>

      <svg class="layer" viewBox="-40 -40 280 280">
        <circle cx="150.6" cy="59.6" r="17.4" fill={BRAND_BURST} />
      </svg>
    </div>
  {/if}
</button>

<style>
  svg {
    overflow: hidden;
    isolation: isolate;
  }

  .reactive-logo {
    will-change: filter;
  }

  /* Each layer is its own composited surface: audio updates change only
     transform/opacity, never the (blurred) SVG content. No CSS transitions:
     updates already arrive every ~33ms, and restarting a transition on six
     layers per update cost as much CPU as all the blur work saved. */
  .layer {
    position: absolute;
    inset: 0;
    width: 100%;
    height: 100%;
    will-change: transform, opacity;
  }
</style>
