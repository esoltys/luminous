<script lang="ts">
  import { themeStore } from "../stores/theme.svelte";
  import { playerStore } from "../stores/player.svelte";

  interface Props {
    size?: "sm" | "md" | "lg";
    className?: string;
  }

  let { size = "md", className = "" }: Props = $props();

  // Map size to dimensions
  const sizeMap = {
    sm: "w-8 h-8",
    md: "w-12 h-12",
    lg: "w-16 h-16"
  };

  // Compute reactive gradient colors based on artwork colors
  let stop1 = $derived(getComputedStyle(document.documentElement).getPropertyValue("--color-artwork-primary") || "#3a0d00");
  let stop2 = $derived(getComputedStyle(document.documentElement).getPropertyValue("--color-artwork-accent") || "#c83200");
  let stop3 = $derived(getComputedStyle(document.documentElement).getPropertyValue("--color-artwork-accent") || "#ff7300");
  let stop4 = $derived(lighten(stop2, 0.2) || "#ffcc00");

  function lighten(hex: string, amount: number): string {
    const usePound = hex[0] === "#";
    const col = usePound ? hex.slice(1) : hex;
    const num = parseInt(col, 16);
    const r = Math.min(255, Math.floor(num / 65536) + Math.floor(255 * amount));
    const g = Math.min(255, Math.floor((num / 256) % 256) + Math.floor(255 * amount));
    const b = Math.min(255, Math.floor(num % 256) + Math.floor(255 * amount));
    return (usePound ? "#" : "") + (0x1000000 + r * 0x10000 + g * 0x100 + b).toString(16).slice(1);
  }

  let isPlaying = $derived(playerStore.state === "playing");
</script>

<svg
  xmlns="http://www.w3.org/2000/svg"
  viewBox="0 0 512 512"
  class="{sizeMap[size]} {className}"
  aria-label="Luminous"
>
  <defs>
    <!-- Reactive gradient based on artwork colors -->
    <linearGradient id="reactiveGradient" x1="0%" y1="20%" x2="100%" y2="50%">
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

    <!-- Squircle clip path for app icon aesthetic -->
    <clipPath id="squircle">
      <rect x="32" y="32" width="448" height="448" rx="105" ry="105" />
    </clipPath>
  </defs>

  <!-- Base dark canvas with reactive gradient -->
  <g clip-path="url(#squircle)">
    <!-- Deep space obsidian -->
    <rect x="0" y="0" width="512" height="512" fill="#08080b" />

    <!-- Subtle technical layout grids -->
    <circle cx="256" cy="256" r="215" stroke="#13131c" stroke-width="1.5" fill="none" />
    <circle cx="256" cy="256" r="175" stroke="#13131c" stroke-width="1" fill="none" stroke-dasharray="4 6" />

    <!-- Ambient backplate glow -->
    <circle cx="280" cy="256" r="115" fill="url(#reactiveGradient)" opacity="0.35" filter="url(#adaptiveGlow)" />

    <!-- Reactive eclipse ring (thickened) -->
    <circle cx="256" cy="256" r="120" stroke="url(#reactiveGradient)" stroke-width="26" fill="none" filter="url(#adaptiveGlow)" />

    <!-- White-hot sunrise burst (opacity increases when playing) -->
    <circle cx="368" cy="256" r="32" fill="#ffffff" filter="url(#adaptiveGlow)" opacity={isPlaying ? "0.8" : "0.65"} />

    <!-- The planet disc -->
    <circle cx="252" cy="256" r="107" fill="#101015" />

    <!-- Inner border -->
    <circle cx="252" cy="256" r="107" stroke="#08080b" stroke-width="1.5" fill="none" />
  </g>
</svg>

<style>
  svg {
    transition: filter 0.3s ease;
  }

  svg:hover {
    filter: drop-shadow(0 0 8px rgba(255, 255, 255, 0.2));
  }
</style>
