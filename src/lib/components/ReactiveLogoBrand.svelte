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
          const avg = weightedSum / data.length;
          // Scale it so that middle frequencies correspond to normal brightness (~0.5)
          // and pinned/high-frequency peaks reach maximum brightness (~1.0).
          pulseIntensity = Math.min(1.0, avg * 25);
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

  // Reactive dependencies to trigger stop color calculations when activeThemeId or currentSong changes
  let activeThemeId = $derived(themeStore.activeThemeId);
  let currentSong = $derived(playerStore.currentSong);

  // Compute reactive gradient colors based on active theme accent colors
  let stop1 = $derived.by(() => {
    const _t = activeThemeId;
    const _s = currentSong;
    if (typeof document === "undefined") return "#3a0d00";
    const accent = getComputedStyle(document.documentElement).getPropertyValue("--color-accent").trim() || "#8b5cf6";
    return darken(accent, 0.6) || "#3a0d00";
  });

  let stop2 = $derived.by(() => {
    const _t = activeThemeId;
    const _s = currentSong;
    if (typeof document === "undefined") return "#c83200";
    const accent = getComputedStyle(document.documentElement).getPropertyValue("--color-accent").trim() || "#8b5cf6";
    return darken(accent, 0.2) || "#c83200";
  });

  let stop3 = $derived.by(() => {
    const _t = activeThemeId;
    const _s = currentSong;
    if (typeof document === "undefined") return "#ff7300";
    return getComputedStyle(document.documentElement).getPropertyValue("--color-accent").trim() || "#ff7300";
  });

  let stop4 = $derived.by(() => {
    const _t = activeThemeId;
    const _s = currentSong;
    if (typeof document === "undefined") return "#ffcc00";
    return getComputedStyle(document.documentElement).getPropertyValue("--color-accent-hover").trim() || "#a78bfa";
  });

  function darken(hex: string, amount: number): string {
    if (!hex.startsWith("#")) return hex;
    const usePound = hex[0] === "#";
    const col = usePound ? hex.slice(1) : hex;
    const num = parseInt(col, 16);
    const r = Math.max(0, Math.floor(((num / 65536) % 256) * (1 - amount)));
    const g = Math.max(0, Math.floor(((num / 256) % 256) * (1 - amount)));
    const b = Math.max(0, Math.floor((num % 256) * (1 - amount)));
    return (usePound ? "#" : "") + (0x1000000 + r * 0x10000 + g * 0x100 + b).toString(16).slice(1);
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
        <stop offset="0%" stop-color={stop1} />
        <stop offset="30%" stop-color={stop2} />
        <stop offset="65%" stop-color={stop3} />
        <stop offset="88%" stop-color={stop4} />
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
