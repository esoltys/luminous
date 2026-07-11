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
  let pulseIntensity = $state(0);
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
          pulseIntensity = 0;
          return;
        }
        const data = event.payload;
        if (data && data.length > 0) {
          // Weight the bins so that higher frequencies (right-side of visualizer)
          // have a much stronger influence on the logo brightness/luminance.
          let weightedSum = 0;
          let weightTotal = 0;
          for (let i = 0; i < data.length; i++) {
            // Quadratic weight scaling from 0.1 (low bass) to 8.1 (high treble)
            const weight = 0.1 + Math.pow(i / (data.length - 1), 2) * 8.0;
            weightedSum += data[i] * weight;
            weightTotal += weight;
          }
          const avg = weightedSum / weightTotal; // True weighted average
          // Scale so that an average of ~0.034 gives ~75% pulse intensity
          pulseIntensity = Math.min(1.0, avg * 22);
        } else {
          pulseIntensity = 0;
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
      pulseIntensity = 0;
    } else {
      await invoke("set_spectrum_enabled", { enabled: true }).catch((err) =>
        console.error("Failed to enable spectrum on toggle:", err)
      );
    }
  }

  let isPlaying = $derived(playerStore.state === "playing");

  // Derive element opacities, glow radii, and saturation based on play and pulse intensity
  let bgOpacity = $derived(!isPlaying || !isPulsingEnabled ? 0.35 : 0.05 + pulseIntensity * 0.75);
  let ringOpacity = $derived(!isPlaying || !isPulsingEnabled ? 0.8 : 0.15 + pulseIntensity * 0.85);
  let burstOpacity = $derived(!isPlaying || !isPulsingEnabled ? 0.65 : 0.1 + pulseIntensity * 0.9);

  let bgRadius = $derived(!isPlaying || !isPulsingEnabled ? 115 : 105 + pulseIntensity * 40);
  let ringRadius = $derived(!isPlaying || !isPulsingEnabled ? 120 : 110 + pulseIntensity * 30);
  let burstRadius = $derived(!isPlaying || !isPulsingEnabled ? 32 : 24 + pulseIntensity * 16);

  let saturationVal = $derived(!isPlaying || !isPulsingEnabled ? 1.0 : 0.15 + pulseIntensity * 2.35);
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
    viewBox="0 0 512 512"
    class="{sizeMap[size]} {className} select-none"
    aria-hidden="true"
  >
    <defs>
      <!-- Reactive gradient based on theme colors -->
      <linearGradient id="reactiveGradient" x1="0%" y1="50%" x2="100%" y2="50%">
        <stop offset="0%" stop-color="var(--logo-stop-1, #3a0d00)" />
        <stop offset="30%" stop-color="var(--logo-stop-2, #c83200)" />
        <stop offset="65%" stop-color="var(--logo-stop-3, #ff7300)" />
        <stop offset="88%" stop-color="var(--logo-stop-4, #ffcc00)" />
        <stop offset="100%" stop-color="#ffffff" />
      </linearGradient>

      <!-- Adaptive glow filter -->
      <filter id="adaptiveGlow" x="-50%" y="-50%" width="200%" height="200%">
        <feGaussianBlur stdDeviation="28" result="blur1" />
        <feGaussianBlur stdDeviation="10" result="blur2" />
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
        fill="url(#reactiveGradient)"
        opacity={bgOpacity}
        filter="url(#adaptiveGlow)"
        style="transition: r 0.05s ease-out;"
      />

      <!-- Reactive eclipse ring (thickened) -->
      <circle
        cx="256"
        cy="256"
        r={ringRadius}
        stroke="url(#reactiveGradient)"
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
