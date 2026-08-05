<script lang="ts">
  import { themeStore } from "../stores/theme.svelte";
  import { playerStore } from "../stores/player.svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { listen } from "@tauri-apps/api/event";
  import { onMount, onDestroy } from "svelte";
  import { i18n } from "../stores/i18n.svelte";

  interface Props {
    size?: "sm" | "md" | "lg" | "xl";
    className?: string;
  }

  let { size = "md", className = "" }: Props = $props();

  // Map size to dimensions
  const sizeMap = {
    sm: "w-8 h-8",
    md: "w-12 h-12",
    lg: "w-16 h-16",
    xl: "w-20 h-20"
  };

  // State variables for pulsing
  let isPulsingEnabled = $state(true);
  let bassIntensity = $state(0);
  let midIntensity = $state(0);
  let coronalIntensity = $state(0);
  let unlisten: (() => void) | null = null;
  // Recomputing the SVG saturate() filter over the already-blurred glow
  // circles is expensive; the backend streams spectrum-data at 30fps, but
  // updating this component that often causes visible jank elsewhere on
  // the page (e.g. flicker in other views' blurred backdrops). Halving the
  // update rate keeps the pulse looking reactive at a much lower cost.
  let frameSkip = 0;

  onMount(async () => {
    const stored = localStorage.getItem("logo_pulsing");
    if (stored !== null) {
      isPulsingEnabled = stored === "true";
    }

    if (isPulsingEnabled) {
      await invoke("set_spectrum_enabled", { enabled: true }).catch((err) =>
        console.error("Failed to enable spectrum in logo mount:", err)
      );
    }

    // Listen to real-time audio spectrum
    try {
      unlisten = await listen<number[]>("spectrum-data", (event) => {
        if (!isPulsingEnabled) {
          bassIntensity = 0;
          midIntensity = 0;
          coronalIntensity = 0;
          return;
        }
        frameSkip = (frameSkip + 1) % 2;
        if (frameSkip !== 0) return;

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
          bassIntensity = Math.min(1.0, bassAvg * 1.4);
          midIntensity = Math.min(1.0, midAvg * 1.4);
          coronalIntensity = Math.min(1.0, coronalAvg * 1.6);
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
  });

  async function togglePulsing() {
    isPulsingEnabled = !isPulsingEnabled;
    localStorage.setItem("logo_pulsing", String(isPulsingEnabled));
    if (!isPulsingEnabled) {
      bassIntensity = 0;
      midIntensity = 0;
      coronalIntensity = 0;
    } else {
      await invoke("set_spectrum_enabled", { enabled: true }).catch((err) =>
        console.error("Failed to enable spectrum on toggle:", err)
      );
    }
  }

  let isPlaying = $derived(playerStore.state === "playing");

  // Geometry and resting values below mirror docs/luminous-mark-reactive.svg
  // exactly: centered at (100,100), disc r=68, indigo inner rim r=77, gold
  // eclipse ring r=92, burst at (152,57). That asset (not app-icon.svg/
  // luminous-mark.svg, which are the crisp static variant — see DESIGN.md's
  // "Two artifacts" section) is the lush, pre-blurred baseline for this
  // in-app element; when paused/pulsing-disabled these derive to its own
  // literal opacity/width values, so the logo is indistinguishable from the
  // reactive mark's resting frame.

  // Ambient glow (mid-driven): the blurred halo plus its crisp inner rim —
  // both re-target to the active theme's accent color, per the Luminous
  // Logo System spec.
  let glowRadius = $derived(!isPlaying || !isPulsingEnabled ? 112 : 95 + midIntensity * 35);
  let glowOpacity = $derived(!isPlaying || !isPulsingEnabled ? 0.4 : 0.2 + midIntensity * 0.7);
  let innerRimWidth = $derived(!isPlaying || !isPulsingEnabled ? 16 : 12 + midIntensity * 12);
  let innerRimOpacity = $derived(!isPlaying || !isPulsingEnabled ? 0.95 : 0.7 + midIntensity * 0.3);

  // Eclipse ring (treble-driven): re-targets to the active theme's
  // accent-hover, one step brighter than the glow.
  let ringWidth = $derived(!isPlaying || !isPulsingEnabled ? 9 : 5 + coronalIntensity * 16);
  let ringOpacity = $derived(!isPlaying || !isPulsingEnabled ? 0.85 : 0.5 + coronalIntensity * 0.5);

  // Coronal burst (bass-driven): the halo pulses; the small core dot stays
  // fixed, matching the reactive mark's always-visible white highlight.
  let burstRadius = $derived(!isPlaying || !isPulsingEnabled ? 26 : 18 + bassIntensity * 26);
  let burstOpacity = $derived(!isPlaying || !isPulsingEnabled ? 0.4 : 0.15 + bassIntensity * 0.7);

  let maxIntensity = $derived(Math.max(bassIntensity, midIntensity, coronalIntensity));
  let saturationVal = $derived(!isPlaying || !isPulsingEnabled ? 1.0 : 0.15 + maxIntensity * 2.35);
</script>

<button
  type="button"
  onclick={togglePulsing}
  class="bg-transparent border-none p-0 cursor-pointer focus:outline-none focus-visible:ring-2 focus-visible:ring-brand-accent rounded-full overflow-hidden isolate transition-shadow duration-300"
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

    <!-- Base container with dynamic saturation pulse -->
    <g
      style="filter: saturate({saturationVal}); transition: filter 0.05s ease-out;"
    >
      <!-- Ambient glow (mid-driven): soft blurred halo -->
      <circle
        cx="100"
        cy="100"
        r={glowRadius}
        fill="var(--color-accent)"
        opacity={glowOpacity}
        filter="url(#glowBlurOuter)"
        style="transition: r 0.05s ease-out;"
      />

      <!-- Ambient glow inner rim (mid-driven): softly blurred ring right at the disc's edge -->
      <circle
        cx="100"
        cy="100"
        r="77"
        stroke="var(--color-accent)"
        stroke-width={innerRimWidth}
        fill="none"
        opacity={innerRimOpacity}
        filter="url(#ringBlur)"
        style="transition: stroke-width 0.05s ease-out;"
      />

      <!-- Eclipse ring (treble-driven): re-targets to the active theme's
           accent-hover, one step brighter than the glow -->
      <circle
        cx="100"
        cy="100"
        r="92"
        stroke="var(--color-accent-hover)"
        stroke-width={ringWidth}
        fill="none"
        opacity={ringOpacity}
        filter="url(#ringBlur)"
        style="transition: stroke-width 0.05s ease-out;"
      />

      <!-- The planet disc -->
      <circle cx="100" cy="100" r="68" fill="#0A0A0D" />

      <!-- Inner border separating core disc from corona -->
      <circle cx="100" cy="100" r="68" stroke="var(--bg-main)" stroke-width="1.5" fill="none" />

      <!-- Coronal burst halo (bass-driven) -->
      <circle
        cx="152"
        cy="57"
        r={burstRadius}
        fill="#ffffff"
        filter="url(#burstBlur)"
        opacity={burstOpacity}
        style="transition: r 0.05s ease-out;"
      />

      <!-- Coronal burst core: fixed, always-visible white highlight -->
      <circle cx="152" cy="57" r="12" fill="#ffffff" />
    </g>
  </svg>
</button>

<style>
  svg {
    overflow: hidden;
    isolation: isolate;
    transition: filter 0.3s ease;
  }

  svg:hover {
    filter: drop-shadow(0 0 8px rgba(255, 255, 255, 0.2));
  }
</style>
