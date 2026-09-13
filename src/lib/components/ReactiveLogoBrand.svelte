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
  // from the current one. The three intensities each drive an SVG element
  // filtered with feGaussianBlur (glow/rim/ring/burst below) — changing their
  // r/stroke-width forces the browser to re-rasterize that blur, which is far
  // more expensive than a plain compositor update. Skipping no-op-sized
  // writes at up to ~30fps (spectrum-data cadence) cuts that re-rasterization
  // during quiet/steady passages without touching how peaks render.
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

  // Ambient glow (mid-driven): soft blurred halo, active only while playing
  let glowRadius = $derived(isResting ? 0 : 95 + midIntensity * 35);
  let glowOpacity = $derived(isResting ? 0 : 0.2 + midIntensity * 0.7);

  // Ambient glow inner rim (mid-driven): crisp ring right at the disc's edge at rest, pulsing softly when playing
  let innerRimStroke = $derived(isResting ? BRAND_INDIGO : "var(--color-accent)");
  let innerRimWidth = $derived(isResting ? 14 : 12 + midIntensity * 12);
  let innerRimOpacity = $derived(isResting ? 1.0 : 0.7 + midIntensity * 0.3);

  // Eclipse ring (treble-driven): brand gold at rest, re-targets to active theme's accent-hover when playing
  let ringStroke = $derived(isResting ? BRAND_GOLD : "var(--color-accent-hover)");
  let ringWidth = $derived(isResting ? 8 : 5 + coronalIntensity * 16);
  let ringOpacity = $derived(isResting ? 1.0 : 0.5 + coronalIntensity * 0.5);

  // Coronal burst (bass-driven): the halo pulses when playing; the core dot stays
  // fixed, matching the canonical mark's white highlight at (150.6, 59.6) with r=17.4
  let burstRadius = $derived(isResting ? 0 : 18 + bassIntensity * 26);
  let burstOpacity = $derived(isResting ? 0 : 0.15 + bassIntensity * 0.7);

  let maxIntensity = $derived(Math.max(bassIntensity, midIntensity, coronalIntensity));
  let saturationVal = $derived(isResting ? 1.0 : 0.15 + maxIntensity * 2.35);
</script>

<button
  type="button"
  onclick={togglePulsing}
  class="bg-transparent border-none p-0 focus:outline-none focus-visible:ring-2 focus-visible:ring-brand-accent rounded-full overflow-hidden isolate"
  title={isPulsingEnabled ? i18n.t('common.disableLogoPulse') : i18n.t('common.enableLogoPulse')}
  aria-label={i18n.t('common.toggleLogoPulsing')}
>
  <svg
    xmlns="http://www.w3.org/2000/svg"
    viewBox="-40 -40 280 280"
    class="{sizeMap[size]} {className} select-none"
    aria-hidden="true"
  >
    <defs>
      <!-- Filters mirror docs/luminous-mark-reactive.svg's three distinct
           blur passes — one per layer, not a single shared filter -->
      <filter id="glowBlurOuter" x="-30%" y="-30%" width="160%" height="160%">
        <feGaussianBlur stdDeviation="22" />
      </filter>
      <filter id="ringBlur" x="-15%" y="-15%" width="130%" height="130%">
        <feGaussianBlur stdDeviation="3" />
      </filter>
      <filter id="burstBlur" x="-40%" y="-40%" width="180%" height="180%">
        <feGaussianBlur stdDeviation="10" />
      </filter>
    </defs>

    <g
      style="filter: {isResting ? 'none' : `saturate(${saturationVal})`}; transition: filter 0.05s ease-out;"
    >
      <!-- Ambient glow (mid-driven): soft blurred halo, active only while playing -->
      <circle
        cx="100"
        cy="100"
        r={glowRadius}
        fill="var(--color-accent)"
        opacity={glowOpacity}
        filter={isResting ? undefined : "url(#glowBlurOuter)"}
        style="transition: r 0.05s ease-out, opacity 0.3s ease;"
      />

      <!-- Ambient glow inner rim (mid-driven): crisp ring right at the disc's edge at rest, softly blurred when playing -->
      <circle
        cx="100"
        cy="100"
        r="77"
        stroke={innerRimStroke}
        stroke-width={innerRimWidth}
        fill="none"
        opacity={innerRimOpacity}
        filter={isResting ? undefined : "url(#ringBlur)"}
        style="transition: stroke 0.3s ease, stroke-width 0.05s ease-out, opacity 0.3s ease;"
      />

      <!-- Eclipse ring (treble-driven): re-targets to the active theme's
           accent-hover when playing -->
      <circle
        cx="100"
        cy="100"
        r="92"
        stroke={ringStroke}
        stroke-width={ringWidth}
        fill="none"
        opacity={ringOpacity}
        filter={isResting ? undefined : "url(#ringBlur)"}
        style="transition: stroke 0.3s ease, stroke-width 0.05s ease-out, opacity 0.3s ease;"
      />

      <!-- The planet disc -->
      <circle cx="100" cy="100" r="68" fill={BRAND_SILHOUETTE} />

      <!-- Coronal burst halo (bass-driven) -->
      <circle
        cx="150.6"
        cy="59.6"
        r={burstRadius}
        fill={BRAND_BURST}
        filter={isResting ? undefined : "url(#burstBlur)"}
        opacity={burstOpacity}
        style="transition: r 0.05s ease-out, opacity 0.3s ease;"
      />

      <!-- Coronal burst core: fixed, always-visible white highlight matching docs/luminous-mark.svg -->
      <circle cx="150.6" cy="59.6" r="17.4" fill={BRAND_BURST} />
    </g>
  </svg>
</button>

<style>
  svg {
    overflow: hidden;
    isolation: isolate;
  }
</style>
