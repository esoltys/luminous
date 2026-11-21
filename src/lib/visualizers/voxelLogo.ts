import type { ThemeColors } from "../stores/theme.svelte";
import type {
  VisualizerFrame,
  VisualizerInstance,
  VisualizerPreset,
  VisualizerRenderOptions,
  VisualizerSize,
} from "./types";
import {
  LogoGrid,
  LogoLevels,
  VoxelLogoRenderer,
  buildVoxelLogoPalette,
  type VoxelLogoPalette,
} from "./voxelLogoEngine";

/** The full view's frame, in mark units: the mark plus room for the glow halo (radius up to 130). */
const FULL_FRAME_MIN = -40;
const FULL_FRAME_MAX = 240;
const FULL_GRID = 56;
/** The mini view drops the glow, so its frame is the mark with a little margin. */
const MINI_FRAME_MIN = -4;
const MINI_FRAME_MAX = 204;
// 9 cells is the coarsest grid that still has a cell for each of the disc, rim and ring.
const MINI_GRIDS = [13, 11, 9] as const;

/** Still-preview levels: a calm mark with a little burst halo, so it reads as the reactive logo. */
const PREVIEW_LEVELS = LogoLevels.fixed(0.55, 0.45, 0.4);

/** The immersive stage: a 56-cell mark with a glow halo, set in a field of unlit housing cells. */
class VoxelLogoFullInstance implements VisualizerInstance {
  private readonly levels = new LogoLevels();
  private readonly renderer = new VoxelLogoRenderer({
    grids: [new LogoGrid(FULL_GRID, FULL_FRAME_MIN, FULL_FRAME_MAX)],
    housing: true,
    glow: true,
  });
  private palette!: VoxelLogoPalette;

  setTheme(colors: ThemeColors): void {
    this.palette = buildVoxelLogoPalette(colors, "bg-main");
  }

  push(frame: VisualizerFrame): void {
    this.levels.push(frame);
  }

  step(dtMs: number, reducedMotion: boolean): boolean {
    return this.levels.step(dtMs, reducedMotion);
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
 * The playbar slot and menu thumbnail: the coarsest grid that keeps cells crisp at the slot's
 * height, with no glow or housing. It has no smoothing of its own and draws once per frame of
 * data, so it costs no more than the voxel spectrum's mini view.
 */
class VoxelLogoMiniInstance implements VisualizerInstance {
  private readonly levels = new LogoLevels();
  private readonly renderer = new VoxelLogoRenderer({
    grids: MINI_GRIDS.map((size) => new LogoGrid(size, MINI_FRAME_MIN, MINI_FRAME_MAX)),
    housing: false,
    glow: false,
  });
  private palette!: VoxelLogoPalette;

  setTheme(colors: ThemeColors): void {
    // The slot sits on the playbar surface, so contrast is checked against that.
    this.palette = buildVoxelLogoPalette(colors, "bg-playerbar");
  }

  push(frame: VisualizerFrame): void {
    this.levels.push(frame);
    this.levels.settle();
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
    const levels = options.preview ? PREVIEW_LEVELS : this.levels;
    this.renderer.render(ctx, width, height, levels, this.palette, options.reducedMotion);
  }
}

/** The Luminous mark as an LED grid: mids light the rim, treble the ring, bass flares the burst. */
export const VOXEL_LOGO_PRESET: VisualizerPreset = {
  id: "voxel-logo",
  labelKey: "visualizers.voxelLogo",
  needs: ["spectrum"],
  create(size: VisualizerSize): VisualizerInstance {
    return size === "mini" ? new VoxelLogoMiniInstance() : new VoxelLogoFullInstance();
  },
};
