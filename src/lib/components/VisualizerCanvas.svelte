<script lang="ts">
  /**
   * Draws one visualizer preset on a canvas. Owns everything that's the same for every
   * preset and every place one is shown: turning on the data feeds the preset needs,
   * listening for their data, drawing on animation frames only while there's something
   * new to show, pausing while the window is hidden, redrawing on resize and re-theming.
   */
  import { onDestroy, onMount, untrack } from "svelte";
  import { playerStore } from "../stores/player.svelte";
  import { themeStore } from "../stores/theme.svelte";
  import { prefersReducedMotion } from "../utils/motion";
  import { acquireScope, listenScope, releaseScope, type ScopeFrame } from "../utils/scopeFeed";
  import { acquireSpectrum, listenSpectrum, releaseSpectrum } from "../utils/spectrumEnable";
  import { prepareCanvas2d } from "../visualizers/canvas";
  import type {
    VisualizerFeed,
    VisualizerInstance,
    VisualizerPreset,
    VisualizerSize,
  } from "../visualizers/types";

  interface Props {
    preset: VisualizerPreset;
    size: VisualizerSize;
    /** Show the preset's still preview instead of live data. */
    preview?: boolean;
    /** While false, feeds are released and nothing is drawn. */
    active?: boolean;
    class?: string;
    testId?: string;
  }

  let {
    preset,
    size,
    preview = false,
    active = true,
    class: className = "",
    testId,
  }: Props = $props();

  const FEEDS: Record<VisualizerFeed, { acquire(): void; release(): void }> = {
    spectrum: { acquire: acquireSpectrum, release: releaseSpectrum },
    scope: { acquire: acquireScope, release: releaseScope },
  };
  const NO_BANDS: readonly number[] = [];

  let canvas = $state<HTMLCanvasElement | null>(null);
  // Plain (non-reactive) on purpose: the draw loop reads these, and as $state the effects
  // below would re-run on every spectrum event. `instance` is created by the first run of
  // the $effect.pre below, before any other effect reads it.
  let instance!: VisualizerInstance;
  let bands: readonly number[] = NO_BANDS;
  let rafId = 0;
  let lastFrameTime = 0;
  let unlisteners: (() => void)[] = [];
  let destroyed = false;
  let resizeObserver: ResizeObserver | null = null;

  const isPlaying = $derived(playerStore.state === "playing");

  function isHidden(): boolean {
    return typeof document !== "undefined" && document.hidden;
  }

  /**
   * Hands the preset the latest data. `onset` and `scope` belong to one event, so only
   * that event passes them.
   */
  function pushCurrent(onset = 0, scope: ScopeFrame | null = null) {
    instance.push({ bands, playing: isPlaying, onset, scope });
  }

  function frame(now: number) {
    rafId = 0;
    if (!active || isHidden()) {
      lastFrameTime = 0;
      return;
    }
    const dtMs = lastFrameTime > 0 ? now - lastFrameTime : 16.67;
    lastFrameTime = now;
    const reducedMotion = prefersReducedMotion();
    const stillMoving = !preview && instance.step(dtMs, reducedMotion);

    const target = prepareCanvas2d(canvas);
    if (target) {
      instance.render(target.ctx, target.width, target.height, { reducedMotion, preview });
    }

    if (stillMoving) {
      rafId = requestAnimationFrame(frame);
    } else {
      lastFrameTime = 0;
    }
  }

  /** Draws on the next animation frame; repeated calls before it fires share one draw. */
  function requestFrame() {
    if (!active || isHidden() || rafId) return;
    rafId = requestAnimationFrame(frame);
  }

  function stopLoop() {
    if (rafId) cancelAnimationFrame(rafId);
    rafId = 0;
    lastFrameTime = 0;
  }

  // A new preset (or size) gets a fresh instance, primed with the current theme and data.
  $effect.pre(() => {
    const p = preset;
    const s = size;
    untrack(() => {
      instance = p.create(s);
      instance.setTheme(themeStore.resolvedColors);
      pushCurrent();
      lastFrameTime = 0;
      requestFrame();
    });
  });

  $effect(() => {
    instance.setTheme(themeStore.resolvedColors);
    untrack(requestFrame);
  });

  // Hold the feeds the preset needs only while active.
  $effect(() => {
    if (!active) return;
    const needs = preset.needs;
    for (const feed of needs) FEEDS[feed].acquire();
    return () => {
      for (const feed of needs) FEEDS[feed].release();
    };
  });

  $effect(() => {
    void preview;
    if (active) {
      untrack(requestFrame);
    } else {
      stopLoop();
    }
  });

  // Playback starting or stopping is data too: a paused preset settles to rest.
  $effect(() => {
    void isPlaying;
    untrack(() => {
      pushCurrent();
      requestFrame();
    });
  });

  function handleVisibilityChange() {
    if (isHidden()) stopLoop();
    else requestFrame();
  }

  onMount(async () => {
    document.addEventListener("visibilitychange", handleVisibilityChange);
    if (canvas && typeof ResizeObserver !== "undefined") {
      resizeObserver = new ResizeObserver(() => requestFrame());
      resizeObserver.observe(canvas);
    }
    // Listening costs nothing while a feed is off: events only flow for feeds held above.
    // A still preview doesn't change with the data, so only live views redraw.
    await Promise.all([
      keepListening(
        listenSpectrum((data) => {
          bands = data.bands;
          pushCurrent(data.onset);
          if (!preview) requestFrame();
        }),
        "spectrum",
      ),
      keepListening(
        listenScope((data) => {
          pushCurrent(0, data);
          if (!preview) requestFrame();
        }),
        "scope",
      ),
    ]);
  });

  async function keepListening(registering: Promise<() => void>, feed: string) {
    try {
      const stop = await registering;
      // Unmounted while the listener was being registered: drop it straight away.
      if (destroyed) stop();
      else unlisteners.push(stop);
    } catch (e) {
      console.error(`Failed to listen for ${feed} data:`, e);
    }
  }

  onDestroy(() => {
    destroyed = true;
    stopLoop();
    resizeObserver?.disconnect();
    if (typeof document !== "undefined") {
      document.removeEventListener("visibilitychange", handleVisibilityChange);
    }
    for (const stop of unlisteners) stop();
  });
</script>

<canvas
  bind:this={canvas}
  data-testid={testId}
  data-preset={preset.id}
  data-preview={preview ? "true" : "false"}
  class={className}
></canvas>
