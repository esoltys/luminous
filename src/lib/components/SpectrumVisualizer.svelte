<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { listen } from "@tauri-apps/api/event";
  import { onMount, onDestroy } from "svelte";
  import { themeStore } from "../stores/theme.svelte";

  let canvas = $state<HTMLCanvasElement | null>(null);
  let unlisten: (() => void) | null = null;
  let spectrumData = $state<number[]>(Array(32).fill(0));

  function hexToRgb(hex: string): { r: number; g: number; b: number } | null {
    const shorthandRegex = /^#?([a-f\d])([a-f\d])([a-f\d])$/i;
    const fullHex = hex.replace(shorthandRegex, (_, r, g, b) => r + r + g + g + b + b);
    const result = /^#?([a-f\d]{2})([a-f\d]{2})([a-f\d]{2})$/i.exec(fullHex);
    return result
      ? {
          r: parseInt(result[1], 16),
          g: parseInt(result[2], 16),
          b: parseInt(result[3], 16)
        }
      : null;
  }

  function draw() {
    if (!canvas) return;
    const ctx = canvas.getContext("2d");
    if (!ctx) return;

    // Handle high DPI screens
    const dpr = window.devicePixelRatio || 1;
    const width = canvas.clientWidth;
    const height = canvas.clientHeight;

    if (canvas.width !== width * dpr || canvas.height !== height * dpr) {
      canvas.width = width * dpr;
      canvas.height = height * dpr;
      ctx.scale(dpr, dpr);
    }

    ctx.clearRect(0, 0, width, height);

    const numBars = spectrumData.length;
    const barGap = 2.5;
    const barWidth = (width - (numBars - 1) * barGap) / numBars;

    // Dynamically read active theme colors from document styles
    const accentColor = getComputedStyle(document.documentElement).getPropertyValue('--color-accent').trim() || '#8b5cf6';
    const hoverColor = getComputedStyle(document.documentElement).getPropertyValue('--color-accent-hover').trim() || '#a78bfa';

    const rgb = hexToRgb(accentColor) || { r: 139, g: 92, b: 246 };

    // Premium gradient: custom or themed accent to hover
    const grad = ctx.createLinearGradient(0, height, 0, 0);
    grad.addColorStop(0, `rgba(${rgb.r}, ${rgb.g}, ${rgb.b}, 0.2)`); // faint accent color at bottom
    grad.addColorStop(0.5, `rgba(${rgb.r}, ${rgb.g}, ${rgb.b}, 0.8)`); // strong accent color
    grad.addColorStop(1, hoverColor); // accent hover color

    ctx.fillStyle = grad;

    for (let i = 0; i < numBars; i++) {
      const val = spectrumData[i] || 0.0;
      // Apply scaling so the bars dance satisfyingly
      const barHeight = Math.max(1.5, val * height * 14.0);

      const x = i * (barWidth + barGap);
      const y = height - barHeight;

      // Draw rounded rectangle for bar
      ctx.beginPath();
      ctx.roundRect(x, y, barWidth, barHeight, [1.5, 1.5, 0, 0]);
      ctx.fill();
    }
  }

  $effect(() => {
    // Redraw when theme changes
    const _theme = themeStore.activeThemeId;
    draw();
  });

  onMount(async () => {
    try {
      await invoke("set_spectrum_enabled", { enabled: true });
      unlisten = await listen<number[]>("spectrum-data", (event) => {
        spectrumData = event.payload;
        draw();
      });
    } catch (e) {
      console.error("Failed to initialize spectrum visualizer:", e);
    }
  });

  onDestroy(async () => {
    try {
      await invoke("set_spectrum_enabled", { enabled: false });
    } catch (e) {
      console.error("Failed to disable spectrum visualizer:", e);
    }
    if (unlisten) {
      unlisten();
    }
  });
</script>

<canvas bind:this={canvas} class="w-full h-full opacity-85 hover:opacity-100 transition-opacity duration-300"></canvas>

