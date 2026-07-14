<script lang="ts">
  import { themeStore } from "../stores/theme.svelte";
  import { playerStore } from "../stores/player.svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { listen } from "@tauri-apps/api/event";
  import { onMount, onDestroy } from "svelte";

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
        const data = event.payload;
        if (data && data.length > 0) {
          // Segment and average the 32 spectrum bins into Bass, Mids, and Coronal/Treble
          // to prevent single-bin noise/leakage from driving unrelated components
          const bassAvg = data.slice(0, 8).reduce((sum, v) => sum + v, 0) / 8;
          const midAvg = data.slice(8, 20).reduce((sum, v) => sum + v, 0) / 12;
          const coronalAvg = data.slice(20, 32).reduce((sum, v) => sum + v, 0) / 12;

          // Calibrate each band's multiplier for average energy values
          bassIntensity = Math.min(1.0, bassAvg * 28.0);
          midIntensity = Math.min(1.0, midAvg * 28.0);
          coronalIntensity = Math.min(1.0, coronalAvg * 45.0);
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

  // Derive element opacities, glow radii, and saturation based on play and specific band intensities
  let bgOpacity = $derived(!isPlaying || !isPulsingEnabled ? 0.35 : 0.0 + midIntensity * 0.8);
  let ringOpacity = $derived(!isPlaying || !isPulsingEnabled ? 0.8 : 0.05 + coronalIntensity * 0.9);
  let burstOpacity = $derived(!isPlaying || !isPulsingEnabled ? 0.65 : 0.05 + bassIntensity * 0.95);

  let bgRadius = $derived(!isPlaying || !isPulsingEnabled ? 115 : 105 + midIntensity * 40);
  let ringRadius = $derived(!isPlaying || !isPulsingEnabled ? 120 : 110 + coronalIntensity * 30);
  let burstRadius = $derived(!isPlaying || !isPulsingEnabled ? 32 : 24 + bassIntensity * 16);

  let maxIntensity = $derived(Math.max(bassIntensity, midIntensity, coronalIntensity));
  let saturationVal = $derived(!isPlaying || !isPulsingEnabled ? 1.0 : 0.15 + maxIntensity * 2.35);
</script>

<button
  type="button"
  onclick={togglePulsing}
  class="bg-transparent border-none p-0 cursor-pointer focus:outline-none focus-visible:ring-2 focus-visible:ring-brand-accent rounded-full transition-shadow duration-300"
  title={isPulsingEnabled ? "Click to disable logo pulsing" : "Click to enable logo pulsing"}
  aria-label="Toggle Luminous logo pulsing"
>
  <svg
    xmlns="http://www.w3.org/2000/svg"
    viewBox="70 66 380 380"
    class="{sizeMap[size]} {className} select-none"
    aria-hidden="true"
  >
    <defs>
      <!-- Bass Gradient (Ambient Glow): Deep Accent to Accent -->
      <linearGradient id="bassGradient" x1="0%" y1="50%" x2="100%" y2="50%">
        <stop offset="0%" stop-color="var(--logo-stop-1, #3a0d00)" />
        <stop offset="100%" stop-color="var(--logo-stop-3, #ff7300)" />
      </linearGradient>

      <!-- Mid Gradient (Eclipse Ring): Deep Accent Hover to Accent Hover -->
      <linearGradient id="midGradient" x1="0%" y1="50%" x2="100%" y2="50%">
        <stop offset="0%" stop-color="var(--logo-stop-2, #5a1d00)" />
        <stop offset="100%" stop-color="var(--logo-stop-4, #ffcc00)" />
      </linearGradient>

      <!-- Adaptive glow filter -->
      <filter id="adaptiveGlow" x="-50%" y="-50%" width="200%" height="200%">
        <feGaussianBlur stdDeviation="12" result="blur1" />
        <feGaussianBlur stdDeviation="4" result="blur2" />
        <feMerge>
          <feMergeNode in="blur1" />
          <feMergeNode in="blur2" />
          <feMergeNode in="SourceGraphic" />
        </feMerge>
      </filter>
    </defs>

    <!-- Base container with dynamic saturation pulse -->
    <g
      style="filter: saturate({saturationVal}); transition: filter 0.05s ease-out;"
    >
      <!-- Ambient backplate glow -->
      <circle
        cx="280"
        cy="256"
        r={bgRadius}
        fill="url(#bassGradient)"
        opacity={bgOpacity}
        filter="url(#adaptiveGlow)"
        style="transition: r 0.05s ease-out;"
      />

      <!-- Reactive eclipse ring (thickened) -->
      <circle
        cx="256"
        cy="256"
        r={ringRadius}
        stroke="url(#midGradient)"
        stroke-width="26"
        fill="none"
        opacity={ringOpacity}
        filter="url(#adaptiveGlow)"
        style="transition: r 0.05s ease-out;"
      />

      <!-- White-hot sunrise burst (opacity increases when playing) -->
      <circle
        cx="368"
        cy="256"
        r={burstRadius}
        fill="#ffffff"
        filter="url(#adaptiveGlow)"
        opacity={burstOpacity}
        style="transition: r 0.05s ease-out;"
      />

      <!-- The planet disc -->
      <circle cx="252" cy="256" r="107" fill="#101015" />

      <!-- Inner border separating core disc from corona -->
      <circle cx="252" cy="256" r="107" stroke="var(--bg-main)" stroke-width="1.5" fill="none" />
    </g>
  </svg>
</button>

<style>
  svg {
    transition: filter 0.3s ease;
  }

  svg:hover {
    filter: drop-shadow(0 0 8px rgba(255, 255, 255, 0.2));
  }
</style>
