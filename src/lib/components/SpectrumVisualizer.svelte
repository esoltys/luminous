<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { listen } from "@tauri-apps/api/event";
  import { onMount, onDestroy } from "svelte";

  let canvas = $state<HTMLCanvasElement | null>(null);
  let unlisten: (() => void) | null = null;
  let spectrumData = $state<number[]>(Array(32).fill(0));

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

    // Premium gradient: violet to pink
    const grad = ctx.createLinearGradient(0, height, 0, 0);
    grad.addColorStop(0, "rgba(139, 92, 246, 0.2)"); // faint violet at bottom
    grad.addColorStop(0.5, "rgba(139, 92, 246, 0.8)"); // violet-500
    grad.addColorStop(1, "#ec4899"); // pink-500

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
