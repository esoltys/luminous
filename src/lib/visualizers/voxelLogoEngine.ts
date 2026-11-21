import type { ThemeColors } from "../stores/theme.svelte";
import { clampForContrast, hexToRgb } from "../utils/colorUtils";
import type { VisualizerFrame } from "./types";
import { voxelUnlitCellColor } from "./voxelEngine";

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
const GLOW_R_MIN = 95;
const GLOW_R_RANGE = 35;
/** Under reduced motion the geometry holds at mid-range sizes and only brightness follows the music. */
const RIM_HALF_STILL = 9;
const RING_HALF_STILL = 6.5;
const HALO_R_STILL = 31;
const GLOW_R_STILL = 112;
/** The glow is a faint halo: its brightest cell is this fraction of full brightness. */
const GLOW_PEAK = 0.45;

const ATTACK_RATE = 38;
const DECAY_RATE = 10.5;
const DECAY_RATE_REDUCED = 6.5;
const EPSILON = 0.002;

/** Steps in each layer's brightness ladder. */
const LADDER = 10;

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

  constructor(size: number, min: number, max: number) {
    this.size = Math.max(1, Math.floor(size));
    this.cellUnits = (max - min) / this.size;
    const n = this.size * this.size;
    this.markDistance = new Float32Array(n);
    this.burstDistance = new Float32Array(n);
    this.restLayer = new Uint8Array(n);
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
}

function bandAverage(bands: readonly number[], from: number, to: number): number {
  let sum = 0;
  for (let i = from; i < to; i++) sum += bands[i] || 0;
  return sum / (to - from);
}

/**
 * The logo's audio state: bass (burst), mids (rim and glow) and treble (ring), each in [0, 1].
 * `push` sets targets from a frame, using the band split and gains of the reactive logo; a beat
 * onset lifts the burst target, so it flares and then decays with the bass smoothing.
 */
export class LogoLevels {
  bass = 0;
  mid = 0;
  treble = 0;
  private bassTarget = 0;
  private midTarget = 0;
  private trebleTarget = 0;

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
    const bass = Math.min(1, bandAverage(bands, 0, 8) * 1.4);
    this.bassTarget = Math.max(bass, Math.min(1, frame.onset || 0));
    this.midTarget = Math.min(1, bandAverage(bands, 8, 20) * 1.4);
    this.trebleTarget = Math.min(1, bandAverage(bands, 20, 32) * 1.6);
  }

  /** Jumps straight to the targets. */
  settle(): void {
    this.bass = this.bassTarget;
    this.mid = this.midTarget;
    this.treble = this.trebleTarget;
  }

  /**
   * Eases towards the targets: fast attack, slower decay (slower still under reduced motion).
   * Returns false once every level has arrived.
   */
  step(dtMs: number, reducedMotion: boolean): boolean {
    const dtSec = Math.max(0, Math.min(100, dtMs)) / 1000;
    const attack = 1 - Math.exp(-ATTACK_RATE * dtSec);
    const decay = 1 - Math.exp(-(reducedMotion ? DECAY_RATE_REDUCED : DECAY_RATE) * dtSec);
    this.bass = approach(this.bass, this.bassTarget, attack, decay);
    this.mid = approach(this.mid, this.midTarget, attack, decay);
    this.treble = approach(this.treble, this.trebleTarget, attack, decay);
    return (
      this.bass !== this.bassTarget ||
      this.mid !== this.midTarget ||
      this.treble !== this.trebleTarget
    );
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
): VoxelLogoPalette {
  const bg = colors[surface] || colors["bg-main"] || "#191918";
  const accent = hexToRgb(clampForContrast(colors["color-accent"] || "#4f5bd5", bg, 3.0));
  const hover = hexToRgb(
    clampForContrast(colors["color-accent-hover"] || colors["color-accent"] || "#a78bfa", bg, 3.0),
  );
  const burst = hexToRgb(clampForContrast("#ffffff", bg, 3.0));

  const styles: string[] = new Array(LOGO_STYLE.count);
  styles[LOGO_STYLE.none] = "transparent";
  // Only the full view draws housing, on `bg-main`, matching the voxel spectrum's.
  styles[LOGO_STYLE.housing] = voxelUnlitCellColor(colors);
  styles[LOGO_STYLE.disc] = "#0a0a0d";
  const ladder = (base: number, rgb: { r: number; g: number; b: number }) => {
    for (let k = 0; k < LADDER; k++) {
      const alpha = 0.12 + (0.88 * k) / (LADDER - 1);
      styles[base + k] = `rgba(${rgb.r}, ${rgb.g}, ${rgb.b}, ${alpha.toFixed(3)})`;
    }
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
}

/**
 * Draws the mark as an LED grid. Cells are bucketed by fill so `fillStyle` changes once per
 * fill rather than once per cell; the buckets live in buffers sized for the largest grid, so
 * drawing allocates nothing.
 */
export class VoxelLogoRenderer {
  private readonly grids: readonly LogoGrid[];
  private readonly housing: boolean;
  private readonly glow: boolean;
  private readonly cellStyle: Uint8Array;
  private readonly order: Int32Array;
  private readonly bucketStart = new Int32Array(LOGO_STYLE.count + 1);
  private readonly bucketFill = new Int32Array(LOGO_STYLE.count);

  constructor(options: VoxelLogoRendererOptions) {
    this.grids = options.grids;
    this.housing = options.housing;
    this.glow = options.glow;
    let largest = 1;
    for (const grid of this.grids) largest = Math.max(largest, grid.size * grid.size);
    this.cellStyle = new Uint8Array(largest);
    this.order = new Int32Array(largest);
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
    if (width <= 0 || height <= 0) return;
    ctx.clearRect(0, 0, width, height);

    const grid = this.gridFor(width, height);
    const n = grid.size;
    const pitch = Math.max(1, Math.floor(Math.min(width, height) / n));
    const gap = pitch >= 5 ? Math.max(1, Math.round(pitch * 0.18)) : pitch >= 2 ? 1 : 0;
    const cell = pitch - gap;
    const originX = Math.floor((width - n * pitch) / 2);
    const originY = Math.floor((height - n * pitch) / 2);

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

    // 1. Classify every cell, topmost layer first (the SVG's paint order reversed).
    const count = n * n;
    const styleOf = this.cellStyle;
    const starts = this.bucketStart;
    starts.fill(0);
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
        else if (rest === LOGO_REST.ring || Math.abs(d - RING_R) <= ringHalf) style = ringStyle;
        else if (rest === LOGO_REST.rim || Math.abs(d - RIM_R) <= rimHalf) style = rimStyle;
        else if (this.glow && d <= glowR) {
          const k = rung(glowOpacity * (1 - (d - RING_OUTER) / (glowR - RING_OUTER)));
          if (k > 0) style = LOGO_STYLE.glow + k;
        }
        if (style === LOGO_STYLE.none && this.housing) style = LOGO_STYLE.housing;
      }
      styleOf[i] = style;
      starts[style + 1]++;
    }

    // 2. Bucket the cells by style (a counting sort into preallocated buffers).
    for (let s = 0; s < LOGO_STYLE.count; s++) starts[s + 1] += starts[s];
    const fill = this.bucketFill;
    for (let s = 0; s < LOGO_STYLE.count; s++) fill[s] = starts[s];
    for (let i = 0; i < count; i++) this.order[fill[styleOf[i]]++] = i;

    // 3. Housing cells around the grid, aligned to its pitch.
    if (this.housing) {
      ctx.fillStyle = palette.styles[LOGO_STYLE.housing];
      const firstCol = -Math.ceil(originX / pitch);
      const firstRow = -Math.ceil(originY / pitch);
      const lastCol = Math.ceil((width - originX) / pitch);
      const lastRow = Math.ceil((height - originY) / pitch);
      for (let row = firstRow; row < lastRow; row++) {
        const inRows = row >= 0 && row < n;
        const y = originY + row * pitch;
        for (let col = firstCol; col < lastCol; col++) {
          if (inRows && col >= 0 && col < n) continue;
          ctx.fillRect(originX + col * pitch, y, cell, cell);
        }
      }
    }

    // 4. The grid's cells, one fillStyle per bucket.
    for (let s = LOGO_STYLE.none + 1; s < LOGO_STYLE.count; s++) {
      const from = starts[s];
      const to = starts[s + 1];
      if (from === to) continue;
      ctx.fillStyle = palette.styles[s];
      for (let j = from; j < to; j++) {
        const i = this.order[j];
        const row = (i / n) | 0;
        const col = i - row * n;
        ctx.fillRect(originX + col * pitch, originY + row * pitch, cell, cell);
      }
    }
  }
}
