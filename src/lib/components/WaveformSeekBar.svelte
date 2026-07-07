<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { playerStore } from "../stores/player.svelte";

  let canvas = $state<HTMLCanvasElement | null>(null);
  let waveformData = $state<number[]>([]);
  let isDragging = $state(false);

  // Fetch waveform when current song changes
  async function loadWaveform(songId: number | undefined) {
    if (songId === undefined) {
      waveformData = [];
      return;
    }
    try {
      const data = await invoke<number[] | null>("get_waveform_data", { songId });
      if (data) {
        waveformData = data;
      } else {
        // Fallback flat peaks if no waveform exists yet
        waveformData = Array(150).fill(40);
      }
    } catch (e) {
      console.error("Failed to load waveform:", e);
      waveformData = Array(150).fill(40);
    }
  }

  // Draw waveform
  function draw() {
    if (!canvas) return;
    const ctx = canvas.getContext("2d");
    if (!ctx) return;

    const dpr = window.devicePixelRatio || 1;
    const width = canvas.clientWidth;
    const height = canvas.clientHeight;

    if (canvas.width !== width * dpr || canvas.height !== height * dpr) {
      canvas.width = width * dpr;
      canvas.height = height * dpr;
      ctx.scale(dpr, dpr);
    }

    ctx.clearRect(0, 0, width, height);

    const data = waveformData.length > 0 ? waveformData : Array(150).fill(40);
    const numBars = data.length;
    const barGap = 1.5;
    const barWidth = (width - (numBars - 1) * barGap) / numBars;

    const songLength = playerStore.currentSong?.length_nanosec || 1;
    const progressPct = playerStore.positionNanosec / songLength;

    // Premium gradients for played part
    const gradPlayed = ctx.createLinearGradient(0, height, 0, 0);
    gradPlayed.addColorStop(0, "#7c3aed"); // violet-600
    gradPlayed.addColorStop(1, "#d946ef"); // fuchsia-500

    for (let i = 0; i < numBars; i++) {
      const val = data[i] / 255.0;
      // Center the bars vertically (Clementine/Strawberry style)
      const barHeight = Math.max(2, val * height * 0.85);
      const x = i * (barWidth + barGap);
      const y = (height - barHeight) / 2;

      const barPct = i / numBars;
      if (barPct <= progressPct) {
        ctx.fillStyle = gradPlayed;
      } else {
        ctx.fillStyle = "#374151"; // gray-700
      }

      ctx.beginPath();
      ctx.roundRect(x, y, barWidth, barHeight, 1);
      ctx.fill();
    }
  }

  // React to changes in currentSong using Svelte 5 $effect
  $effect(() => {
    loadWaveform(playerStore.currentSong?.id);
  });

  $effect(() => {
    // Redraw whenever position, length, or data updates
    const _pos = playerStore.positionNanosec;
    const _len = playerStore.currentSong?.length_nanosec;
    const _data = waveformData;
    draw();
  });

  // Handle seek actions (click / drag)
  function seekToX(clientX: number) {
    if (!canvas || !playerStore.currentSong) return;
    const rect = canvas.getBoundingClientRect();
    const x = Math.max(0, Math.min(clientX - rect.left, rect.width));
    const pct = x / rect.width;
    const targetNs = pct * (playerStore.currentSong.length_nanosec || 0);
    playerStore.seek(targetNs);
  }

  function handleMouseDown(e: MouseEvent) {
    if (!playerStore.currentSong) return;
    isDragging = true;
    seekToX(e.clientX);
  }

  function handleMouseMove(e: MouseEvent) {
    if (isDragging) {
      seekToX(e.clientX);
    }
  }

  function handleMouseUp() {
    isDragging = false;
  }
</script>

<svelte:window onmouseup={handleMouseUp} onmousemove={handleMouseMove} />

<!-- svelte-ignore a11y_no_static_element_interactions -->
<div
  onmousedown={handleMouseDown}
  class="relative flex-1 h-8 cursor-pointer flex items-center group select-none"
>
  <canvas bind:this={canvas} class="w-full h-7 opacity-75 group-hover:opacity-100 transition-opacity duration-200"></canvas>
</div>
