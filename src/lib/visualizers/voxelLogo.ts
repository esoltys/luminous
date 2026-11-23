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
  LogoGrid,
  LogoLevels,
  VoxelLogoRenderer,
  buildVoxelLogoPalette,
  type VoxelLogoPalette,
} from "./voxelLogoEngine";

interface VoxelLogoSizeConfig {
  /** Grids from finest to coarsest, in mark units; the renderer picks the finest that fits. */
  readonly grids: () => LogoGrid[];
  /** Unlit housing cells around the mark, as the voxel spectrum's matrix has. */
  readonly housing: boolean;
  /** The mid-driven glow halo around the ring. */
  readonly glow: boolean;
  /**
   * Beat ripples travelling out across the stage. No effect without `smoothed`: ripples only
   * move in `step`, and settling each frame drops them.
   */
  readonly ripples: boolean;
  /** The surface the canvas sits on, which colours are contrast-checked against. */
  readonly surface: "bg-main" | "bg-playerbar";
  /**
   * Ease levels between frames of data. Without it the levels jump to each frame and the view
   * draws once per frame of data, as the voxel spectrum's mini view does.
   */
  readonly smoothed: boolean;
}

const SIZES: Record<VisualizerSize, VoxelLogoSizeConfig> = {
  // The immersive stage: the mark plus room for the glow halo (radius up to 130) in a 56-cell
  // field, set in unlit housing.
  full: {
    grids: () => [new LogoGrid(56, -40, 240)],
    housing: true,
    glow: true,
    ripples: true,
    surface: "bg-main",
    smoothed: true,
  },
  // The playbar slot and menu thumbnail: just the mark with a little margin, on the finest of
  // three grids whose cells stay crisp at the slot's height. 9 cells is the coarsest grid that
  // still gives the disc, rim and ring a cell each.
  mini: {
    grids: () => [13, 11, 9].map((size) => new LogoGrid(size, -4, 204)),
    housing: false,
    glow: false,
    ripples: false,
    surface: "bg-playerbar",
    smoothed: false,
  },
};

/** Still-preview levels: a calm mark with a little burst halo, so it reads as the reactive logo. */
const PREVIEW_LEVELS = LogoLevels.fixed(0.55, 0.45, 0.4);

class VoxelLogoInstance implements VisualizerInstance {
  private readonly levels = new LogoLevels();
  private readonly renderer: VoxelLogoRenderer;
  private colors!: ThemeColors;
  private trackAccent: string | null = null;
  private palette!: VoxelLogoPalette;

  constructor(private readonly config: VoxelLogoSizeConfig) {
    this.renderer = new VoxelLogoRenderer({
      grids: config.grids(),
      housing: config.housing,
      glow: config.glow,
      ripples: config.ripples,
    });
  }

  setTheme(colors: ThemeColors): void {
    this.colors = colors;
    this.palette = buildVoxelLogoPalette(colors, this.config.surface, this.trackAccent);
  }

  setTrack(track: VisualizerTrack | null): void {
    const nextAccent = track?.accent ?? null;
    if (nextAccent !== this.trackAccent) {
      this.trackAccent = nextAccent;
      if (this.colors) {
        this.palette = buildVoxelLogoPalette(this.colors, this.config.surface, nextAccent);
      }
    }
  }

  push(frame: VisualizerFrame): void {
    this.levels.push(frame);
    if (!this.config.smoothed) this.levels.settle();
  }

  step(dtMs: number, reducedMotion: boolean): boolean {
    return this.config.smoothed && this.levels.step(dtMs, reducedMotion);
  }

  render(
    ctx: CanvasRenderingContext2D,
    width: number,
    height: number,
    options: VisualizerRenderOptions,
  ): void {
    const levels = options.preview ? PREVIEW_LEVELS : this.levels;
    this.renderer.render(ctx, width, height, levels, this.palette, options.reducedMotion);
  }
}

/**
 * The Luminous mark as an LED grid: mids light the rim, treble the ring, bass flares the burst,
 * and in the full view each beat sends a ripple out from the ring.
 */
export const VOXEL_LOGO_PRESET: VisualizerPreset = {
  id: "voxel-logo",
  labelKey: "visualizers.voxelLogo",
  needs: ["spectrum"],
  create(size: VisualizerSize): VisualizerInstance {
    return new VoxelLogoInstance(SIZES[size]);
  },
};
