import type { ThemeColors } from "../stores/theme.svelte";
import { clampForContrast, hexToRgb, isLightColor } from "../utils/colorUtils";

// Shared building blocks for the LED-grid ("voxel") presets: square cells fitted to a canvas, a
// painter that draws a fill per cell with as few `fillStyle` changes as possible, a cached layer
// for cells that hold still, and the theme-derived fills every voxel preset shares, so their
// housings and brightness steps match.

export type Ctx2D = CanvasRenderingContext2D | OffscreenCanvasRenderingContext2D;

/**
 * Square cells laid out on a canvas, in CSS px: `cols`×`rows` cells at `pitch` px apart, each
 * `cell` px square (the rest of the pitch is the gap between LEDs), with the grid's top-left
 * cell at (`originX`, `originY`). Columns and rows outside [0, cols) / [0, rows) continue at the
 * same pitch, e.g. for housing around the grid.
 */
export class VoxelGrid {
  cols = 1;
  rows = 1;
  pitch = 1;
  cell = 1;
  originX = 0;
  originY = 0;

  /**
   * Fits `cols`×`rows` cells into a `width`×`height` canvas at the largest whole-pixel pitch that
   * fits, centred. The gap is about 18% of the pitch from 5 px up, 1 px below that, and none at a
   * 1 px pitch. Returns whether the layout changed.
   */
  fit(width: number, height: number, cols: number, rows: number): boolean {
    const c = Math.max(1, Math.floor(cols));
    const r = Math.max(1, Math.floor(rows));
    const pitch = Math.max(1, Math.floor(Math.min(width / c, height / r)));
    const gap = pitch >= 5 ? Math.max(1, Math.round(pitch * 0.18)) : pitch >= 2 ? 1 : 0;
    const originX = Math.floor((width - c * pitch) / 2);
    const originY = Math.floor((height - r * pitch) / 2);
    const changed =
      c !== this.cols ||
      r !== this.rows ||
      pitch !== this.pitch ||
      originX !== this.originX ||
      originY !== this.originY;
    this.cols = c;
    this.rows = r;
    this.pitch = pitch;
    this.cell = pitch - gap;
    this.originX = originX;
    this.originY = originY;
    return changed;
  }

  contains(col: number, row: number): boolean {
    return col >= 0 && row >= 0 && col < this.cols && row < this.rows;
  }

  fillCell(ctx: Ctx2D, col: number, row: number): void {
    ctx.fillRect(
      this.originX + col * this.pitch,
      this.originY + row * this.pitch,
      this.cell,
      this.cell,
    );
  }

  /**
   * The cells covering a whole `width`×`height` canvas, grid and beyond, as half-open ranges
   * written to `out` as [firstCol, firstRow, endCol, endRow]; the firsts may be negative.
   */
  span(width: number, height: number, out: Int32Array): void {
    out[0] = -Math.ceil(this.originX / this.pitch);
    out[1] = -Math.ceil(this.originY / this.pitch);
    out[2] = Math.ceil((width - this.originX) / this.pitch);
    out[3] = Math.ceil((height - this.originY) / this.pitch);
  }
}

/**
 * Paints a grid from a fill index per cell (row-major, 0 = draw nothing), setting each fill once
 * per frame rather than once per cell: a counting sort into buffers that grow only when a larger
 * grid or palette arrives, so painting the same grid again allocates nothing.
 */
export class VoxelCellPainter {
  private order = new Int32Array(0);
  private starts = new Int32Array(0);
  private next = new Int32Array(0);

  paint(ctx: Ctx2D, grid: VoxelGrid, styles: Uint8Array, fills: readonly string[]): void {
    const count = grid.cols * grid.rows;
    const nFills = fills.length;
    if (this.order.length < count) this.order = new Int32Array(count);
    if (this.starts.length < nFills + 1) {
      this.starts = new Int32Array(nFills + 1);
      this.next = new Int32Array(nFills);
    }
    const starts = this.starts;
    starts.fill(0, 0, nFills + 1);
    for (let i = 0; i < count; i++) starts[styles[i] + 1]++;
    for (let s = 0; s < nFills; s++) starts[s + 1] += starts[s];
    const next = this.next;
    for (let s = 0; s < nFills; s++) next[s] = starts[s];
    for (let i = 0; i < count; i++) this.order[next[styles[i]]++] = i;

    const cols = grid.cols;
    for (let s = 1; s < nFills; s++) {
      const from = starts[s];
      const to = starts[s + 1];
      if (from === to) continue;
      ctx.fillStyle = fills[s];
      for (let j = from; j < to; j++) {
        const i = this.order[j];
        const row = (i / cols) | 0;
        grid.fillCell(ctx, i - row * cols, row);
      }
    }
  }
}

/**
 * Cells that hold still across frames, painted once onto an offscreen surface at the canvas's
 * backing resolution and blitted each frame with one `drawImage`. It repaints when the canvas
 * size changes or the caller says its content is stale. Where no offscreen surface is available
 * (or `ctx` can't blit), it paints straight onto `ctx` instead, so callers needn't care.
 */
export class VoxelLayerCache {
  private surface: HTMLCanvasElement | OffscreenCanvas | null = null;
  private surfaceCtx: Ctx2D | null = null;
  private width = 0;
  private height = 0;
  private backingW = 0;
  private backingH = 0;
  private painted = false;

  /**
   * Draws the layer over the `width`×`height` canvas (CSS px), repainting it with `paint` first
   * if `stale` or the canvas changed size. `paint` draws in CSS px onto a cleared surface.
   */
  blit(
    ctx: CanvasRenderingContext2D,
    width: number,
    height: number,
    stale: boolean,
    paint: (c: Ctx2D) => void,
  ): void {
    if (typeof ctx.drawImage !== "function") {
      paint(ctx);
      return;
    }
    const dpr = ctx.canvas?.width ? ctx.canvas.width / width : 1;
    const backingW = Math.max(1, Math.round(width * dpr));
    const backingH = Math.max(1, Math.round(height * dpr));
    if (
      stale ||
      !this.painted ||
      width !== this.width ||
      height !== this.height ||
      backingW !== this.backingW ||
      backingH !== this.backingH
    ) {
      if (!this.ensureSurface(backingW, backingH)) {
        paint(ctx);
        return;
      }
      const c = this.surfaceCtx!;
      c.setTransform(1, 0, 0, 1, 0, 0);
      c.clearRect(0, 0, backingW, backingH);
      c.setTransform(dpr, 0, 0, dpr, 0, 0);
      paint(c);
      this.width = width;
      this.height = height;
      this.backingW = backingW;
      this.backingH = backingH;
      this.painted = true;
    }
    ctx.drawImage(this.surface!, 0, 0, width, height);
  }

  private ensureSurface(w: number, h: number): boolean {
    if (this.surface) {
      if (this.surface.width !== w || this.surface.height !== h) {
        this.surface.width = w;
        this.surface.height = h;
      }
      return true;
    }
    if (typeof document !== "undefined") {
      const canvas = document.createElement("canvas");
      canvas.width = w;
      canvas.height = h;
      const c = canvas.getContext("2d");
      if (c) {
        this.surface = canvas;
        this.surfaceCtx = c;
        return true;
      }
    }
    if (typeof OffscreenCanvas !== "undefined") {
      const canvas = new OffscreenCanvas(w, h);
      const c = canvas.getContext("2d");
      if (c) {
        this.surface = canvas;
        this.surfaceCtx = c;
        return true;
      }
    }
    return false;
  }
}

/** `hex` at opacity `alpha` (clamped to 1), as a fill. */
export function voxelRgba(hex: string, alpha: number): string {
  const { r, g, b } = hexToRgb(hex);
  return `rgba(${r}, ${g}, ${b}, ${Number(Math.min(1, alpha).toFixed(3))})`;
}

/** Opacity of an unlit cell over `bg-main`, by theme polarity. */
function unlitAlpha(colors: ThemeColors): number {
  return isLightColor(colors["bg-main"] || "#191918") ? 0.13 : 0.1;
}

/** The colour unlit cells are drawn in: the theme's border, or failing that its accent. */
function unlitBase(colors: ThemeColors): string {
  const bgMain = colors["bg-main"] || "#191918";
  return (
    colors["color-border"] || clampForContrast(colors["color-accent"] || "#4f5bd5", bgMain, 3.0)
  );
}

/**
 * The fill of an unlit LED cell, shared by the voxel presets so their matrix housings match:
 * the theme's border colour, faint against `bg-main`. `strength` scales its opacity, e.g. for
 * cells a shade deeper than the housing.
 */
export function voxelUnlitCellColor(colors: ThemeColors, strength = 1): string {
  return voxelRgba(unlitBase(colors), unlitAlpha(colors) * strength);
}

/**
 * `steps` fills of `hex` from faint to bright, for brightness levels drawn as discrete steps: step
 * k has opacity `floor + (ceiling - floor) * k / (steps - 1)`.
 */
export function voxelAlphaLadder(hex: string, steps: number, floor = 0.12, ceiling = 1): string[] {
  const ladder: string[] = new Array(steps);
  for (let k = 0; k < steps; k++) {
    ladder[k] = voxelRgba(
      hex,
      steps <= 1 ? ceiling : floor + ((ceiling - floor) * k) / (steps - 1),
    );
  }
  return ladder;
}
