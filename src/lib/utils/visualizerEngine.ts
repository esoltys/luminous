import type { ThemeColors } from "../stores/theme.svelte";
import { clampForContrast, hexToRgb, isLightColor, rgbToHex } from "./colorUtils";

export interface VoxelColumnState {
  /** Current smoothed bar height in [0, 1]. */
  level: number;
  /** Quantized active voxel row count in [0, rows]. */
  activeCells: number;
  /** Peak-hold cap position in [0, 1]. */
  peakLevel: number;
  /** Quantized peak-hold voxel row index in [0, rows] (0 means resting/unlit). */
  peakCell: number;
  /** Remaining hold time in milliseconds before gravity begins pulling the peak cap down. */
  peakHoldMs: number;
  /** Current downward velocity of the peak cap (normalized units per second). */
  peakVelocity: number;
}

export interface VoxelThemePalette {
  isLight: boolean;
  /** Pre-computed fill color for each voxel row from bottom (0) to top (rowCount - 1). */
  rowColors: readonly string[];
  /** High-contrast fill color for the pushed-up peak-hold cap block. */
  peakCapColor: string;
  /** Subtle accent fill for the unlit voxel matrix cells. */
  unlitCellColor: string;
  /** Horizon separator line between the active matrix and the reflection floor. */
  baselineColor: string;
  /** Pre-computed decaying alpha colors for the mirrored reflection rows below the baseline. */
  reflectionRowColors: readonly string[];
}

export interface VisualizerRenderOptions {
  reducedMotion?: boolean;
}

export interface VisualizerPreset {
  id: string;
  labelKey: string;
  render(
    ctx: CanvasRenderingContext2D,
    width: number,
    height: number,
    engine: VoxelSpectrumEngine,
    palette: VoxelThemePalette,
    options?: VisualizerRenderOptions,
  ): void;
}

const DEFAULT_COLUMNS = 64;
const DEFAULT_ROWS = 28;
const PEAK_HOLD_MS = 340;
const PEAK_HOLD_REDUCED_MS = 480;
const PEAK_GRAVITY = 1.45;
const PEAK_GRAVITY_REDUCED = 0.65;
const ATTACK_RATE = 38;
const DECAY_RATE = 10.5;
const DECAY_RATE_REDUCED = 6.5;
const EPSILON = 0.002;
const REFLECTION_ROWS = 4;

function mixRgb(
  a: { r: number; g: number; b: number },
  b: { r: number; g: number; b: number },
  t: number,
): { r: number; g: number; b: number } {
  const clamped = Math.max(0, Math.min(1, t));
  return {
    r: Math.round(a.r + (b.r - a.r) * clamped),
    g: Math.round(a.g + (b.g - a.g) * clamped),
    b: Math.round(a.b + (b.b - a.b) * clamped),
  };
}

/**
 * Builds a pre-computed, theme-reactive color palette for the voxel visualizer
 * from `themeStore.resolvedColors`. Pre-calculating every row's CSS color string
 * when the theme changes avoids parsing hex or allocating color strings inside
 * the 60 FPS canvas render loop.
 */
export function buildVoxelThemePalette(
  colors: ThemeColors,
  rowCount = DEFAULT_ROWS,
): VoxelThemePalette {
  const bgMain = colors["bg-main"] || "#191918";
  const isLight = isLightColor(bgMain);

  const safeAccentHex = clampForContrast(colors["color-accent"] || "#4f5bd5", bgMain, 3.0);
  const safeHoverHex = clampForContrast(colors["color-accent-hover"] || safeAccentHex, bgMain, 3.0);
  const textPrimaryHex = colors["color-text-primary"] || (isLight ? "#16181d" : "#f1f3f8");
  const borderHex = colors["color-border"] || safeAccentHex;

  const accentRgb = hexToRgb(safeAccentHex);
  const hoverRgb = hexToRgb(safeHoverHex);
  const textPrimaryRgb = hexToRgb(textPrimaryHex);
  const bgRgb = hexToRgb(bgMain);
  const borderRgb = hexToRgb(borderHex);

  // Base rows start slightly deeper toward the canvas polarity so the vertical
  // gradient has depth even when accent and accent-hover are close in hue.
  const baseAnchorRgb = mixRgb(accentRgb, bgRgb, isLight ? 0.15 : 0.22);
  // Crest rows blend toward primary text so high-energy peaks glow/pop with
  // theme-appropriate polarity (luminous on dark themes, crisp ink on light themes).
  const crestAnchorRgb = mixRgb(hoverRgb, textPrimaryRgb, 0.35);

  const rowColors: string[] = new Array(rowCount);
  for (let row = 0; row < rowCount; row++) {
    const t = rowCount <= 1 ? 1 : row / (rowCount - 1);
    let rgb: { r: number; g: number; b: number };
    if (t < 0.55) {
      rgb = mixRgb(baseAnchorRgb, accentRgb, t / 0.55);
    } else if (t < 0.85) {
      rgb = mixRgb(accentRgb, hoverRgb, (t - 0.55) / 0.3);
    } else {
      rgb = mixRgb(hoverRgb, crestAnchorRgb, (t - 0.85) / 0.15);
    }
    rowColors[row] = rgbToHex(rgb.r, rgb.g, rgb.b);
  }

  const rawCapRgb = mixRgb(textPrimaryRgb, hoverRgb, 0.28);
  const peakCapColor = clampForContrast(
    rgbToHex(rawCapRgb.r, rawCapRgb.g, rawCapRgb.b),
    bgMain,
    4.5,
  );

  const unlitAlpha = isLight ? 0.13 : 0.1;
  const unlitCellColor = `rgba(${borderRgb.r}, ${borderRgb.g}, ${borderRgb.b}, ${unlitAlpha})`;
  const baselineAlpha = isLight ? 0.32 : 0.26;
  const baselineColor = `rgba(${borderRgb.r}, ${borderRgb.g}, ${borderRgb.b}, ${baselineAlpha})`;

  const reflectionAlphas = isLight ? [0.22, 0.14, 0.08, 0.03] : [0.26, 0.16, 0.09, 0.035];
  const reflectionRowColors = reflectionAlphas.map(
    (a) => `rgba(${accentRgb.r}, ${accentRgb.g}, ${accentRgb.b}, ${a})`,
  );

  return {
    isLight,
    rowColors,
    peakCapColor,
    unlitCellColor,
    baselineColor,
    reflectionRowColors,
  };
}

/**
 * Stateful, allocation-free physics stepper for the voxel spectrum visualizer.
 * Upsamples 32 normalized FFT bins into `columnCount` smoothed voxel columns
 * with pushed-up peak-hold blocks, hold timers, and gravity decay.
 */
export class VoxelSpectrumEngine {
  readonly columnCount: number;
  readonly rowCount: number;
  private readonly targetLevels: Float32Array;
  private readonly columnStates: VoxelColumnState[];

  constructor(columnCount = DEFAULT_COLUMNS, rowCount = DEFAULT_ROWS) {
    this.columnCount = Math.max(1, columnCount);
    this.rowCount = Math.max(1, rowCount);
    this.targetLevels = new Float32Array(this.columnCount);
    this.columnStates = Array.from({ length: this.columnCount }, () => ({
      level: 0,
      activeCells: 0,
      peakLevel: 0,
      peakCell: 0,
      peakHoldMs: 0,
      peakVelocity: 0,
    }));
  }

  get columns(): readonly VoxelColumnState[] {
    return this.columnStates;
  }

  /**
   * Maps an incoming normalized spectrum array (typically 32 log-spaced bins
   * in `[0, 1]`) onto `columnCount` target levels using cosine-smoothed
   * interpolation so adjacent columns form a continuous curve rather than
   * duplicated stair-step pairs.
   */
  pushSpectrum(bins: readonly number[]): void {
    const n = bins.length;
    if (n === 0) {
      this.targetLevels.fill(0);
      return;
    }

    const cols = this.columnCount;
    for (let c = 0; c < cols; c++) {
      const pos = cols <= 1 ? 0 : (c / (cols - 1)) * (n - 1);
      const i0 = Math.floor(pos);
      const i1 = Math.min(n - 1, i0 + 1);
      const frac = pos - i0;
      // Cosine interpolation gives smoother peaks and valleys than linear without overshooting `[0, 1]`.
      const mu = (1 - Math.cos(frac * Math.PI)) * 0.5;
      const v0 = Math.max(0, Math.min(1, bins[i0] || 0));
      const v1 = Math.max(0, Math.min(1, bins[i1] || 0));
      const interp = v0 * (1 - mu) + v1 * mu;
      this.targetLevels[c] = interp;
    }
  }

  /**
   * Clears target levels to zero (e.g. when playback pauses or stops) while
   * allowing existing bars and peak-hold blocks to fall naturally to rest via `step()`.
   */
  clearTargets(): void {
    this.targetLevels.fill(0);
  }

  /** Immediately resets all bars and peak-hold blocks to zero. */
  reset(): void {
    this.targetLevels.fill(0);
    for (let i = 0; i < this.columnCount; i++) {
      const col = this.columnStates[i];
      col.level = 0;
      col.activeCells = 0;
      col.peakLevel = 0;
      col.peakCell = 0;
      col.peakHoldMs = 0;
      col.peakVelocity = 0;
    }
  }

  /**
   * Advances bar smoothing and peak-hold gravity by `dtMs` milliseconds.
   * Returns `true` if any column or peak block is still active/moving, or
   * `false` once every column and peak has settled to rest.
   */
  step(dtMs: number, reducedMotion = false): boolean {
    const clampedDtMs = Math.max(0, Math.min(100, dtMs));
    const dtSec = clampedDtMs / 1000;
    const attackAlpha = 1 - Math.exp(-ATTACK_RATE * dtSec);
    const decayRate = reducedMotion ? DECAY_RATE_REDUCED : DECAY_RATE;
    const decayAlpha = 1 - Math.exp(-decayRate * dtSec);
    const holdDurationMs = reducedMotion ? PEAK_HOLD_REDUCED_MS : PEAK_HOLD_MS;
    const gravity = reducedMotion ? PEAK_GRAVITY_REDUCED : PEAK_GRAVITY;
    const rows = this.rowCount;

    let anyActive = false;

    for (let i = 0; i < this.columnCount; i++) {
      const col = this.columnStates[i];
      const target = this.targetLevels[i];

      // 1. Asymmetric attack/decay bar smoothing
      if (target >= col.level) {
        col.level += (target - col.level) * attackAlpha;
      } else {
        col.level += (target - col.level) * decayAlpha;
      }

      if (col.level < EPSILON && target < EPSILON) {
        col.level = 0;
      }

      col.activeCells = Math.min(rows, Math.max(0, Math.round(col.level * rows)));
      const quantizedBarLevel = col.activeCells / rows;
      const effectivePushLevel = Math.max(col.level, quantizedBarLevel);

      // 2. Peak-hold block physics: pushed up by rising spectrum, holds at apex, then falls with gravity
      if (effectivePushLevel >= col.peakLevel && effectivePushLevel > 0) {
        col.peakLevel = effectivePushLevel;
        col.peakHoldMs = holdDurationMs;
        col.peakVelocity = 0;
      } else if (col.peakHoldMs > 0) {
        col.peakHoldMs = Math.max(0, col.peakHoldMs - clampedDtMs);
      } else if (col.peakLevel > effectivePushLevel) {
        col.peakVelocity += gravity * dtSec;
        col.peakLevel = Math.max(effectivePushLevel, col.peakLevel - col.peakVelocity * dtSec);
        if (col.peakLevel <= effectivePushLevel) {
          col.peakLevel = effectivePushLevel;
          col.peakVelocity = 0;
          if (effectivePushLevel > 0) {
            col.peakHoldMs = holdDurationMs * 0.4;
          }
        }
      }

      if (col.peakLevel < EPSILON && col.activeCells === 0) {
        col.peakLevel = 0;
        col.peakVelocity = 0;
        col.peakHoldMs = 0;
        col.peakCell = 0;
      } else {
        const rawPeakCell = Math.min(rows, Math.max(1, Math.round(col.peakLevel * rows)));
        col.peakCell = Math.max(col.activeCells, rawPeakCell);
      }

      if (
        col.activeCells > 0 ||
        col.peakCell > 0 ||
        col.level > EPSILON ||
        col.peakLevel > EPSILON ||
        target > EPSILON
      ) {
        anyActive = true;
      }
    }

    return anyActive;
  }
}

/**
 * Prepares a `<canvas>` for HiDPI 2D rendering at its current CSS layout size,
 * resizing the backing store only when dimensions or `devicePixelRatio` change
 * and resetting the transform matrix idempotently via `setTransform`.
 * Returns `null` if the canvas is detached, zero-sized, or lacks a 2D context.
 */
export function prepareCanvas2d(
  canvas: HTMLCanvasElement | null,
): { ctx: CanvasRenderingContext2D; width: number; height: number } | null {
  if (!canvas) return null;
  const width = canvas.clientWidth;
  const height = canvas.clientHeight;
  if (width <= 0 || height <= 0) return null;
  const ctx = canvas.getContext("2d");
  if (!ctx) return null;

  const dpr = (typeof window !== "undefined" && window.devicePixelRatio) || 1;
  const targetW = Math.round(width * dpr);
  const targetH = Math.round(height * dpr);
  if (canvas.width !== targetW || canvas.height !== targetH) {
    canvas.width = targetW;
    canvas.height = targetH;
  }
  ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
  return { ctx, width, height };
}

/**
 * Renders the voxel LED matrix with pushed-up peak-hold blocks and mirrored
 * floor reflection onto `ctx`. Batches `fillRect` draws by row color so `ctx.fillStyle`
 * changes at most `O(rows)` times per frame rather than `O(columns * rows)`.
 */
export function renderVoxelMatrix(
  ctx: CanvasRenderingContext2D,
  width: number,
  height: number,
  engine: VoxelSpectrumEngine,
  palette: VoxelThemePalette,
  options: VisualizerRenderOptions = {},
): void {
  if (width <= 0 || height <= 0) return;

  ctx.clearRect(0, 0, width, height);

  const cols = engine.columnCount;
  const rows = engine.rowCount;
  const columns = engine.columns;
  const showReflection = !options.reducedMotion;
  const reflectionRows = showReflection ? REFLECTION_ROWS : 0;

  // Vertical layout: main matrix rows + horizon gap + optional reflection strip
  const totalVirtualRows = rows + (reflectionRows > 0 ? reflectionRows + 1.2 : 0);
  const colGap = Math.max(2, Math.min(4, Math.floor(width / (cols * 4.5))));
  const rowGap = Math.max(1.5, Math.min(3, Math.floor(height / (totalVirtualRows * 3.8))));

  const cellWidth = Math.max(2, Math.floor((width - (cols - 1) * colGap) / cols));
  const cellHeight = Math.max(
    2,
    Math.floor((height - (totalVirtualRows - 1) * rowGap) / totalVirtualRows),
  );

  const gridWidth = cols * cellWidth + (cols - 1) * colGap;
  const gridHeight =
    rows * cellHeight +
    (rows - 1) * rowGap +
    (reflectionRows > 0
      ? Math.round(cellHeight * 0.8) + reflectionRows * (cellHeight + rowGap)
      : 0);

  const originX = Math.floor((width - gridWidth) / 2);
  const originY = Math.floor((height - gridHeight) / 2);
  const baselineY = originY + rows * cellHeight + (rows - 1) * rowGap;

  // 1. Unlit matrix grid cells (subtle hardware LED matrix housing)
  ctx.fillStyle = palette.unlitCellColor;
  for (let r = 0; r < rows; r++) {
    const y = baselineY - (r + 1) * cellHeight - r * rowGap;
    for (let c = 0; c < cols; c++) {
      const col = columns[c];
      const rowIndex1 = r + 1;
      if (rowIndex1 <= col.activeCells || col.peakCell === rowIndex1) continue;
      const x = originX + c * (cellWidth + colGap);
      ctx.fillRect(x, y, cellWidth, cellHeight);
    }
  }

  // 2. Lit spectrum voxel cells, batched by row so fillStyle only updates once per row
  for (let r = 0; r < rows; r++) {
    const rowIndex1 = r + 1;
    const y = baselineY - rowIndex1 * cellHeight - r * rowGap;
    ctx.fillStyle = palette.rowColors[r] || palette.rowColors[palette.rowColors.length - 1];

    for (let c = 0; c < cols; c++) {
      const col = columns[c];
      if (col.activeCells < rowIndex1) continue;
      // When the peak cap sits on the top cell of the active bar, step 3 paints it in peakCapColor
      if (col.peakCell === rowIndex1) continue;
      const x = originX + c * (cellWidth + colGap);
      ctx.fillRect(x, y, cellWidth, cellHeight);
    }
  }

  // 3. Pushed-up Peak-Hold blocks (batched in a single fillStyle pass)
  ctx.fillStyle = palette.peakCapColor;
  for (let c = 0; c < cols; c++) {
    const peakCell = columns[c].peakCell;
    if (peakCell <= 0) continue;
    const r = peakCell - 1;
    const x = originX + c * (cellWidth + colGap);
    const y = baselineY - peakCell * cellHeight - r * rowGap;
    ctx.fillRect(x, y, cellWidth, cellHeight);
  }

  // 4. Horizon separator & mirrored floor reflection
  if (reflectionRows > 0) {
    const separatorY = baselineY + Math.max(2, Math.floor(cellHeight * 0.35));
    ctx.fillStyle = palette.baselineColor;
    ctx.fillRect(originX, separatorY, gridWidth, 1);

    const reflectionStartY = separatorY + Math.max(3, Math.floor(cellHeight * 0.45));
    for (let rr = 0; rr < reflectionRows; rr++) {
      ctx.fillStyle =
        palette.reflectionRowColors[rr] ||
        palette.reflectionRowColors[palette.reflectionRowColors.length - 1];
      const y = reflectionStartY + rr * (cellHeight + rowGap);
      const requiredActive = rr + 1;
      for (let c = 0; c < cols; c++) {
        if (columns[c].activeCells < requiredActive) continue;
        const x = originX + c * (cellWidth + colGap);
        ctx.fillRect(x, y, cellWidth, cellHeight);
      }
    }
  }
}

// Static silhouette profile (16 columns, normalized 0..1) for the playbar preview
// when the full immersive visualizer owns the live animation.
const STATIC_PREVIEW_PROFILE = [
  0.25, 0.45, 0.72, 0.55, 0.85, 0.62, 0.4, 0.78, 0.95, 0.68, 0.5, 0.82, 0.6, 0.42, 0.58, 0.3,
] as const;

/**
 * Draws a crisp, zero-animation static voxel-spectrum preview for the miniature
 * playbar visualizer slot while the immersive visualizer is active.
 */
export function renderStaticVoxelPreview(
  ctx: CanvasRenderingContext2D,
  width: number,
  height: number,
  palette: VoxelThemePalette,
): void {
  if (width <= 0 || height <= 0) return;
  ctx.clearRect(0, 0, width, height);

  const cols = STATIC_PREVIEW_PROFILE.length;
  const rows = 6;
  const colGap = 2;
  const rowGap = 1.5;
  const cellWidth = Math.max(2, (width - (cols - 1) * colGap) / cols);
  const cellHeight = Math.max(1.5, (height - (rows - 1) * rowGap) / rows);

  for (let c = 0; c < cols; c++) {
    const level = STATIC_PREVIEW_PROFILE[c];
    const activeCells = Math.max(1, Math.round(level * (rows - 1)));
    const peakCell = Math.min(rows, activeCells + (c % 2 === 0 ? 1 : 0));
    const x = c * (cellWidth + colGap);

    for (let r = 0; r < activeCells; r++) {
      const paletteIdx = Math.min(
        palette.rowColors.length - 1,
        Math.floor((r / Math.max(1, rows - 1)) * (palette.rowColors.length - 1)),
      );
      const y = height - (r + 1) * cellHeight - r * rowGap;
      ctx.fillStyle = palette.rowColors[paletteIdx];
      ctx.fillRect(x, y, cellWidth, cellHeight);
    }

    if (peakCell > activeCells) {
      const r = peakCell - 1;
      const y = height - peakCell * cellHeight - r * rowGap;
      ctx.fillStyle = palette.peakCapColor;
      ctx.fillRect(x, y, cellWidth, cellHeight);
    }
  }
}

export const VOXEL_VISUALIZER_PRESET: VisualizerPreset = {
  id: "voxel",
  labelKey: "playerBar.visualizerTooltip",
  render: renderVoxelMatrix,
};
