<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { playerStore } from "../stores/player.svelte";
  import { themeStore } from "../stores/theme.svelte";

  let canvas = $state<HTMLCanvasElement | null>(null);
  let waveformData = $state<number[]>([]);
  let isDragging = $state(false);

  // Guards a slow, still-in-flight request from a previously-skipped-past
  // track from overwriting waveformData after a newer track has already
  // taken over (e.g. the in-flight request settles just after another skip).
  let waveformRequestId = 0;

  // Fetch waveform when current song changes. get_waveform_data() falls back
  // to a full offline decode of the audio file (decode_all_samples) on a
  // cache miss, which is expensive — rapid-fire skips must not each trigger
  // one, or a burst of skips queues up several concurrent full-file decodes
  // that compete with real-time playback for CPU/disk and can make the
  // whole app feel stuck until they drain. Debounced in the $effect below.
  async function loadWaveform(songId: number | undefined) {
    const requestId = ++waveformRequestId;
    if (songId === undefined) {
      waveformData = [];
      return;
    }
    try {
      const data = await invoke<number[] | null>("get_waveform_data", { songId });
      if (requestId !== waveformRequestId) return; // superseded by a newer track
      if (data) {
        waveformData = data;
      } else {
        // Fallback flat peaks if no waveform exists yet
        waveformData = Array(150).fill(40);
      }
    } catch (e) {
      if (requestId !== waveformRequestId) return;
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

    // Dynamically read active theme colors from document styles
    const accentColor = getComputedStyle(document.documentElement).getPropertyValue('--color-accent').trim() || '#8b5cf6';
    const hoverColor = getComputedStyle(document.documentElement).getPropertyValue('--color-accent-hover').trim() || '#a78bfa';
    const borderCol = getComputedStyle(document.documentElement).getPropertyValue('--color-border').trim() || '#374151';

    // Premium gradients for played part
    const gradPlayed = ctx.createLinearGradient(0, height, 0, 0);
    gradPlayed.addColorStop(0, accentColor);
    gradPlayed.addColorStop(1, hoverColor);

    for (let i = 0; i < numBars; i++) {
      const val = data[i] / 255.0;
      // Center the bars vertically
      const barHeight = Math.max(2, val * height * 0.85);
      const x = i * (barWidth + barGap);
      const y = (height - barHeight) / 2;

      const barPct = i / numBars;
      if (barPct <= progressPct) {
        ctx.fillStyle = gradPlayed;
      } else {
        ctx.fillStyle = borderCol;
      }

      ctx.beginPath();
      ctx.roundRect(x, y, barWidth, barHeight, 1);
      ctx.fill();
    }
  }

  // React to changes in currentSong using Svelte 5 $effect. Debounced: the
  // cleanup callback cancels the pending timer whenever songId changes again
  // before it fires, so a burst of rapid skips only ever loads the waveform
  // for whichever track the user actually settles on.
  $effect(() => {
    const songId = playerStore.currentSong?.id;
    const timer = setTimeout(() => loadWaveform(songId), 300);
    return () => clearTimeout(timer);
  });

  $effect(() => {
    // Redraw whenever position, length, theme, or data updates
    const _pos = playerStore.positionNanosec;
    const _len = playerStore.currentSong?.length_nanosec;
    const _theme = themeStore.activeThemeId;
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
  <canvas bind:this={canvas} class="w-full h-7 opacity-100"></canvas>
</div>
