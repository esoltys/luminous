<script lang="ts">
  import { listen } from "@tauri-apps/api/event";
  import { onMount, onDestroy } from "svelte";
  import { i18n } from "../stores/i18n.svelte";
  import { playerStore } from "../stores/player.svelte";
  import { themeStore } from "../stores/theme.svelte";
  import { windowLayoutStore } from "../stores/windowLayout.svelte";
  import { isLiveSong } from "../types";
  import { prefersReducedMotion } from "../utils/motion";
  import { acquireSpectrum, releaseSpectrum } from "../utils/spectrumEnable";
  import {
    VOXEL_VISUALIZER_PRESET,
    VoxelSpectrumEngine,
    buildVoxelThemePalette,
    type VisualizerPreset,
    type VoxelThemePalette,
  } from "../utils/visualizerEngine";
  import CoverArt from "./CoverArt.svelte";

  interface Props {
    active?: boolean;
    preset?: VisualizerPreset;
  }

  let { active = true, preset = VOXEL_VISUALIZER_PRESET }: Props = $props();

  const COLUMNS = 64;
  const ROWS = 28;
  const engine = new VoxelSpectrumEngine(COLUMNS, ROWS);

  let canvas = $state<HTMLCanvasElement | null>(null);
  let unlisten: (() => void) | null = null;
  let resizeObserver: ResizeObserver | null = null;
  let rafId = 0;
  let lastFrameTime = 0;
  let hasAcquiredSpectrum = false;
  let palette: VoxelThemePalette = buildVoxelThemePalette(themeStore.resolvedColors, ROWS);

  function syncSpectrumSubscription(shouldAcquire: boolean) {
    if (shouldAcquire && !hasAcquiredSpectrum) {
      acquireSpectrum();
      hasAcquiredSpectrum = true;
    } else if (!shouldAcquire && hasAcquiredSpectrum) {
      releaseSpectrum();
      hasAcquiredSpectrum = false;
    }
  }

  function stopLoop() {
    if (rafId) {
      cancelAnimationFrame(rafId);
      rafId = 0;
    }
    lastFrameTime = 0;
  }

  function drawFrame() {
    if (typeof document !== "undefined" && document.hidden) return;
    if (!canvas) return;
    const ctx = canvas.getContext("2d");
    if (!ctx) return;

    const dpr = window.devicePixelRatio || 1;
    const width = canvas.clientWidth;
    const height = canvas.clientHeight;
    if (width === 0 || height === 0) return;

    const targetW = Math.round(width * dpr);
    const targetH = Math.round(height * dpr);
    if (canvas.width !== targetW || canvas.height !== targetH) {
      canvas.width = targetW;
      canvas.height = targetH;
    }

    ctx.save();
    ctx.scale(dpr, dpr);
    preset.render(ctx, width, height, engine, palette, {
      reducedMotion: prefersReducedMotion(),
    });
    ctx.restore();
  }

  function tick(now: number) {
    rafId = 0;
    if (!active || (typeof document !== "undefined" && document.hidden)) {
      lastFrameTime = 0;
      return;
    }

    const dtMs = lastFrameTime > 0 ? now - lastFrameTime : 16.67;
    lastFrameTime = now;

    const stillMoving = engine.step(dtMs, prefersReducedMotion());
    drawFrame();

    if (stillMoving || playerStore.state === "playing") {
      rafId = requestAnimationFrame(tick);
    } else {
      lastFrameTime = 0;
    }
  }

  function ensureLoop() {
    if (!active || (typeof document !== "undefined" && document.hidden)) return;
    if (!rafId) {
      rafId = requestAnimationFrame(tick);
    }
  }

  // Re-build the voxel color ramp whenever any theme setting changes
  $effect(() => {
    const colors = themeStore.resolvedColors;
    palette = buildVoxelThemePalette(colors, ROWS);
    if (active) {
      drawFrame();
    }
  });

  // Acquire spectrum and run loop only while this immersive view is active
  $effect(() => {
    syncSpectrumSubscription(active);
    if (active) {
      drawFrame();
      if (playerStore.state === "playing") {
        ensureLoop();
      }
    } else {
      stopLoop();
    }
  });

  // When playback pauses or stops, zero the target bins so bars and peak caps fall to rest
  $effect(() => {
    const state = playerStore.state;
    if (state !== "playing") {
      engine.clearTargets();
      if (active) {
        ensureLoop();
      }
    }
  });

  function handleVisibilityChange() {
    if (typeof document === "undefined") return;
    if (document.hidden) {
      stopLoop();
    } else if (active) {
      drawFrame();
      ensureLoop();
    }
  }

  function switchToCoverView() {
    windowLayoutStore.toggleImmersiveMode("cover");
  }

  onMount(async () => {
    if (typeof document !== "undefined") {
      document.addEventListener("visibilitychange", handleVisibilityChange);
    }
    if (canvas && typeof ResizeObserver !== "undefined") {
      resizeObserver = new ResizeObserver(() => {
        if (active) drawFrame();
      });
      resizeObserver.observe(canvas);
    }
    try {
      unlisten = await listen<number[]>("spectrum-data", (event) => {
        engine.pushSpectrum(event.payload);
        if (active && (typeof document === "undefined" || !document.hidden)) {
          ensureLoop();
        }
      });
    } catch (e) {
      console.error("Failed to initialize immersive spectrum visualizer:", e);
    }
    if (active) {
      drawFrame();
    }
  });

  onDestroy(() => {
    stopLoop();
    syncSpectrumSubscription(false);
    resizeObserver?.disconnect();
    if (typeof document !== "undefined") {
      document.removeEventListener("visibilitychange", handleVisibilityChange);
    }
    if (unlisten) {
      unlisten();
    }
  });
</script>

<div
  data-testid="immersive-visualizer"
  class="relative z-10 flex flex-col w-full max-w-5xl h-full max-h-[600px] gap-4 md:gap-6 justify-center"
>
  {#if playerStore.currentSong}
    <!-- Compact Song Context Header: keeps track identity accessible and lets the user click cover art to flip to the artwork stage -->
    <div class="flex items-center gap-4 px-1">
      <button
        type="button"
        onclick={switchToCoverView}
        title={i18n.t("playerBar.immersiveTitle", {}, "Immersive Mode")}
        class="w-14 h-14 md:w-16 md:h-16 rounded-xl overflow-hidden border border-brand-border/50 bg-brand-sidebar flex-shrink-0 shadow-md hover:scale-105 active:scale-95 transition-transform duration-200 focus:outline-hidden focus-visible:ring-2 focus-visible:ring-brand-accent"
      >
        <CoverArt
          songId={playerStore.currentSong.id}
          artEmbedded={playerStore.currentSong.art_embedded}
          artAutomatic={playerStore.currentSong.art_automatic}
          artManual={playerStore.currentSong.art_manual}
          sizeClass="w-full h-full"
          contain={isLiveSong(playerStore.currentSong)}
        />
      </button>

      <div class="flex flex-col min-w-0 flex-1">
        <span
          class="label-small self-start px-2.5 py-0.5 bg-brand-accent/15 text-brand-accent-text border border-brand-border rounded-full select-none"
        >
          {i18n.t("playerBar.nowPlaying")}
        </span>
        <h1
          class="mt-1.5 text-2xl md:text-3xl font-heading font-bold text-brand-text-primary leading-tight tracking-tight truncate select-text"
        >
          {playerStore.currentSongDisplayTitle}
        </h1>
        {#if playerStore.currentSong.album?.trim()}
          <p
            class="mt-1 text-sm md:text-base text-brand-text-secondary font-semibold truncate select-text"
          >
            {playerStore.currentSong.album}
          </p>
        {/if}
        <p
          class="{playerStore.currentSong.album?.trim()
            ? 'mt-0.5'
            : 'mt-1'} text-xs md:text-sm text-brand-text-secondary/70 font-medium truncate select-text"
        >
          {playerStore.currentSongDisplaySubtitle}
        </p>
      </div>
    </div>
  {/if}

  <!-- Voxel LED Spectrum Matrix Stage -->
  <div
    class="flex-1 min-h-[220px] md:min-h-[320px] w-full rounded-2xl border border-brand-border/40 bg-brand-sidebar/65 p-4 md:p-6 flex items-center justify-center shadow-[0_25px_50px_-12px_rgba(0,0,0,0.45)]"
  >
    <canvas bind:this={canvas} data-testid="immersive-visualizer-canvas" class="w-full h-full block"
    ></canvas>
  </div>
</div>
