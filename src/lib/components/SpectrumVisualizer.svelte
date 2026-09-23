<script lang="ts">
  import { listen } from "@tauri-apps/api/event";
  import { onMount, onDestroy } from "svelte";
  import { themeStore } from "../stores/theme.svelte";
  import { acquireSpectrum, releaseSpectrum } from "../utils/spectrumEnable";

  let canvas = $state<HTMLCanvasElement | null>(null);
  let unlisten: (() => void) | null = null;
  // Plain (non-reactive) on purpose: draw() reads it, and as $state the
  // theme $effect below would also re-run on every spectrum event, drawing
  // each frame twice.
  let spectrumData: number[] = Array(32).fill(0);
  let pendingFrame = 0;
  let accentColor = "#8b5cf6";
  let hoverColor = "#a78bfa";

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
    if (typeof document !== "undefined" && document.hidden) return;
    if (!canvas) return;
    const ctx = canvas.getContext("2d");
    if (!ctx) return;

    const dpr = window.devicePixelRatio || 1;
    const width = canvas.clientWidth;
    const height = canvas.clientHeight;
    if (width === 0 || height === 0) return;

    if (canvas.width !== width * dpr || canvas.height !== height * dpr) {
      canvas.width = width * dpr;
      canvas.height = height * dpr;
      ctx.scale(dpr, dpr);
    }

    ctx.clearRect(0, 0, width, height);

    const numBars = spectrumData.length;
    const barGap = 2.5;
    const barWidth = (width - (numBars - 1) * barGap) / numBars;

    const rgb = hexToRgb(accentColor) || { r: 139, g: 92, b: 246 };

    const grad = ctx.createLinearGradient(0, height, 0, 0);
    grad.addColorStop(0, `rgba(${rgb.r}, ${rgb.g}, ${rgb.b}, 0.2)`);
    grad.addColorStop(0.5, `rgba(${rgb.r}, ${rgb.g}, ${rgb.b}, 0.8)`);
    grad.addColorStop(1, hoverColor);

    ctx.fillStyle = grad;

    for (let i = 0; i < numBars; i++) {
      const val = spectrumData[i] || 0.0;
      // Bins already arrive normalized to the frame's own peak (see
      // analyzer::calculate_spectrum), so no extra gain is needed here —
      // a large fixed multiplier would just clip most bars to the same
      // "maxed out" height instead of showing their real relative shape.
      const barHeight = Math.min(height, Math.max(1.5, val * height));

      const x = i * (barWidth + barGap);
      const y = height - barHeight;

      ctx.beginPath();
      ctx.roundRect(x, y, barWidth, barHeight, [1.5, 1.5, 0, 0]);
      ctx.fill();
    }
  }

  // Coalesces bursts of spectrum events into at most one draw per display
  // frame, painted in step with the compositor rather than mid-frame.
  function scheduleDraw() {
    if (pendingFrame) return;
    pendingFrame = requestAnimationFrame(() => {
      pendingFrame = 0;
      draw();
    });
  }

  $effect(() => {
    const colors = themeStore.resolvedColors;
    accentColor = colors["color-accent"] || '#8b5cf6';
    hoverColor = colors["color-accent-hover"] || '#a78bfa';
    scheduleDraw();
  });

  function handleVisibilityChange() {
    if (typeof document !== "undefined" && !document.hidden) {
      scheduleDraw();
    }
  }

  onMount(async () => {
    acquireSpectrum();
    if (typeof document !== "undefined") {
      document.addEventListener("visibilitychange", handleVisibilityChange);
    }
    try {
      unlisten = await listen<number[]>("spectrum-data", (event) => {
        spectrumData = event.payload;
        if (typeof document === "undefined" || !document.hidden) {
          scheduleDraw();
        }
      });
    } catch (e) {
      console.error("Failed to initialize spectrum visualizer:", e);
    }
  });

  onDestroy(() => {
    if (pendingFrame) cancelAnimationFrame(pendingFrame);
    releaseSpectrum();
    if (typeof document !== "undefined") {
      document.removeEventListener("visibilitychange", handleVisibilityChange);
    }
    if (unlisten) {
      unlisten();
    }
  });
</script>

<canvas bind:this={canvas} class="w-full h-full opacity-85 hover:opacity-100 transition-opacity duration-300"></canvas>

