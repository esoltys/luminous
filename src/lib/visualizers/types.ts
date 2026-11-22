import type { ThemeColors } from "../stores/theme.svelte";
import type { ScopeFrame } from "../utils/scopeFeed";

/**
 * Where a visualizer is drawn. `mini` is the playbar slot (about 96×28 CSS px, on the
 * playbar surface); `full` is the immersive stage. A preset decides how it scales down:
 * the two sizes can differ in data resolution and palette, not just in scale.
 */
export type VisualizerSize = "mini" | "full";

/**
 * A backend data feed a preset reads. The host turns on only the feeds the selected
 * preset needs, so an unused feed costs nothing.
 */
export type VisualizerFeed = "spectrum" | "scope";

/** The latest audio data, handed to a preset each time it changes. */
export interface VisualizerFrame {
  /** 32 log-spaced spectrum bands in [0, 1], normalized to the frame's own peak. */
  readonly bands: readonly number[];
  /** False while paused or stopped; a preset settles to rest on its own. */
  readonly playing: boolean;
  /** Strength in (0, 1] of a beat detected since the previous frame; 0 means none. */
  readonly onset: number;
  /**
   * New vectorscope data (`scope` feed) that arrived with this push; null when the push
   * carries none, e.g. a playback-state change or a spectrum event.
   */
  readonly scope: ScopeFrame | null;
}

/** The track a visualizer is showing, for presets that vary with it. */
export interface VisualizerTrack {
  /** Stable while one track plays and different for the next, e.g. to seed a per-track layout. */
  readonly key: string;
}

export interface VisualizerRenderOptions {
  readonly reducedMotion: boolean;
  /** Draw a still preview instead of live data, e.g. the playbar slot while immersive is open. */
  readonly preview: boolean;
}

/**
 * One running visualizer, created by its preset for a given size. It owns its own state,
 * smoothing and palette; the host only feeds it data and asks it to draw. `render` must
 * not allocate or parse colours: build anything theme-derived in `setTheme`.
 */
export interface VisualizerInstance {
  /** Called on creation and whenever the theme changes. */
  setTheme(colors: ThemeColors): void;
  /** Optional. Called on creation and whenever the track changes; null when nothing is loaded. */
  setTrack?(track: VisualizerTrack | null): void;
  /** Called with each new frame of audio data, and when playback starts or stops. */
  push(frame: VisualizerFrame): void;
  /**
   * Advances animation by `dtMs`. Returns true while there is motion left to show without
   * new data, so the host keeps drawing; false once another frame would look the same.
   */
  step(dtMs: number, reducedMotion: boolean): boolean;
  render(
    ctx: CanvasRenderingContext2D,
    width: number,
    height: number,
    options: VisualizerRenderOptions,
  ): void;
}

/** A selectable visualizer, registered in `registry.ts`. */
export interface VisualizerPreset {
  /** Stable id, saved as the user's choice. */
  readonly id: string;
  /** i18n key of the name shown in the selector. */
  readonly labelKey: string;
  readonly needs: readonly VisualizerFeed[];
  create(size: VisualizerSize): VisualizerInstance;
}
