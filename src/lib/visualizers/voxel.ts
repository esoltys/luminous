import type { ThemeColors } from "../stores/theme.svelte";
import { clampForContrast, hexToRgb } from "../utils/colorUtils";
import type {
  VisualizerFrame,
  VisualizerInstance,
  VisualizerPreset,
  VisualizerRenderOptions,
  VisualizerSize,
  VisualizerTrack,
} from "./types";
import {
  VoxelSpectrumEngine,
  buildVoxelThemePalette,
  renderStaticVoxelPreview,
  renderVoxelMatrix,
  type VoxelThemePalette,
} from "./voxelEngine";
import { VoxelLayerCache } from "./voxelGrid";

const FULL_COLUMNS = 64;
const FULL_ROWS = 28;
/** Rows in the still preview, sized for the playbar slot and menu thumbnails. */
const PREVIEW_ROWS = 6;
const MINI_BAR_GAP = 2.5;

/** The immersive LED matrix: 64 smoothed columns with peak-hold caps and a reflection floor. */
class VoxelFullInstance implements VisualizerInstance {
  private readonly engine = new VoxelSpectrumEngine(FULL_COLUMNS, FULL_ROWS);
  private readonly housingCache = new VoxelLayerCache();
  private colors!: ThemeColors;
  private trackKey: string | null = null;
  private trackAccent: string | null = null;
  private palette!: VoxelThemePalette;
  private previewPalette!: VoxelThemePalette;

  setTheme(colors: ThemeColors): void {
    this.colors = colors;
    this.palette = buildVoxelThemePalette(colors, FULL_ROWS, this.trackAccent);
    this.previewPalette = buildVoxelThemePalette(colors, PREVIEW_ROWS, this.trackAccent);
  }

  setTrack(track: VisualizerTrack | null): void {
    const nextKey = track?.key ?? null;
    if (nextKey !== null && nextKey !== this.trackKey) {
      this.engine.triggerSweep();
    }
    this.trackKey = nextKey;
    const nextAccent = track?.accent ?? null;
    if (nextAccent !== this.trackAccent) {
      this.trackAccent = nextAccent;
      if (this.colors) {
        this.palette = buildVoxelThemePalette(this.colors, FULL_ROWS, nextAccent);
        this.previewPalette = buildVoxelThemePalette(this.colors, PREVIEW_ROWS, nextAccent);
      }
    }
  }

  push(frame: VisualizerFrame): void {
    // Paused or stopped: zero the targets so bars and peak caps fall to rest.
    if (frame.playing) {
      this.engine.pushSpectrum(frame.bands);
      if (frame.onset > 0) this.engine.pushOnset(frame.onset);
    } else {
      this.engine.clearTargets();
    }
  }

  step(dtMs: number, reducedMotion: boolean): boolean {
    return this.engine.step(dtMs, reducedMotion);
  }

  render(
    ctx: CanvasRenderingContext2D,
    width: number,
    height: number,
    options: VisualizerRenderOptions,
  ): void {
    if (options.preview) renderStaticVoxelPreview(ctx, width, height, this.previewPalette);
    else
      renderVoxelMatrix(ctx, width, height, this.engine, this.palette, {
        reducedMotion: options.reducedMotion,
        housingCache: this.housingCache,
      });
  }
}

/**
 * The playbar slot: the raw 32 bands as accent bars, drawn once per frame of data with no
 * smoothing of its own, so it costs no more than one draw per spectrum event.
 */
class VoxelMiniInstance implements VisualizerInstance {
  private bands: readonly number[] = [];
  private accentRgb = { r: 0, g: 0, b: 0 };
  private hoverColor = "#a78bfa";
  private previewPalette!: VoxelThemePalette;
  // The bar gradient only depends on the theme and canvas height; rebuilt when either changes.
  private gradient: CanvasGradient | null = null;
  private gradientHeight = 0;

  setTheme(colors: ThemeColors): void {
    // The slot sits on the playbar surface, so contrast is checked against that, not bg-main.
    const surfaceBg = colors["bg-playerbar"] || colors["bg-main"] || "#191918";
    this.accentRgb = hexToRgb(
      clampForContrast(colors["color-accent"] || "#8b5cf6", surfaceBg, 3.0),
    );
    this.hoverColor = clampForContrast(colors["color-accent-hover"] || "#a78bfa", surfaceBg, 3.0);
    this.previewPalette = buildVoxelThemePalette(colors, PREVIEW_ROWS);
    this.gradient = null;
  }

  push(frame: VisualizerFrame): void {
    this.bands = frame.bands;
  }

  step(): boolean {
    return false;
  }

  render(
    ctx: CanvasRenderingContext2D,
    width: number,
    height: number,
    options: VisualizerRenderOptions,
  ): void {
    if (width <= 0 || height <= 0) return;
    if (options.preview) {
      renderStaticVoxelPreview(ctx, width, height, this.previewPalette);
      return;
    }

    ctx.clearRect(0, 0, width, height);
    if (!this.gradient || this.gradientHeight !== height) {
      const { r, g, b } = this.accentRgb;
      const grad = ctx.createLinearGradient(0, height, 0, 0);
      grad.addColorStop(0, `rgba(${r}, ${g}, ${b}, 0.2)`);
      grad.addColorStop(0.5, `rgba(${r}, ${g}, ${b}, 0.8)`);
      grad.addColorStop(1, this.hoverColor);
      this.gradient = grad;
      this.gradientHeight = height;
    }
    ctx.fillStyle = this.gradient;

    const numBars = 32;
    const barWidth = (width - (numBars - 1) * MINI_BAR_GAP) / numBars;
    for (let i = 0; i < numBars; i++) {
      // Bands already arrive normalized to the frame's own peak (see VisualizerAnalyzer::bands),
      // so no extra gain: a fixed multiplier would clip most bars to the same height.
      const val = this.bands[i] || 0;
      const barHeight = Math.min(height, Math.max(1.5, val * height));
      ctx.beginPath();
      ctx.roundRect(
        i * (barWidth + MINI_BAR_GAP),
        height - barHeight,
        barWidth,
        barHeight,
        [1.5, 1.5, 0, 0],
      );
      ctx.fill();
    }
  }
}

export const VOXEL_PRESET: VisualizerPreset = {
  id: "voxel",
  labelKey: "visualizers.voxel",
  needs: ["spectrum"],
  create(size: VisualizerSize): VisualizerInstance {
    return size === "mini" ? new VoxelMiniInstance() : new VoxelFullInstance();
  },
};
