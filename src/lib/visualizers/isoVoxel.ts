import { hexToRgb, rgbToHex } from "../utils/colorUtils";
import type { Ctx2D } from "./voxelGrid";

// Isometric voxels: unit cubes in a scene's (x, y, z) space drawn in a 2:1 isometric projection,
// x running down-right, y down-left and z up, so the viewer looks from the +x, +y, +z corner.
// Painting cubes in increasing x, then y, then z draws each over everything it can hide.

/** The three visible faces of a cube: lit top, the left face (+y side) and the darker right (+x). */
export interface IsoFaces {
  readonly top: string;
  readonly left: string;
  readonly right: string;
}

/** Which faces `IsoView.cube` draws, as a mask. */
export const FACE_TOP = 1;
export const FACE_LEFT = 2;
export const FACE_RIGHT = 4;
export const FACE_ALL = FACE_TOP | FACE_LEFT | FACE_RIGHT;

/** Share of a cube's width left as a gap on each side, so cubes read as separate voxels. */
const GAP = 0.06;

function shade(hex: string, toward: number, t: number): string {
  const { r, g, b } = hexToRgb(hex);
  return rgbToHex(
    Math.round(r + (toward - r) * t),
    Math.round(g + (toward - g) * t),
    Math.round(b + (toward - b) * t),
  );
}

/** A cube in `hex`, lit from above: the top in `hex` itself and the sides a step and two darker. */
export function isoFaces(hex: string): IsoFaces {
  return { top: hex, left: shade(hex, 0, 0.2), right: shade(hex, 0, 0.36) };
}

/**
 * A scene box projected onto a canvas: `fit` scales and centres it, then `cube` and `tile` draw
 * in scene units. `unit` is half a cube's projected width in CSS px; a cube is `2 * unit` wide
 * and `2 * unit` tall on screen.
 */
export class IsoView {
  unit = 1;
  originX = 0;
  originY = 0;

  /**
   * Scales the box x in [x0, x1], y in [y0, y1], z in [z0, z1] to the largest size that fits a
   * `width`×`height` canvas with a margin, centred. Returns whether the projection changed.
   */
  fit(
    width: number,
    height: number,
    x0: number,
    x1: number,
    y0: number,
    y1: number,
    z0: number,
    z1: number,
  ): boolean {
    // Projected extents, in units: x - y across, (x + y) / 2 - z down.
    const left = x0 - y1;
    const right = x1 - y0;
    const top = (x0 + y0) / 2 - z1;
    const bottom = (x1 + y1) / 2 - z0;
    const unit = Math.max(
      0.5,
      Math.min((width * 0.94) / (right - left), (height * 0.92) / (bottom - top)),
    );
    const originX = Math.round((width - (right - left) * unit) / 2 - left * unit);
    const originY = Math.round((height - (bottom - top) * unit) / 2 - top * unit);
    const changed = unit !== this.unit || originX !== this.originX || originY !== this.originY;
    this.unit = unit;
    this.originX = originX;
    this.originY = originY;
    return changed;
  }

  /** Screen x of scene point (x, y, ·). */
  screenX(x: number, y: number): number {
    return this.originX + (x - y) * this.unit;
  }

  /** Screen y of scene point (x, y, z). */
  screenY(x: number, y: number, z: number): number {
    return this.originY + ((x + y) / 2 - z) * this.unit;
  }

  /**
   * Draws the cube of edge `size` whose low corner is (x, y, z), with the faces in `mask`. The
   * horizontal edges are inset by a small gap so neighbouring cubes stay distinct.
   */
  cube(
    ctx: Ctx2D,
    x: number,
    y: number,
    z: number,
    size: number,
    faces: IsoFaces,
    mask = FACE_ALL,
  ): void {
    const g = size * GAP;
    const a = x + g;
    const b = y + g;
    const e = size - 2 * g;
    const zt = z + size;
    if (mask & FACE_TOP)
      this.quad(ctx, faces.top, a, b, zt, a + e, b, zt, a + e, b + e, zt, a, b + e, zt);
    if (mask & FACE_LEFT)
      this.quad(ctx, faces.left, a, b + e, z, a + e, b + e, z, a + e, b + e, zt, a, b + e, zt);
    if (mask & FACE_RIGHT)
      this.quad(ctx, faces.right, a + e, b, z, a + e, b + e, z, a + e, b + e, zt, a + e, b, zt);
  }

  /** Draws a flat square of edge `size` lying at height z, low corner at (x, y), in `fill`. */
  tile(ctx: Ctx2D, x: number, y: number, z: number, size: number, fill: string): void {
    const g = size * GAP;
    const a = x + g;
    const b = y + g;
    const e = size - 2 * g;
    this.quad(ctx, fill, a, b, z, a + e, b, z, a + e, b + e, z, a, b + e, z);
  }

  private quad(
    ctx: Ctx2D,
    fill: string,
    x1: number,
    y1: number,
    z1: number,
    x2: number,
    y2: number,
    z2: number,
    x3: number,
    y3: number,
    z3: number,
    x4: number,
    y4: number,
    z4: number,
  ): void {
    ctx.fillStyle = fill;
    ctx.beginPath();
    ctx.moveTo(this.screenX(x1, y1), this.screenY(x1, y1, z1));
    ctx.lineTo(this.screenX(x2, y2), this.screenY(x2, y2, z2));
    ctx.lineTo(this.screenX(x3, y3), this.screenY(x3, y3, z3));
    ctx.lineTo(this.screenX(x4, y4), this.screenY(x4, y4, z4));
    ctx.closePath();
    ctx.fill();
  }
}
