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
    prepareCanvas2d,
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
  let detailsHeight = $state(0);
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
    const target = prepareCanvas2d(canvas);
    if (!target) return;
    const { ctx, width, height } = target;
    preset.render(ctx, width, height, engine, palette, {
      reducedMotion: prefersReducedMotion(),
    });
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
    <!-- Compact Song Context Header: Now playing pill sits above the heading, while the square
         album artwork spans from the cap-top of the heading to the descender-bottom of the artist line. -->
    <div class="flex items-end gap-4 px-1">
      <button
        type="button"
        onclick={() => windowLayoutStore.toggleImmersiveMode("cover")}
        title={i18n.t("playerBar.immersiveTitle", {}, "Immersive Mode")}
        style={detailsHeight > 0
          ? `width: ${detailsHeight}px; height: ${detailsHeight}px;`
          : undefined}
        class="w-16 h-16 md:w-[4.5rem] md:h-[4.5rem] rounded-xl overflow-hidden border border-brand-border/50 bg-brand-sidebar flex-shrink-0 shadow-md hover:scale-105 active:scale-95 transition-transform duration-200 focus:outline-hidden focus-visible:ring-2 focus-visible:ring-brand-accent"
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
          class="label-small self-start mb-2 px-2.5 py-0.5 bg-brand-accent/15 text-brand-accent-text border border-brand-border rounded-full select-none"
        >
          {i18n.t("playerBar.nowPlaying")}
        </span>
        <div
          bind:clientHeight={detailsHeight}
          data-testid="immersive-visualizer-song-details"
          class="flex flex-col min-w-0"
        >
          <h1
            class="text-2xl md:text-3xl font-heading font-bold text-brand-text-primary leading-none -mt-[0.12em] tracking-tight truncate select-text"
          >
            {playerStore.currentSongDisplayTitle}
          </h1>
          {#if playerStore.currentSong.album?.trim()}
            <p
              class="mt-1.5 text-sm md:text-base text-brand-text-secondary font-semibold leading-tight truncate select-text"
            >
              {playerStore.currentSong.album}
            </p>
          {/if}
          <p
            class="{playerStore.currentSong.album?.trim()
              ? 'mt-1'
              : 'mt-1.5'} text-xs md:text-sm text-brand-text-secondary/70 font-medium leading-none truncate select-text"
          >
            {playerStore.currentSongDisplaySubtitle}
          </p>
        </div>
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
