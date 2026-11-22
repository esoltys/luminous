import type { ThemeColors } from "../stores/theme.svelte";
import { clampForContrast, hexToRgb, isLightColor, rgbToHex } from "../utils/colorUtils";
import { LEAP_S, RIPPLE_POOL, SalmonRun, type SalmonRunConfig } from "./salmonRunEngine";
import {
  VoxelCellPainter,
  VoxelGrid,
  VoxelLayerCache,
  voxelAlphaLadder,
  voxelUnlitCellColor,
  type Ctx2D,
} from "./voxelGrid";
import type {
  VisualizerFrame,
  VisualizerInstance,
  VisualizerPreset,
  VisualizerSize,
  VisualizerTrack,
} from "./types";

// The salmon run is drawn as an LED grid, like the voxel spectrum and voxel logo: the simulation
// runs in world units (the height is 1) and each frame is rasterised onto square cells. The world
// is sized to the grid (`cols / rows` wide), so a cell is exactly `1 / rows` world units.

interface SalmonRunSizeConfig {
  readonly river: SalmonRunConfig;
  /**
   * Draw the river itself (water, banks, riverbed, boulders, current). Without it the salmon and
   * leaf are drawn straight onto the surface the canvas sits on.
   */
  readonly scenery: boolean;
  /** The surface the canvas sits on, which colours are derived from and contrast-checked against. */
  readonly surface: "bg-main" | "bg-playerbar";
  /** Rows of cells for a canvas `height` CSS px tall. */
  readonly rows: (height: number) => number;
}

const SIZES: Record<VisualizerSize, SalmonRunSizeConfig> = {
  // The immersive stage: a full reach of river with boulders, a school, leaps and ripples, on a
  // grid fine enough that a salmon is about nine cells long.
  full: {
    river: {
      fish: [5, 9],
      boulders: [4, 7],
      streaks: 60,
      pebbles: 70,
      fishLength: 0.18,
      leaf: true,
      leafSize: 0.28,
      leafCrossS: 20,
      ripples: true,
      transients: "leap",
      holdStation: false,
    },
    scenery: true,
    surface: "bg-main",
    rows: () => 48,
  },
  // The playbar slot (about 96×28 px): a lone salmon holding station in an unseen current, and the
  // track-change leaf drifting past, on cells at least 3 px apart so they stay crisp.
  mini: {
    river: {
      fish: [1, 1],
      boulders: [0, 0],
      streaks: 0,
      pebbles: 0,
      fishLength: 1.1,
      leaf: true,
      leafSize: 1.2,
      leafCrossS: 6,
      ripples: false,
      transients: "surge",
      holdStation: true,
    },
    scenery: false,
    surface: "bg-playerbar",
    rows: (height) => Math.max(6, Math.min(12, Math.floor(height / 3))),
  },
};

/** Bands each bank is shaded in, deepest at the edge; the band past them is open water. */
const BANK_BANDS = 4;
/** Fraction of the height each bank's shading covers. */
const BANK_DEPTH = 0.16;
/** How much deeper than open water the cells at the very edge of a bank are. */
const BANK_STRENGTH = 1.8;
/** Steps of the reduced-motion light shift, from unlit to fully lit. */
const LIGHT_STEPS = 6;
/** How much the most light lifts the water's cells, as a fraction of their unlit opacity. */
const MAX_LIGHT = 1.2;
/** Brightness steps of foam (wakes, ripples) and current streaks over the water. */
const FOAM_STEPS = 8;
/** Furthest a salmon turns from straight upstream, in radians. */
const MAX_VEER = 0.7;

/** Riverbed cell kinds, indices into each of `SalmonRunPalette.bed`. Water shades come last. */
const BED = {
  none: 0,
  pebble: 1,
  stone: 2,
  stoneLight: 3,
  stoneFoam: 4,
  water: 5,
} as const;
const WATER_SHADES = BANK_BANDS + 1;

/**
 * Every fill the river is drawn with, from the theme alone so it sits with the other voxel
 * presets: water and banks are the shared unlit cell in deepening shades, stones the border
 * colour, foam and ripples the accent hover, current streaks the accent, and the salmon and leaf
 * lit cells contrast-checked against the surface the canvas sits on. Gaps between cells are left
 * transparent, so that surface shows through as it does for the other voxel presets.
 */
export interface SalmonRunPalette {
  /** Riverbed fills per reduced-motion light step, each indexed by `BED`. */
  readonly bed: readonly (readonly string[])[];
  /** Foam (wakes, ripples) from faint to bright, `FOAM_STEPS` long. */
  readonly foam: readonly string[];
  /** Current streaks from faint to bright, `FOAM_STEPS` long. */
  readonly streak: readonly string[];
  readonly fishBody: string;
  readonly fishHead: string;
  readonly leaf: string;
  /** The leaf's midrib and the like, a step down from `leaf`. */
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

/** `hex` at opacity `alpha`, as a fill. */
function rgba(hex: string, alpha: number): string {
  const { r, g, b } = hexToRgb(hex);
  return `rgba(${r}, ${g}, ${b}, ${Number(Math.min(1, alpha).toFixed(3))})`;
}

/** Builds the river's fills from the theme, for a canvas on `surface`. */
export function buildSalmonRunPalette(
  colors: ThemeColors,
  surface: "bg-main" | "bg-playerbar",
): SalmonRunPalette {
  const bg = hexOr(colors[surface], hexOr(colors["bg-main"], "#191918"));
  const light = isLightColor(bg);
  const accent = clampForContrast(hexOr(colors["color-accent"], "#4f5bd5"), bg, 3.0);
  const hover = clampForContrast(hexOr(colors["color-accent-hover"], accent), bg, 3.0);
  const text = hexOr(colors["color-text-primary"], light ? "#16181d" : "#f1f3f8");
  const border = hexOr(colors["color-border"], accent);

  const bed: string[][] = [];
  for (let l = 0; l < LIGHT_STEPS; l++) {
    const lift = 1 + (MAX_LIGHT * l) / (LIGHT_STEPS - 1);
    const fills: string[] = new Array(BED.water + WATER_SHADES);
    fills[BED.none] = "transparent";
    fills[BED.pebble] = voxelUnlitCellColor(colors, 2.4);
    fills[BED.stone] = rgba(border, light ? 0.5 : 0.42);
    fills[BED.stoneLight] = rgba(border, light ? 0.72 : 0.62);
    fills[BED.stoneFoam] = rgba(hover, 0.3);
    for (let b = 0; b < WATER_SHADES; b++) {
      const depth = b < BANK_BANDS ? BANK_STRENGTH * (1 - (b + 0.5) / BANK_BANDS) : 0;
      fills[BED.water + b] = voxelUnlitCellColor(colors, (1 + depth) * lift);
    }
    bed.push(fills);
  }
  // Foam and current stay dimmer than any lit cell, so the salmon read as the river's subject.
  return {
    bed,
    foam: voxelAlphaLadder(hover, FOAM_STEPS, 0.12, 0.55),
    streak: voxelAlphaLadder(accent, FOAM_STEPS, 0.12, 0.55),
    // Lit cells, brightening toward primary text like the voxel spectrum's peak caps; the leaf
    // is the brightest thing on the river.
    fishBody: clampForContrast(mix(text, hover, 0.5), bg, 4.5),
    fishHead: hover,
    leaf: clampForContrast(mix(text, hover, 0.15), bg, 4.5),
    leafVein: hover,
  };
}

/** Half the leaf's width at its widest, as a fraction of its length. */
const LEAF_HALF_WIDTH = 0.24;
/** Length of the leaf's stem, as a fraction of the blade's. */
const LEAF_STEM = 0.16;

/**
 * The riverbed rasterised onto the grid (water and banks, pebbles, boulders), which holds still
 * for a whole track: a `BED` kind per cell, so moving things can tell open water from stone, and
 * painted onto a cached layer that each frame blits with one `drawImage`.
 */
class RiverbedLayer {
  /** A `BED` kind per cell, row-major. */
  styles = new Uint8Array(0);
  private readonly painter = new VoxelCellPainter();
  private readonly layer = new VoxelLayerCache();
  private layoutVersion = -1;
  private drawnPalette: SalmonRunPalette | null = null;
  private drawnLight = -1;
  private stale = true;
  private grid = new VoxelGrid();
  private fills: readonly string[] = [];
  private readonly paint = (c: Ctx2D) => this.painter.paint(c, this.grid, this.styles, this.fills);

  /** Rasterises the riverbed if the grid (`gridChanged`) or the track's layout changed. */
  update(grid: VoxelGrid, gridChanged: boolean, river: SalmonRun): void {
    if (!gridChanged && river.layoutVersion === this.layoutVersion) return;
    this.layoutVersion = river.layoutVersion;
    this.grid = grid;
    this.stale = true;
    const rows = grid.rows;
    const cols = grid.cols;
    const count = cols * rows;
    if (this.styles.length < count) this.styles = new Uint8Array(count);
    const s = this.styles;
    const unit = 1 / rows;

    const bankRows = BANK_DEPTH * rows;
    for (let r = 0; r < rows; r++) {
      const fromEdge = Math.min(r, rows - 1 - r);
      const band = Math.min(BANK_BANDS, Math.floor((fromEdge / bankRows) * BANK_BANDS));
      s.fill(BED.water + band, r * cols, (r + 1) * cols);
    }
    for (let i = 0; i < river.pebbleFX.length; i++) {
      const c = Math.floor(river.pebbleFX[i] * cols);
      const r = Math.floor(river.pebbleY[i] * rows);
      if (grid.contains(c, r)) s[r * cols + c] = BED.pebble;
    }
    for (let b = 0; b < river.boulderCount; b++) {
      const bx = river.boulderFX[b] * river.aspect;
      const by = river.boulderY[b];
      const rad = Math.max(river.boulderR[b], unit);
      const foamR = rad + unit * 0.9;
      const c0 = Math.max(0, Math.floor((bx - foamR) * rows));
      const c1 = Math.min(cols - 1, Math.ceil((bx + foamR) * rows));
      const r0 = Math.max(0, Math.floor((by - foamR) * rows));
      const r1 = Math.min(rows - 1, Math.ceil((by + foamR) * rows));
      for (let r = r0; r <= r1; r++) {
        const y = (r + 0.5) * unit;
        for (let c = c0; c <= c1; c++) {
          const x = (c + 0.5) * unit;
          const dx = x - bx;
          const dy = (y - by) / 0.86;
          const d = Math.hypot(dx, dy);
          const i = r * cols + c;
          if (d <= rad) {
            // Lit from the upper left.
            s[i] = dx + dy < -rad * 0.45 ? BED.stoneLight : BED.stone;
          } else if (d <= foamR && dx < -rad * 0.3) {
            // Foam where the current meets the upstream face.
            s[i] = BED.stoneFoam;
          }
        }
      }
    }
  }

  /** Draws the riverbed, at reduced-motion `light` step, over the whole canvas. */
  draw(
    ctx: CanvasRenderingContext2D,
    width: number,
    height: number,
    p: SalmonRunPalette,
    light: number,
  ): void {
    const stale = this.stale || p !== this.drawnPalette || light !== this.drawnLight;
    this.fills = p.bed[light];
    this.layer.blit(ctx, width, height, stale, this.paint);
    this.stale = false;
    this.drawnPalette = p;
    this.drawnLight = light;
  }
}

/**
 * Draws a `SalmonRun` as an LED grid. The riverbed is a cached layer; the moving parts (wakes,
 * current, ripples, salmon, leaf) are rasterised onto the grid each frame, a few hundred cells
 * at most. Allocates nothing per frame.
 */
class VoxelRiverRenderer {
  private readonly grid = new VoxelGrid();
  private readonly bed = new RiverbedLayer();

  constructor(private readonly config: SalmonRunSizeConfig) {}

  render(
    ctx: CanvasRenderingContext2D,
    width: number,
    height: number,
    river: SalmonRun,
    p: SalmonRunPalette,
  ): void {
    if (!Number.isFinite(width) || !Number.isFinite(height) || width <= 0 || height <= 0) return;
    const grid = this.grid;
    // Rows are fixed per size; the river runs as many columns as fit at that pitch.
    const rows = this.config.rows(height);
    const pitch = Math.max(1, Math.floor(height / rows));
    const gridChanged = grid.fit(width, height, Math.floor(width / pitch), rows);
    river.setAspect(grid.cols / grid.rows);

    ctx.clearRect(0, 0, width, height);
    if (this.config.scenery) {
      this.bed.update(grid, gridChanged, river);
      // Under reduced motion a beat lifts the light on the water instead of making waves.
      const light = Math.min(LIGHT_STEPS - 1, Math.round(river.glow * (LIGHT_STEPS - 1)));
      this.bed.draw(ctx, width, height, p, light);
      this.drawWakes(ctx, river, p);
      this.drawStreaks(ctx, river, p);
      this.drawRipples(ctx, river, p);
    }
    this.drawSchool(ctx, river, p);
    if (river.leafActive) this.drawLeaf(ctx, river, p);
  }

  private fillCell(ctx: CanvasRenderingContext2D, col: number, row: number): void {
    this.grid.fillCell(ctx, col, row);
  }

  /** Whether the cell at (`col`, `row`) is on the grid and open water, which effects may light. */
  private isWater(col: number, row: number): boolean {
    const g = this.grid;
    return g.contains(col, row) && this.bed.styles[row * g.cols + col] >= BED.water;
  }

  /** `FOAM_STEPS` rung for brightness `b` in [0, 1], or -1 when too faint to show. */
  private rung(b: number): number {
    const k = Math.round(b * FOAM_STEPS) - 1;
    return Math.min(FOAM_STEPS - 1, k);
  }

  /** Vertical shift of the current at (`x`, `y`) as it parts around the boulders. */
  private deflect(river: SalmonRun, x: number, y: number): number {
    let dy = 0;
    for (let b = 0; b < river.boulderCount; b++) {
      const ox = x - river.boulderFX[b] * river.aspect;
      const oy = y - river.boulderY[b];
      const r = river.boulderR[b];
      dy += (r * r * 1.6 * oy) / (ox * ox + oy * oy + r * r);
    }
    return dy;
  }

  /** Current streaks: short runs of glinting cells drifting downstream, parting round boulders. */
  private drawStreaks(ctx: CanvasRenderingContext2D, river: SalmonRun, p: SalmonRunPalette): void {
    const n = river.streakX.length;
    const k = this.rung((0.45 + 0.35 * river.treble) * (0.45 + 0.55 * river.activity));
    if (n === 0 || k < 0) return;
    const rows = this.grid.rows;
    ctx.fillStyle = p.streak[k];
    for (let i = 0; i < n; i++) {
      const x1 = river.streakX[i];
      const y = river.streakY[i];
      const c0 = Math.floor((x1 - river.streakLen[i]) * rows);
      const c1 = Math.floor(x1 * rows);
      for (let c = c0; c <= c1; c++) {
        const x = (c + 0.5) / rows;
        const r = Math.floor((y + this.deflect(river, x, y)) * rows);
        if (this.isWater(c, r)) this.fillCell(ctx, c, r);
      }
    }
  }

  /** Eddy wakes downstream of each boulder: arcs of foam that brighten with the bass. */
  private drawWakes(ctx: CanvasRenderingContext2D, river: SalmonRun, p: SalmonRunPalette): void {
    if (river.boulderCount === 0) return;
    const k = this.rung((0.15 + 0.6 * river.bass) * (0.5 + 0.5 * river.activity));
    if (k < 0) return;
    const rows = this.grid.rows;
    const unit = 1 / rows;
    ctx.fillStyle = p.foam[k];
    for (let b = 0; b < river.boulderCount; b++) {
      const bx = river.boulderFX[b] * river.aspect;
      const by = river.boulderY[b];
      const rad = river.boulderR[b];
      const outer = rad * 2.5 + unit;
      const c0 = Math.floor(bx * rows);
      const c1 = Math.ceil((bx + outer) * rows);
      const r0 = Math.floor((by - outer) * rows);
      const r1 = Math.ceil((by + outer) * rows);
      for (let r = r0; r <= r1; r++) {
        const y = (r + 0.5) * unit - by;
        for (let c = c0; c <= c1; c++) {
          const x = (c + 0.5) * unit - bx;
          const d = Math.hypot(x, y);
          const a = Math.abs(Math.atan2(y, x));
          // Two arcs, narrowing as the wake trails off.
          const onArc =
            (Math.abs(d - rad * 1.6) <= unit * 0.5 && a <= 0.55) ||
            (Math.abs(d - rad * 2.3) <= unit * 0.5 && a <= 0.4);
          if (onArc && this.isWater(c, r)) this.fillCell(ctx, c, r);
        }
      }
    }
  }

  private drawRipples(ctx: CanvasRenderingContext2D, river: SalmonRun, p: SalmonRunPalette): void {
    const rows = this.grid.rows;
    const unit = 1 / rows;
    for (let i = 0; i < RIPPLE_POOL; i++) {
      const k = this.rung(river.rippleAlpha(i));
      if (k < 0) continue;
      const cx = river.rippleX[i];
      const cy = river.rippleY[i];
      const rad = river.rippleRadius(i);
      const reach = rad + unit;
      ctx.fillStyle = p.foam[k];
      const c0 = Math.floor((cx - reach) * rows);
      const c1 = Math.ceil((cx + reach) * rows);
      const r0 = Math.floor((cy - reach) * rows);
      const r1 = Math.ceil((cy + reach) * rows);
      for (let r = r0; r <= r1; r++) {
        const y = (r + 0.5) * unit - cy;
        for (let c = c0; c <= c1; c++) {
          const d = Math.hypot((c + 0.5) * unit - cx, y);
          if (Math.abs(d - rad) <= unit * 0.55 && this.isWater(c, r)) this.fillCell(ctx, c, r);
        }
      }
    }
  }

  /**
   * Each salmon as cells: a body tapering to a forked tail that flexes with the tail beat, the
   * head in its own colour. Strokes are widened to at least half a cell so a fish keeps a solid
   * outline at any grid size.
   */
  private drawSchool(ctx: CanvasRenderingContext2D, river: SalmonRun, p: SalmonRunPalette): void {
    const g = this.grid;
    const rows = g.rows;
    const unit = 1 / rows;
    const minHalf = unit * 0.5;
    const baseLen = river.config.fishLength;
    for (let i = 0; i < river.fishCount; i++) {
      // Always into the current, turned at most ~40° toward a bank: a salmon sidestepping a
      // boulder, or carried back after a dart, still points upstream.
      const veer = Math.atan2(river.fishVY[i], Math.max(-river.fishVX[i], 0.04));
      const angle = Math.PI - Math.max(-MAX_VEER, Math.min(MAX_VEER, veer));
      const cos = Math.cos(angle);
      const sin = Math.sin(angle);
      const leap = river.fishLeap[i];
      const lift = leap > 0 ? Math.sin(Math.PI * (1 - leap / LEAP_S)) : 0;
      const len = baseLen * river.fishScale[i] * (1 + 0.45 * lift);
      const flex = Math.sin(river.fishTail[i]) * len * 0.12;
      const fx = river.fishX[i];
      const fy = river.fishY[i];

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
            const part = fishPart(u, Math.abs(v), len, minHalf);
            if (part === pass + 1) this.fillCell(ctx, c, r);
          }
        }
      }
    }
  }

  /**
   * The leaf: a pointed oval tilted 45° (stem at the lower left, tip at the upper right), with a
   * stem and, once it's wide enough to show one, a midrib in the vein colour. It's shaped per cell
   * from its size on this grid, so it reads as a leaf at the stage's size and the playbar's.
   */
  private drawLeaf(ctx: CanvasRenderingContext2D, river: SalmonRun, p: SalmonRunPalette): void {
    const g = this.grid;
    const len = Math.max(4, river.config.leafSize * g.rows);
    const halfWidth = len * LEAF_HALF_WIDTH;
    const vein = halfWidth >= 1.6;
    const cx = river.leafX * g.rows;
    const cy = river.leafY * g.rows;
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
          const part = leafPart(u, v, len, halfWidth, vein);
          if (part === pass + 1) this.fillCell(ctx, c, r);
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

class SalmonRunInstance implements VisualizerInstance {
  private readonly river: SalmonRun;
  private readonly renderer: VoxelRiverRenderer;
  private palette!: SalmonRunPalette;

  constructor(private readonly config: SalmonRunSizeConfig) {
    this.river = new SalmonRun(config.river);
    this.renderer = new VoxelRiverRenderer(config);
  }

  setTheme(colors: ThemeColors): void {
    this.palette = buildSalmonRunPalette(colors, this.config.surface);
  }

  setTrack(track: VisualizerTrack | null): void {
    // With nothing loaded the river stays as it was; the next track still brings a leaf.
    if (track) this.river.setTrack(track.key);
  }

  push(frame: VisualizerFrame): void {
    this.river.push(frame);
  }

  step(dtMs: number, reducedMotion: boolean): boolean {
    return this.river.step(dtMs, reducedMotion);
  }

  // The still preview needs no special case: the host doesn't step a preview, so the river as it
  // stands holds still.
  render(ctx: CanvasRenderingContext2D, width: number, height: number): void {
    this.renderer.render(ctx, width, height, this.river, this.palette);
  }
}

/**
 * A salmon run seen from above, as an LED grid in the theme's colours: a mountain river laid out
 * afresh for each track, with salmon swimming upstream past boulders. Mid and treble set their
 * pace, treble transients make one surge or leap, beats drop ripples, and a leaf drifts by
 * when the track changes. The playbar slot shows a lone salmon holding station and the leaf.
 */
export const SALMON_RUN_PRESET: VisualizerPreset = {
  id: "salmon-run",
  labelKey: "visualizers.salmonRun",
  needs: ["spectrum"],
  create(size: VisualizerSize): VisualizerInstance {
    return new SalmonRunInstance(SIZES[size]);
  },
};
