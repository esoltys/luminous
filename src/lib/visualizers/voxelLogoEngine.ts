import type { ThemeColors } from "../stores/theme.svelte";
import { clampForContrast } from "../utils/colorUtils";
import type { VisualizerFrame } from "./types";
import {
  VoxelCellPainter,
  VoxelGrid,
  VoxelLayerCache,
  voxelAlphaLadder,
  voxelTrackHighlight,
  voxelUnlitCellColor,
  type Ctx2D,
} from "./voxelGrid";

// The brand mark (docs/luminous-mark.svg), in its own 200×200 units.
const CENTER = 100;
const DISC_R = 68;
const RIM_R = 77;
const RIM_HALF = 7;
const RING_R = 92;
const RING_HALF = 4;
const RING_OUTER = RING_R + RING_HALF;
const BURST_X = 150.6;
const BURST_Y = 59.6;
const BURST_R = 17.4;

// Audio-driven geometry, in the same ranges as ReactiveLogoBrand.svelte (half stroke widths).
const RIM_HALF_MIN = 6;
const RIM_HALF_RANGE = 6;
const RING_HALF_MIN = 2.5;
const RING_HALF_RANGE = 8;
const HALO_R_MIN = 18;
const HALO_R_RANGE = 26;
const GLOW_R_MIN = RING_OUTER;
const GLOW_R_RANGE = 34;
/** Under reduced motion the geometry holds at mid-range sizes and only brightness follows the music. */
const RIM_HALF_STILL = 9;
const RING_HALF_STILL = 6.5;
const HALO_R_STILL = 31;
const GLOW_R_STILL = 112;
/** The glow is a faint halo: its brightest cell is this fraction of full brightness. */
const GLOW_PEAK = 0.45;

// Beat ripples (#2270): rings of light that leave the ring's outer edge on each onset.
/** Ripples in flight at once; a new beat replaces the oldest beyond this. */
const RIPPLE_POOL = 6;
/** Outward speed, in mark units per second: a ripple lives about 1.5 s. */
const RIPPLE_SPEED = 150;
/**
 * Radius at which a ripple has faded out. The stage is drawn edge to edge (the grid plus the
 * housing around it), so on a 16:9 stage, about 250 units to either side, a ripple leaves the
 * left and right edges at about a third of its brightness rather than vanishing mid-field.
 */
const RIPPLE_END_R = RING_OUTER + 220;
/** Half the ripple's width, in mark units, before widening to at least half a cell. */
const RIPPLE_HALF = 3;
/** Brightness of the faintest onset's ripple as it leaves the ring; the strongest starts at 1. */
const RIPPLE_FLOOR = 0.4;

const ATTACK_RATE = 38;
const DECAY_RATE = 10.5;
const DECAY_RATE_REDUCED = 6.5;
const EPSILON = 0.002;

/** Steps in each layer's brightness ladder. */
const LADDER = 10;
/** Angular speed of the treble harmonic shimmer on the outer ring, in radians per second. */
const SHIMMER_SPEED = 4.2;

/** Values of `LogoGrid.restLayer`: what a cell shows when no audio-driven layer reaches it. */
export const LOGO_REST = { none: 0, disc: 1, rim: 2, ring: 3 } as const;

/**
 * The mark rasterized onto a square grid of `size` cells covering `[min, max]` mark units on
 * both axes. Built once; each cell stores its distances from the mark's centre and the burst's
 * centre, and the layer it belongs to at rest: the mark's own strokes, widened to at least half
 * a cell either side so the rim, ring and disc each keep a closed loop of cells at any resolution.
 */
export class LogoGrid {
  readonly size: number;
  /** Mark units per cell. */
  readonly cellUnits: number;
  readonly markDistance: Float32Array;
  readonly burstDistance: Float32Array;
  readonly restLayer: Uint8Array;
  readonly cellAngle: Float32Array;
  /** Mark units at the grid's top-left corner, on both axes. */
  private readonly min: number;

  constructor(size: number, min: number, max: number) {
    this.size = Math.max(1, Math.floor(size));
    this.min = min;
    this.cellUnits = (max - min) / this.size;
    const n = this.size * this.size;
    this.markDistance = new Float32Array(n);
    this.burstDistance = new Float32Array(n);
    this.restLayer = new Uint8Array(n);
    this.cellAngle = new Float32Array(n);
    const halfCell = this.cellUnits * 0.5;
    const rimReach = Math.max(RIM_HALF, halfCell);
    const ringReach = Math.max(RING_HALF, halfCell);
    for (let row = 0; row < this.size; row++) {
      const y = min + (row + 0.5) * this.cellUnits;
      for (let col = 0; col < this.size; col++) {
        const x = min + (col + 0.5) * this.cellUnits;
        const i = row * this.size + col;
        const d = Math.hypot(x - CENTER, y - CENTER);
        this.markDistance[i] = d;
        this.burstDistance[i] = Math.hypot(x - BURST_X, y - BURST_Y);
        this.cellAngle[i] = Math.atan2(y - CENTER, x - CENTER);
        // The ring is painted over the rim, so it wins where the widened strokes overlap.
        this.restLayer[i] =
          d <= DISC_R
            ? LOGO_REST.disc
            : Math.abs(d - RING_R) <= ringReach
              ? LOGO_REST.ring
              : Math.abs(d - RIM_R) <= rimReach
                ? LOGO_REST.rim
                : LOGO_REST.none;
      }
    }
  }

  /**
   * Distance from the mark's centre of the cell at (`col`, `row`), which may lie outside the
   * grid: cells beyond it continue at the same pitch (the housing around the grid).
   */
  markDistanceAt(col: number, row: number): number {
    const x = this.min + (col + 0.5) * this.cellUnits;
    const y = this.min + (row + 0.5) * this.cellUnits;
    return Math.hypot(x - CENTER, y - CENTER);
  }
}

function bandAverage(bands: readonly number[], from: number, to: number): number {
  let sum = 0;
  for (let i = from; i < to; i++) sum += bands[i] || 0;
  return sum / (to - from);
}

/**
 * The logo's audio state: bass (burst), mids (rim and glow) and treble (ring), each in [0, 1],
 * plus the beat ripples in flight. `push` sets targets from a frame, using the band split and
 * gains of the reactive logo; a beat onset lifts the burst target, so it flares and then decays
 * with the bass smoothing, and sends a ripple outward from the ring. Ripples only move in `step`;
 * `settle` and reduced motion drop them, since a ripple is pure motion with no resting state.
 */
export class LogoLevels {
  bass = 0;
  mid = 0;
  treble = 0;
  shimmerPhase = 0;
  private bassTarget = 0;
  private midTarget = 0;
  private trebleTarget = 0;
  /** Each ripple's radius in mark units, and its starting brightness (0 = free slot). */
  private readonly rippleRadius = new Float32Array(RIPPLE_POOL);
  private readonly rippleStrength = new Float32Array(RIPPLE_POOL);

  /** Levels held at fixed values, e.g. for a still preview. */
  static fixed(bass: number, mid: number, treble: number): LogoLevels {
    const levels = new LogoLevels();
    levels.bass = levels.bassTarget = bass;
    levels.mid = levels.midTarget = mid;
    levels.treble = levels.trebleTarget = treble;
    return levels;
  }

  push(frame: VisualizerFrame): void {
    const bands = frame.bands;
    if (!frame.playing || bands.length === 0) {
      this.bassTarget = this.midTarget = this.trebleTarget = 0;
      return;
    }
    const onset = Math.min(1, frame.onset || 0);
    const bass = Math.min(1, bandAverage(bands, 0, 8) * 1.4);
    this.bassTarget = Math.max(bass, onset);
    this.midTarget = Math.min(1, bandAverage(bands, 8, 20) * 1.4);
    this.trebleTarget = Math.min(1, bandAverage(bands, 20, 32) * 1.6);
    if (onset > 0) this.spawnRipple(onset);
  }

  /** Starts a ripple at the ring's outer edge, in a free slot or else over the oldest one. */
  private spawnRipple(onset: number): void {
    let slot = 0;
    for (let i = 0; i < RIPPLE_POOL; i++) {
      if (this.rippleStrength[i] === 0) {
        slot = i;
        break;
      }
      if (this.rippleRadius[i] > this.rippleRadius[slot]) slot = i;
    }
    this.rippleRadius[slot] = RING_OUTER;
    this.rippleStrength[slot] = RIPPLE_FLOOR + (1 - RIPPLE_FLOOR) * onset;
  }

  /** Jumps straight to the targets, dropping any ripples. */
  settle(): void {
    this.bass = this.bassTarget;
    this.mid = this.midTarget;
    this.treble = this.trebleTarget;
    this.rippleStrength.fill(0);
  }

  /**
   * Eases towards the targets: fast attack, slower decay (slower still under reduced motion), and
   * moves ripples outward (reduced motion drops them). Returns false once every level has arrived
   * and no ripple is in flight.
   */
  step(dtMs: number, reducedMotion: boolean): boolean {
    const dtSec = Math.max(0, Math.min(100, dtMs)) / 1000;
    const attack = 1 - Math.exp(-ATTACK_RATE * dtSec);
    const decay = 1 - Math.exp(-(reducedMotion ? DECAY_RATE_REDUCED : DECAY_RATE) * dtSec);
    this.bass = approach(this.bass, this.bassTarget, attack, decay);
    this.mid = approach(this.mid, this.midTarget, attack, decay);
    this.treble = approach(this.treble, this.trebleTarget, attack, decay);
    if (reducedMotion) {
      this.shimmerPhase = 0;
    } else if (this.treble > 0.12) {
      this.shimmerPhase = (this.shimmerPhase + SHIMMER_SPEED * dtSec) % (Math.PI * 2);
    }
    let rippling = false;
    for (let i = 0; i < RIPPLE_POOL; i++) {
      if (this.rippleStrength[i] === 0) continue;
      this.rippleRadius[i] += RIPPLE_SPEED * dtSec;
      if (reducedMotion || this.rippleRadius[i] >= RIPPLE_END_R) this.rippleStrength[i] = 0;
      else rippling = true;
    }
    return (
      rippling ||
      (!reducedMotion && this.treble > 0.2) ||
      this.bass !== this.bassTarget ||
      this.mid !== this.midTarget ||
      this.treble !== this.trebleTarget
    );
  }

  /** Whether any ripple is in flight. */
  get rippling(): boolean {
    for (let i = 0; i < RIPPLE_POOL; i++) if (this.rippleStrength[i] > 0) return true;
    return false;
  }

  /**
   * Brightness in [0, 1] of the ripples at distance `d` from the mark's centre: the brightest of
   * those whose band, `halfWidth` either side of its radius, covers `d`; 0 where none does. A
   * ripple fades linearly from its starting brightness at the ring to nothing at `RIPPLE_END_R`,
   * which on most stages is past the canvas edge.
   */
  rippleAt(d: number, halfWidth: number): number {
    let brightest = 0;
    for (let i = 0; i < RIPPLE_POOL; i++) {
      const strength = this.rippleStrength[i];
      if (strength === 0) continue;
      const r = this.rippleRadius[i];
      if (Math.abs(d - r) > halfWidth) continue;
      const b = strength * (1 - (r - RING_OUTER) / (RIPPLE_END_R - RING_OUTER));
      if (b > brightest) brightest = b;
    }
    return brightest;
  }
}

function approach(current: number, target: number, attack: number, decay: number): number {
  const next = current + (target - current) * (target >= current ? attack : decay);
  return Math.abs(target - next) < EPSILON ? target : next;
}

/**
 * The layout of `VoxelLogoPalette.styles`. `none` (draw nothing), `housing` and `disc` are single
 * fills; `rim`, `ring`, `glow` and `burst` each start a brightness ladder of `ladder` fills, dimmest
 * first. `count` is the number of styles.
 */
export const LOGO_STYLE = {
  none: 0,
  housing: 1,
  disc: 2,
  rim: 3,
  ring: 3 + LADDER,
  glow: 3 + 2 * LADDER,
  burst: 3 + 3 * LADDER,
  ladder: LADDER,
  count: 3 + 4 * LADDER,
} as const;

export interface VoxelLogoPalette {
  /** Every fill the renderer uses, laid out as `LOGO_STYLE` describes. */
  readonly styles: readonly string[];
}

/**
 * Builds the logo's fills against the surface it's drawn on (`bg-playerbar` for the playbar
 * slot, `bg-main` for the stage): rim and glow in the accent, ring in accent-hover (the colours
 * the reactive logo uses while playing), burst near white, each contrast-clamped, and the disc as
 * the mark's dark silhouette.
 */
export function buildVoxelLogoPalette(
  colors: ThemeColors,
  surface: "bg-main" | "bg-playerbar",
  trackAccent?: string | null,
): VoxelLogoPalette {
  const bg = colors[surface] || colors["bg-main"] || "#191918";
  const accent = clampForContrast(colors["color-accent"] || "#4f5bd5", bg, 3.0);
  const baseHover = clampForContrast(
    colors["color-accent-hover"] || colors["color-accent"] || "#a78bfa",
    bg,
    3.0,
  );
  const hover = voxelTrackHighlight(baseHover, bg, trackAccent, 0.55);
  const baseBurst = clampForContrast("#ffffff", bg, 3.0);
  const burst = voxelTrackHighlight(baseBurst, bg, trackAccent, 0.25);

  const styles: string[] = new Array(LOGO_STYLE.count);
  styles[LOGO_STYLE.none] = "transparent";
  // Only the full view draws housing, on `bg-main`, matching the voxel spectrum's.
  styles[LOGO_STYLE.housing] = voxelUnlitCellColor(colors);
  styles[LOGO_STYLE.disc] = "#0a0a0d";
  const ladder = (base: number, hex: string) => {
    const steps = voxelAlphaLadder(hex, LADDER);
    for (let k = 0; k < LADDER; k++) styles[base + k] = steps[k];
  };
  ladder(LOGO_STYLE.rim, accent);
  ladder(LOGO_STYLE.ring, hover);
  ladder(LOGO_STYLE.glow, accent);
  ladder(LOGO_STYLE.burst, burst);
  return { styles };
}

/** Ladder step for brightness `b` in [0, 1]; 0 means too faint to draw. */
function rung(b: number): number {
  return Math.max(0, Math.min(LADDER - 1, Math.round(b * (LADDER - 1))));
}

export interface VoxelLogoRendererOptions {
  /** Grids to draw with, from finest to coarsest; the finest that fits the canvas is used. */
  readonly grids: readonly LogoGrid[];
  /** Fill the rest of the canvas with unlit housing cells, as the voxel spectrum does. */
  readonly housing: boolean;
  /** Draw the mid-driven glow halo around the ring. */
  readonly glow: boolean;
  /**
   * Draw beat ripples travelling out from the ring to the canvas edges (never under reduced
   * motion).
   */
  readonly ripples: boolean;
}

/** Minimum cell pitch (CSS px) at which outer housing cells are drawn. */
const MIN_HOUSING_PITCH = 3;

/**
 * The style of a cell `d` mark units from the centre under the ripples in `levels`: a step of
 * the ring's ladder, or `LOGO_STYLE.none` where no ripple is bright enough to show.
 */
function rippleStyle(levels: LogoLevels, d: number, halfWidth: number): number {
  const k = rung(levels.rippleAt(d, halfWidth));
  return k > 0 ? LOGO_STYLE.ring + k : LOGO_STYLE.none;
}

const housingSpan = new Int32Array(4);

/** The housing: unlit cells covering the rest of a `width`×`height` canvas around `grid`. */
function drawOuterHousing(
  ctx: Ctx2D,
  width: number,
  height: number,
  grid: VoxelGrid,
  fill: string,
): void {
  ctx.fillStyle = fill;
  const span = housingSpan;
  grid.span(width, height, span);
  for (let row = span[1]; row < span[3]; row++) {
    for (let col = span[0]; col < span[2]; col++) {
      if (!grid.contains(col, row)) grid.fillCell(ctx, col, row);
    }
  }
}

const rippleSpan = new Int32Array(4);

/**
 * Draws the ripples where they cross the housing, so they travel on to the canvas edges. Each
 * lit cell is cleared first, so it shows the same fill over the stage as the ripple cells inside
 * the grid do rather than that fill over a housing cell.
 */
function drawOuterRipples(
  ctx: CanvasRenderingContext2D,
  width: number,
  height: number,
  grid: VoxelGrid,
  logo: LogoGrid,
  levels: LogoLevels,
  halfWidth: number,
  styles: readonly string[],
): void {
  const span = rippleSpan;
  grid.span(width, height, span);
  let current: number = LOGO_STYLE.none;
  for (let row = span[1]; row < span[3]; row++) {
    const y = grid.originY + row * grid.pitch;
    for (let col = span[0]; col < span[2]; col++) {
      if (grid.contains(col, row)) continue;
      const style = rippleStyle(levels, logo.markDistanceAt(col, row), halfWidth);
      if (style === LOGO_STYLE.none) continue;
      if (style !== current) {
        ctx.fillStyle = styles[style];
        current = style;
      }
      const x = grid.originX + col * grid.pitch;
      ctx.clearRect(x, y, grid.cell, grid.cell);
      ctx.fillRect(x, y, grid.cell, grid.cell);
    }
  }
}

/**
 * Draws the mark as an LED grid, classifying each cell into a fill and painting them with the
 * shared voxel cell painter; the housing around the grid is a cached layer. Drawing allocates
 * nothing once the largest grid has been drawn.
 */
export class VoxelLogoRenderer {
  private readonly grids: readonly LogoGrid[];
  private readonly housing: boolean;
  private readonly glow: boolean;
  private readonly ripples: boolean;
  private readonly cells = new VoxelGrid();
  private readonly painter = new VoxelCellPainter();
  private readonly housingLayer = new VoxelLayerCache();
  private readonly cellStyle: Uint8Array;
  private housingFill = "";
  private readonly paintHousing = (c: Ctx2D, w: number, h: number): void =>
    drawOuterHousing(c, w, h, this.cells, this.housingFill);

  constructor(options: VoxelLogoRendererOptions) {
    this.grids = options.grids;
    this.housing = options.housing;
    this.glow = options.glow;
    this.ripples = options.ripples;
    let largest = 1;
    for (const grid of this.grids) largest = Math.max(largest, grid.size * grid.size);
    this.cellStyle = new Uint8Array(largest);
  }

  /**
   * The grid used for a `width`×`height` canvas: the finest one with a whole-pixel cell pitch of
   * at least 3 px, else of at least 2 px, else the coarsest.
   */
  gridFor(width: number, height: number): LogoGrid {
    const span = Math.min(width, height);
    for (let minPitch = 3; minPitch >= 2; minPitch--) {
      for (let g = 0; g < this.grids.length; g++) {
        if (Math.floor(span / this.grids[g].size) >= minPitch) return this.grids[g];
      }
    }
    return this.grids[this.grids.length - 1];
  }

  render(
    ctx: CanvasRenderingContext2D,
    width: number,
    height: number,
    levels: LogoLevels,
    palette: VoxelLogoPalette,
    reducedMotion: boolean,
  ): void {
    if (!Number.isFinite(width) || !Number.isFinite(height) || width <= 0 || height <= 0) return;
    ctx.clearRect(0, 0, width, height);

    const grid = this.gridFor(width, height);
    const n = grid.size;
    const cells = this.cells;
    const layoutChanged = cells.fit(width, height, n, n);

    // Geometry for this frame, in mark units. Reduced motion holds it still.
    const rimHalf = reducedMotion ? RIM_HALF_STILL : RIM_HALF_MIN + RIM_HALF_RANGE * levels.mid;
    const ringHalf = reducedMotion
      ? RING_HALF_STILL
      : RING_HALF_MIN + RING_HALF_RANGE * levels.treble;
    const haloR = reducedMotion ? HALO_R_STILL : HALO_R_MIN + HALO_R_RANGE * levels.bass;
    const glowR = reducedMotion ? GLOW_R_STILL : GLOW_R_MIN + GLOW_R_RANGE * levels.mid;
    const coreR = Math.max(BURST_R, grid.cellUnits * 0.5);
    const rimStyle = LOGO_STYLE.rim + rung(0.7 + 0.3 * levels.mid);
    const ringStyle = LOGO_STYLE.ring + rung(0.5 + 0.5 * levels.treble);
    const haloOpacity = 0.15 + 0.7 * levels.bass;
    const glowOpacity = (0.2 + 0.7 * levels.mid) * GLOW_PEAK;
    const ripples = this.ripples && !reducedMotion;
    const shimmer = !reducedMotion && levels.treble > 0.2;
    const rippleHalf = Math.max(RIPPLE_HALF, grid.cellUnits * 0.5);

    // 1. Classify every cell, topmost layer first (the SVG's paint order reversed).
    const count = n * n;
    const styleOf = this.cellStyle;
    for (let i = 0; i < count; i++) {
      const d = grid.markDistance[i];
      const db = grid.burstDistance[i];
      const rest = grid.restLayer[i];
      let style: number = LOGO_STYLE.none;
      if (db <= coreR) {
        style = LOGO_STYLE.burst + LADDER - 1;
      } else if (db <= haloR) {
        const k = rung(haloOpacity * (1 - (db - BURST_R) / (haloR - BURST_R)));
        if (k > 0) style = LOGO_STYLE.burst + k;
      }
      if (style === LOGO_STYLE.none) {
        if (rest === LOGO_REST.disc) style = LOGO_STYLE.disc;
        else if (rest === LOGO_REST.ring || Math.abs(d - RING_R) <= ringHalf) {
          style = shimmer
            ? LOGO_STYLE.ring +
              rung(
                0.5 +
                  0.5 * levels.treble +
                  0.22 * Math.sin(grid.cellAngle[i] * 6 + levels.shimmerPhase),
              )
            : ringStyle;
        } else if (rest === LOGO_REST.rim || Math.abs(d - RIM_R) <= rimHalf) style = rimStyle;
        // Ripples are drawn in the ring's colour, over the glow and housing.
        if (style === LOGO_STYLE.none && ripples) style = rippleStyle(levels, d, rippleHalf);
        if (style === LOGO_STYLE.none && this.glow && d <= glowR && glowR > RING_OUTER) {
          const k = rung(glowOpacity * (1 - (d - RING_OUTER) / (glowR - RING_OUTER)));
          if (k > 0) style = LOGO_STYLE.glow + k;
        }
        if (style === LOGO_STYLE.none && this.housing) style = LOGO_STYLE.housing;
      }
      styleOf[i] = style;
    }

    // 2. Housing cells around the grid, aligned to its pitch (cached offscreen across frames).
    if (this.housing && cells.pitch >= MIN_HOUSING_PITCH) {
      const fill = palette.styles[LOGO_STYLE.housing];
      this.housingFill = fill;
      this.housingLayer.blit(ctx, width, height, layoutChanged, this.paintHousing, fill);
    }

    // 3. The grid's cells, one fillStyle per style.
    this.painter.paint(ctx, cells, styleOf, palette.styles);

    // 4. Ripples carry on across the housing around the grid, out to the canvas edges (at the
    // same pitches the housing is drawn at).
    if (ripples && cells.pitch >= MIN_HOUSING_PITCH && levels.rippling) {
      drawOuterRipples(ctx, width, height, cells, grid, levels, rippleHalf, palette.styles);
    }
  }
}
