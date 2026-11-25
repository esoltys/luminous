import type { ThemeColors } from "../stores/theme.svelte";
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
  renderCompactVoxelGrid,
  renderStaticVoxelPreview,
  renderVoxelMatrix,
  type VoxelThemePalette,
} from "./voxelEngine";
import { VoxelLayerCache } from "./voxelGrid";

const FULL_COLUMNS = 64;
const FULL_ROWS = 28;
/** Dimensions of the compact voxel grid shared by the playbar slot and still preview. */
const MINI_COLUMNS = 16;
const PREVIEW_ROWS = 6;

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
 * The playbar slot: 32 bands downsampled into 16 discrete 6-row voxel columns,
 * drawn once per frame of data with no smoothing of its own, so it matches the
 * immersion-mode preview silhouette at zero extra animation cost.
 */
class VoxelMiniInstance implements VisualizerInstance {
  private readonly levels = new Float32Array(MINI_COLUMNS);
  private colors!: ThemeColors;
  private trackAccent: string | null = null;
  private palette!: VoxelThemePalette;

  setTheme(colors: ThemeColors): void {
    this.colors = colors;
    this.palette = buildVoxelThemePalette(colors, PREVIEW_ROWS, this.trackAccent, "bg-playerbar");
  }

  setTrack(track: VisualizerTrack | null): void {
    const nextAccent = track?.accent ?? null;
    if (nextAccent !== this.trackAccent) {
      this.trackAccent = nextAccent;
      if (this.colors) {
        this.palette = buildVoxelThemePalette(
          this.colors,
          PREVIEW_ROWS,
          nextAccent,
          "bg-playerbar",
        );
      }
    }
  }

  push(frame: VisualizerFrame): void {
    if (!frame.playing || frame.bands.length === 0) {
      this.levels.fill(0);
      return;
    }
    const n = frame.bands.length;
    for (let c = 0; c < MINI_COLUMNS; c++) {
      const start = Math.floor((c * n) / MINI_COLUMNS);
      const end = Math.max(start + 1, Math.floor(((c + 1) * n) / MINI_COLUMNS));
      let peak = 0;
      for (let i = start; i < end; i++) {
        const v = frame.bands[i] || 0;
        if (v > peak) peak = v;
      }
      this.levels[c] = peak;
    }
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
    if (options.preview) renderStaticVoxelPreview(ctx, width, height, this.palette);
    else renderCompactVoxelGrid(ctx, width, height, this.palette, this.levels);
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
