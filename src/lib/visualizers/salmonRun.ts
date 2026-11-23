import type { ThemeColors } from "../stores/theme.svelte";
import { clampForContrast, hexToRgb, isLightColor, rgbToHex } from "../utils/colorUtils";
import { FishLadder } from "./fishLadderEngine";
import { FOAM_STEPS, FishLadderRenderer, LIGHT_STEPS } from "./fishLadderRenderer";
import { isoFaces, type IsoFaces } from "./isoVoxel";
import { LoneSalmon, type LoneSalmonConfig } from "./salmonRunEngine";
import { VoxelGrid, voxelAlphaLadder } from "./voxelGrid";
import type {
  VisualizerFrame,
  VisualizerInstance,
  VisualizerPreset,
  VisualizerSize,
  VisualizerTrack,
} from "./types";

// The salmon run has two scenes. The immersive stage is a fish ladder of stepped pools drawn as
// isometric voxels (`fishLadderEngine.ts`, `fishLadderRenderer.ts`), so it reads at a glance that
// the salmon must leap from pool to pool to climb the river. The playbar slot, too small for
// isometric cubes to read, is a lone salmon on a flat LED grid, drawn here.

/**
 * Every fill the salmon run is drawn with, from the theme alone so it sits with the other voxel
 * presets: stone and banks from primary text faded into the surface, water from the accent, foam,
 * glints and ripples from the accent hover, and the salmon and leaf contrast-checked against the
 * surface the canvas sits on. Cube fills are opaque (cubes overlap); flat tiles may be translucent.
 */
export interface SalmonRunPalette {
  readonly stone: IsoFaces;
  /** Weir caps and boulders, a shade lighter than stone. */
  readonly cap: IsoFaces;
  /** The top of the far bank. */
  readonly bank: IsoFaces;
  /** Water below the surface, and the surface. */
  readonly deep: IsoFaces;
  readonly water: IsoFaces;
  /** The reduced-motion light shift washed over the pools, faint to bright, `LIGHT_STEPS` long. */
  readonly lightWash: readonly string[];
  /** The surface in a weir's notch, where it spills over. */
  readonly spill: IsoFaces;
  /** Falling water from dim to bright, `FOAM_STEPS` long. */
  readonly foam: readonly IsoFaces[];
  /** Ripples and churn, and current glints, from faint to bright, `FOAM_STEPS` long. */
  readonly ripple: readonly string[];
  readonly glint: readonly string[];
  /** The salmon, as flat cells (playbar) and as cubes (stage). */
  readonly fishBody: string;
  readonly fishHead: string;
  readonly fishBodyFaces: IsoFaces;
  readonly fishHeadFaces: IsoFaces;
  readonly leaf: string;
  /** The leaf's midrib and stem, a step down from `leaf`. */
  readonly leafVein: string;
}

const HEX6 = /^#[0-9a-f]{6}$/i;

function hexOr(value: string | undefined, fallback: string): string {
  return value && HEX6.test(value) ? value : fallback;
}

function mix(a: string, b: string, t: number): string {
  const x = hexToRgb(a);
  const y = hexToRgb(b);
  return rgbToHex(
    Math.round(x.r + (y.r - x.r) * t),
    Math.round(x.g + (y.g - x.g) * t),
    Math.round(x.b + (y.b - x.b) * t),
  );
}

/** Builds the salmon run's fills from the theme, for a canvas on `surface`. */
export function buildSalmonRunPalette(
  colors: ThemeColors,
  surface: "bg-main" | "bg-playerbar",
): SalmonRunPalette {
  const bg = hexOr(colors[surface], hexOr(colors["bg-main"], "#191918"));
  const light = isLightColor(bg);
  const accent = clampForContrast(hexOr(colors["color-accent"], "#4f5bd5"), bg, 3.0);
  const hover = clampForContrast(hexOr(colors["color-accent-hover"], accent), bg, 3.0);
  const text = hexOr(colors["color-text-primary"], light ? "#16181d" : "#f1f3f8");

  // White water leans toward the lighter of the theme's text and background, so falls and foam
  // read as white water in both light and dark themes, kept accent-tinted to stay apart from the salmon.
  const bright = light ? bg : text;
  const surfaceWater = mix(bg, accent, 0.6);
  const foam: IsoFaces[] = [];
  for (let k = 0; k < FOAM_STEPS; k++) {
    foam.push(
      isoFaces(mix(mix(surfaceWater, hover, 0.6), bright, 0.15 + (0.45 * k) / (FOAM_STEPS - 1))),
    );
  }
  // On the ladder the salmon lean further toward primary text, to stand out from accent water.
  const fishBody = clampForContrast(mix(text, hover, 0.5), bg, 4.5);
  const ladderFish = clampForContrast(mix(text, hover, 0.3), bg, 4.5);
  return {
    stone: isoFaces(mix(bg, text, light ? 0.3 : 0.2)),
    cap: isoFaces(mix(bg, text, light ? 0.4 : 0.3)),
    bank: isoFaces(mix(mix(bg, text, light ? 0.22 : 0.14), accent, 0.12)),
    deep: isoFaces(mix(bg, accent, 0.4)),
    water: isoFaces(surfaceWater),
    lightWash: voxelAlphaLadder(hover, LIGHT_STEPS, 0.1, 0.4),
    spill: isoFaces(mix(surfaceWater, bright, 0.35)),
    foam,
    // Ripples and glints stay dimmer than the salmon, so the salmon read as the river's subject.
    ripple: voxelAlphaLadder(mix(hover, bright, 0.6), FOAM_STEPS, 0.15, 0.75),
    glint: voxelAlphaLadder(mix(hover, bright, 0.45), FOAM_STEPS, 0.12, 0.6),
    fishBody,
    fishHead: hover,
    fishBodyFaces: isoFaces(ladderFish),
    fishHeadFaces: isoFaces(mix(ladderFish, hover, 0.5)),
    leaf: clampForContrast(mix(text, hover, 0.15), bg, 4.5),
    leafVein: hover,
  };
}

/** Furthest the playbar salmon turns from straight upstream, in radians. */
const MAX_VEER = 0.7;
/** Half the leaf's width at its widest, as a fraction of its length. */
const LEAF_HALF_WIDTH = 0.24;
/** Length of the leaf's stem, as a fraction of the blade's. */
const LEAF_STEM = 0.16;

/** The playbar slot (about 96×28 px): a lone salmon holding station, and the track-change leaf. */
const LONE_SALMON: LoneSalmonConfig = { fishLength: 1.1, leafSize: 0.7, leafCrossS: 6 };

/** Rows of cells for a playbar canvas `height` CSS px tall: cells at least 3 px apart. */
function loneRows(height: number): number {
  return Math.max(6, Math.min(12, Math.floor(height / 3)));
}

/**
 * Draws a `LoneSalmon` as an LED grid: lit cells for the salmon and leaf straight onto the
 * surface the canvas sits on. Allocates nothing per frame.
 */
class LoneSalmonRenderer {
  private readonly grid = new VoxelGrid();

  render(
    ctx: CanvasRenderingContext2D,
    width: number,
    height: number,
    scene: LoneSalmon,
    p: SalmonRunPalette,
  ): void {
    if (!Number.isFinite(width) || !Number.isFinite(height) || width <= 0 || height <= 0) return;
    const grid = this.grid;
    // Rows are fixed for the slot's height; the scene runs as many columns as fit at that pitch.
    const rows = loneRows(height);
    const pitch = Math.max(1, Math.floor(height / rows));
    grid.fit(width, height, Math.floor(width / pitch), rows);
    scene.setAspect(grid.cols / grid.rows);
    ctx.clearRect(0, 0, width, height);
    this.drawSalmon(ctx, scene, p);
    if (scene.leafActive) this.drawLeaf(ctx, scene, p);
  }

  /**
   * The salmon as cells: a body tapering to a forked tail that flexes with the tail beat, the
   * head in its own colour. Strokes are widened to at least half a cell so it keeps a solid
   * outline at any grid size.
   */
  private drawSalmon(ctx: CanvasRenderingContext2D, scene: LoneSalmon, p: SalmonRunPalette): void {
    const g = this.grid;
    const rows = g.rows;
    const unit = 1 / rows;
    const minHalf = unit * 0.5;
    // Always into the current, turned at most ~40° toward a bank: carried back after a dart, it
    // still points upstream.
    const veer = Math.atan2(scene.fishVY, Math.max(-scene.fishVX, 0.04));
    const angle = Math.PI - Math.max(-MAX_VEER, Math.min(MAX_VEER, veer));
    const cos = Math.cos(angle);
    const sin = Math.sin(angle);
    const len = scene.config.fishLength;
    const flex = Math.sin(scene.fishTail) * len * 0.12;
    const fx = scene.fishX;
    const fy = scene.fishY;
    const reach = len * 0.56;
    const c0 = Math.max(0, Math.floor((fx - reach) * rows));
    const c1 = Math.min(g.cols - 1, Math.ceil((fx + reach) * rows));
    const r0 = Math.max(0, Math.floor((fy - reach) * rows));
    const r1 = Math.min(rows - 1, Math.ceil((fy + reach) * rows));
    // Two passes, body then head, so each sets its fill once.
    for (let pass = 0; pass < 2; pass++) {
      ctx.fillStyle = pass === 0 ? p.fishBody : p.fishHead;
      for (let r = r0; r <= r1; r++) {
        const dy = (r + 0.5) * unit - fy;
        for (let c = c0; c <= c1; c++) {
          const dx = (c + 0.5) * unit - fx;
          // Into the fish's frame: `u` runs tail (-0.5) to nose (+0.5) in lengths.
          const u = (dx * cos + dy * sin) / len;
          let v = -dx * sin + dy * cos;
          if (u < 0) v -= flex * (u / 0.5) * (u / 0.5);
          if (fishPart(u, Math.abs(v), len, minHalf) === pass + 1) g.fillCell(ctx, c, r);
        }
      }
    }
  }

  /**
   * The leaf: a pointed oval tilted 45° (stem at the lower left, tip at the upper right), with a
   * stem and, once it's wide enough to show one, a midrib in the vein colour.
   */
  private drawLeaf(ctx: CanvasRenderingContext2D, scene: LoneSalmon, p: SalmonRunPalette): void {
    const g = this.grid;
    const len = Math.max(4, scene.config.leafSize * g.rows);
    const halfWidth = len * LEAF_HALF_WIDTH;
    const vein = halfWidth >= 1.6;
    const cx = scene.leafX * g.rows;
    const cy = scene.leafY * g.rows;
    const reach = len * 0.5 + len * LEAF_STEM + 1;
    const c0 = Math.floor(cx - reach);
    const c1 = Math.ceil(cx + reach);
    const r0 = Math.floor(cy - reach);
    const r1 = Math.ceil(cy + reach);
    // Two passes, blade then midrib, so each sets its fill once.
    for (let pass = 0; pass < (vein ? 2 : 1); pass++) {
      ctx.fillStyle = pass === 0 ? p.leaf : p.leafVein;
      for (let r = r0; r <= r1; r++) {
        for (let c = c0; c <= c1; c++) {
          if (!g.contains(c, r)) continue;
          const dx = c + 0.5 - cx;
          const dy = r + 0.5 - cy;
          // Into the leaf's frame: `u` runs stem end (0) to tip (1) of the blade, `v` across it.
          const u = ((dx - dy) * Math.SQRT1_2) / len + 0.5;
          const v = Math.abs(dx + dy) * Math.SQRT1_2;
          if (leafPart(u, v, len, halfWidth, vein) === pass + 1) g.fillCell(ctx, c, r);
        }
      }
    }
  }
}

/**
 * Which part of the leaf a cell is in, given its position `u` along the blade (0 at the stem end,
 * 1 at the tip) and distance `v` from the midline, both for a leaf `len` cells long and
 * `halfWidth` cells wide at its widest: 0 outside, 1 blade or stem, 2 midrib (only with `vein`).
 */
function leafPart(u: number, v: number, len: number, halfWidth: number, vein: boolean): number {
  if (u < 0) return u >= -LEAF_STEM && v <= 0.5 ? 1 : 0;
  if (u > 1) return 0;
  // Rounder toward the stem, drawn out to a point at the tip.
  const half = halfWidth * Math.pow(Math.sin(Math.PI * Math.pow(u, 0.8)), 0.75);
  if (v > Math.max(half, 0.5)) return 0;
  return vein && v <= 0.5 && u < 0.85 ? 2 : 1;
}

/**
 * Which part of a salmon `len` long a point is in, given its position `u` along the fish (tail
 * -0.5 to nose +0.5, in lengths) and distance `av` from the bent midline: 0 outside, 1 body or
 * tail, 2 head. Half-widths are at least `minHalf`, so thin parts still cover a cell.
 */
function fishPart(u: number, av: number, len: number, minHalf: number): number {
  if (u > 0.5 || u < -0.54) return 0;
  if (u >= -0.34) {
    // The body: an ellipse from the tail root to the nose, deepest just ahead of mid-length.
    const t = (u - 0.08) / 0.42;
    const half = 0.14 * len * Math.sqrt(Math.max(0, 1 - t * t));
    if (av > Math.max(half, minHalf)) return 0;
    return u > 0.22 ? 2 : 1;
  }
  // The tail fin, flaring from the tail root.
  const half = len * (0.035 + 0.5 * (-0.34 - u));
  return av <= Math.max(half, minHalf) ? 1 : 0;
}

/** The immersive stage: the fish ladder. */
class FishLadderInstance implements VisualizerInstance {
  private readonly ladder = new FishLadder();
  private readonly renderer = new FishLadderRenderer(this.ladder);

  setTheme(colors: ThemeColors): void {
    this.renderer.setPalette(buildSalmonRunPalette(colors, "bg-main"));
  }

  setTrack(track: VisualizerTrack | null): void {
    // With nothing loaded the ladder stays as it was; the next track still brings a leaf.
    if (track) this.ladder.setTrack(track.key);
  }

  push(frame: VisualizerFrame): void {
    this.ladder.push(frame);
  }

  step(dtMs: number, reducedMotion: boolean): boolean {
    return this.ladder.step(dtMs, reducedMotion);
  }

  // The stage is never a still preview (that's the playbar slot), so there's no preview case.
  render(ctx: CanvasRenderingContext2D, width: number, height: number): void {
    this.renderer.render(ctx, width, height);
  }
}

/** The playbar slot: a lone salmon. */
class LoneSalmonInstance implements VisualizerInstance {
  private readonly scene = new LoneSalmon(LONE_SALMON);
  private readonly renderer = new LoneSalmonRenderer();
  private palette!: SalmonRunPalette;

  setTheme(colors: ThemeColors): void {
    this.palette = buildSalmonRunPalette(colors, "bg-playerbar");
  }

  setTrack(track: VisualizerTrack | null): void {
    if (track) this.scene.setTrack(track.key);
  }

  push(frame: VisualizerFrame): void {
    this.scene.push(frame);
  }

  step(dtMs: number, reducedMotion: boolean): boolean {
    return this.scene.step(dtMs, reducedMotion);
  }

  render(ctx: CanvasRenderingContext2D, width: number, height: number): void {
    this.renderer.render(ctx, width, height, this.scene, this.palette);
  }
}

/**
 * A salmon run in the theme's colours. On the immersive stage it climbs a fish ladder laid out
 * afresh for each track, drawn as isometric voxels: mid and treble set the school's pace, a
 * treble transient sends a salmon leaping up a step (a weak one falls short), beats drop ripples
 * and brighten the falls, and a leaf floats down the ladder when the track changes. The playbar
 * slot shows a lone salmon holding station against the current, and the leaf.
 */
export const SALMON_RUN_PRESET: VisualizerPreset = {
  id: "salmon-run",
  labelKey: "visualizers.salmonRun",
  needs: ["spectrum"],
  create(size: VisualizerSize): VisualizerInstance {
    return size === "full" ? new FishLadderInstance() : new LoneSalmonInstance();
  },
};
