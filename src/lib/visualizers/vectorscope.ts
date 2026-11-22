import { i18n } from "../stores/i18n.svelte";
import type { ThemeColors } from "../stores/theme.svelte";
import { clampForContrast } from "../utils/colorUtils";
import type {
  VisualizerFrame,
  VisualizerInstance,
  VisualizerPreset,
  VisualizerRenderOptions,
  VisualizerSize,
} from "./types";
import {
  HISTORY_LENGTH,
  ScopeMeters,
  ScopeTrace,
  formatBalance,
  formatCorrelation,
  formatLevel,
  type ScopeLabels,
} from "./vectorscopeEngine";

/** How fast the phosphor afterglow fades: about 1/e of its brightness every this many ms. */
const GLOW_TAU_MS = 90;
/** After this long without new points the glow has faded out and drawing can stop. */
const GLOW_LIFETIME_MS = GLOW_TAU_MS * 6;
/** Scope frames (~30 a second) between readout refreshes: ~10 Hz, ~4 Hz under reduced motion. */
const READOUT_EVERY = 3;
const READOUT_EVERY_REDUCED = 8;
/** The history strip's vertical range, in LUFS. */
const HISTORY_TOP_LUFS = 0;
const HISTORY_BOTTOM_LUFS = -48;
/** Reference line on the history strip: the common streaming loudness target. */
const HISTORY_REFERENCE_LUFS = -14;
/** A negative correlation reads in this red (contrast-checked against the surface). */
const OUT_OF_PHASE_RED = "#ef4444";
/**
 * Guides (graticule, empty bar track, strip frame) sit well behind the data: themes pick
 * `color-border` to be seen, and at full strength it outshines a quiet trace.
 */
const GUIDE_ALPHA = 0.45;
const FONT_STACK = '"Fira Sans", system-ui, sans-serif';
const LABEL_FONT = `500 11px ${FONT_STACK}`;
const AXIS_FONT = `600 12px ${FONT_STACK}`;
const VALUE_FONT = `600 20px ${FONT_STACK}`;
const NO_VALUE = "–";

interface ScopePalette {
  trace: string;
  text: string;
  muted: string;
  grid: string;
  warn: string;
}

function buildPalette(colors: ThemeColors, surface: "bg-main" | "bg-playerbar"): ScopePalette {
  const bg = colors[surface] || colors["bg-main"] || "#191918";
  return {
    trace: clampForContrast(colors["color-accent"] || "#8b5cf6", bg, 3.0),
    text: colors["color-text-primary"] || "#f5f5f4",
    muted: clampForContrast(colors["color-text-secondary"] || "#a8a29e", bg, 4.5),
    grid: colors["color-border"] || "#3f3f46",
    warn: clampForContrast(OUT_OF_PHASE_RED, bg, 3.0),
  };
}

/**
 * A still, plausible trace for previews: a mostly-mono mix with some width, as most
 * music looks. Built once; previews never change.
 */
const PREVIEW_TRACE = (() => {
  const trace = new ScopeTrace();
  const pairs: number[] = [];
  for (let i = 0; i < 512; i++) {
    const t = (i / 512) * Math.PI * 14;
    const mid = 0.55 * Math.sin(t) + 0.2 * Math.sin(2.7 * t);
    const side = 0.14 * Math.sin(1.9 * t + 0.6) + 0.06 * Math.sin(5.3 * t);
    pairs.push(mid - side, mid + side);
  }
  trace.set(pairs);
  return trace;
})();

/** `value` followed by `unit`, e.g. "−14.2 LUFS"; the bare dash when there's no level yet. */
function withUnit(value: string, unit: string): string {
  return value === NO_VALUE ? value : `${value} ${unit}`;
}

/** The y of `lufs` on a history strip spanning `top`..`top + height`. */
function historyY(lufs: number, top: number, height: number): number {
  const t = (lufs - HISTORY_BOTTOM_LUFS) / (HISTORY_TOP_LUFS - HISTORY_BOTTOM_LUFS);
  return top + height - Math.max(0, Math.min(1, t)) * height;
}

/**
 * Strokes `trace` as one smooth path centred on (cx, cy), unit circle → `radius` px. An
 * analogue scope's beam follows the continuous signal, so rather than joining the thinned
 * samples with straight segments (sharp corners), the path curves through the midpoint of
 * each pair of neighbours, using each sample as the control point.
 */
function strokeTrace(
  ctx: CanvasRenderingContext2D,
  trace: ScopeTrace,
  cx: number,
  cy: number,
  radius: number,
): void {
  const n = trace.count;
  if (n === 0) return;
  const p = trace.points;
  ctx.beginPath();
  ctx.moveTo(cx + p[0] * radius, cy - p[1] * radius);
  for (let i = 1; i < n - 1; i++) {
    const x = cx + p[2 * i] * radius;
    const y = cy - p[2 * i + 1] * radius;
    const nextX = cx + p[2 * i + 2] * radius;
    const nextY = cy - p[2 * i + 3] * radius;
    ctx.quadraticCurveTo(x, y, (x + nextX) / 2, (y + nextY) / 2);
  }
  if (n > 1) ctx.lineTo(cx + p[2 * n - 2] * radius, cy - p[2 * n - 1] * radius);
  ctx.stroke();
}

/** One readout under the scope: a small label over a large value. */
interface ReadoutCell {
  label: string;
  value: string;
  warn: boolean;
}

function readoutCells(): ReadoutCell[] {
  return Array.from({ length: 4 }, () => ({ label: "", value: NO_VALUE, warn: false }));
}

interface CachedLabels extends ScopeLabels {
  locale: string;
  momentary: string;
  shortTerm: string;
  truePeak: string;
  balanceTitle: string;
  correlationTitle: string;
}

function readLabels(): CachedLabels {
  return {
    locale: i18n.currentLocale,
    left: i18n.t("visualizers.scope.left"),
    right: i18n.t("visualizers.scope.right"),
    centre: i18n.t("visualizers.scope.centre"),
    momentary: i18n.t("visualizers.scope.momentary"),
    shortTerm: i18n.t("visualizers.scope.shortTerm"),
    truePeak: i18n.t("visualizers.scope.truePeak"),
    balanceTitle: i18n.t("visualizers.scope.balance"),
    correlationTitle: i18n.t("visualizers.scope.correlation"),
  };
}

/**
 * The immersive stage: the goniometer with phosphor afterglow, a correlation bar under it,
 * a row of loudness readouts, and a scrolling short-term loudness strip along the bottom.
 */
class VectorscopeFullInstance implements VisualizerInstance {
  private readonly meters = new ScopeMeters();
  private readonly trace = new ScopeTrace();
  private palette!: ScopePalette;
  private labels!: CachedLabels;

  // Text is rebuilt at the readout rate (and on a language change), never per draw.
  private readonly cells = readoutCells();
  private readonly previewCells = readoutCells();
  private correlationText = "";
  /** Starts past any threshold so the first frame of data refreshes the readouts at once. */
  private framesSinceReadout = Number.MAX_SAFE_INTEGER;
  private reducedMotion = false;
  /** New points not yet drawn into the glow layer. */
  private tracePending = false;
  /** Fraction of the glow layer still to fade away at the next draw. */
  private fadePending = 0;
  private msSinceTrace = Number.MAX_SAFE_INTEGER;
  // The afterglow layer: the scope square only, in device pixels.
  private glow: HTMLCanvasElement | null = null;
  private glowCtx: CanvasRenderingContext2D | null = null;
  private glowSize = 0;

  constructor() {
    this.applyLabels();
  }

  setTheme(colors: ThemeColors): void {
    this.palette = buildPalette(colors, "bg-main");
    this.clearGlow();
  }

  push(frame: VisualizerFrame): void {
    const scope = frame.scope;
    if (!scope) {
      // Paused: the afterglow layer keeps its own pixels and fades them out; without it
      // the last trace would stay frozen on screen.
      if (!frame.playing) this.trace.clear();
      return;
    }
    this.meters.push(scope);
    this.trace.set(scope.points);
    this.tracePending = true;
    this.msSinceTrace = 0;
    this.framesSinceReadout++;
    if (this.framesSinceReadout >= (this.reducedMotion ? READOUT_EVERY_REDUCED : READOUT_EVERY)) {
      this.framesSinceReadout = 0;
      this.refreshReadouts();
    }
  }

  step(dtMs: number, reducedMotion: boolean): boolean {
    this.reducedMotion = reducedMotion;
    if (reducedMotion) return false;
    this.msSinceTrace += dtMs;
    this.fadePending = 1 - (1 - this.fadePending) * Math.exp(-dtMs / GLOW_TAU_MS);
    return this.msSinceTrace < GLOW_LIFETIME_MS;
  }

  /** Read the labels in the current language into every cell, then refresh the values. */
  private applyLabels(): void {
    const l = (this.labels = readLabels());
    for (const cells of [this.cells, this.previewCells]) {
      cells[0].label = l.momentary;
      cells[1].label = l.shortTerm;
      cells[2].label = l.truePeak;
      cells[3].label = l.balanceTitle;
    }
    this.previewCells[3].value = l.centre;
    this.refreshReadouts();
  }

  /** Rebuild the readout values from the meters. */
  private refreshReadouts(): void {
    const m = this.meters;
    const [momentary, shortTerm, peak, balance] = this.cells;
    momentary.value = withUnit(formatLevel(m.momentaryLufs), "LUFS");
    shortTerm.value = withUnit(formatLevel(m.shortTermLufs), "LUFS");
    peak.value = withUnit(formatLevel(m.peakHoldDb), "dBTP");
    peak.warn = m.peakHoldDb > 0;
    balance.value = formatBalance(m.balanceDb, this.labels);
    this.correlationText = `${this.labels.correlationTitle} ${formatCorrelation(m.correlation)}`;
  }

  private clearGlow(): void {
    if (this.glowCtx && this.glow) this.glowCtx.clearRect(0, 0, this.glow.width, this.glow.height);
  }

  /** The glow layer sized for a `size` px scope, or null where canvases can't be made. */
  private glowLayer(size: number): CanvasRenderingContext2D | null {
    const dpr = (typeof window !== "undefined" && window.devicePixelRatio) || 1;
    const px = Math.max(1, Math.round(size * dpr));
    if (!this.glow && typeof document !== "undefined") {
      this.glow = document.createElement("canvas");
      this.glowCtx = this.glow.getContext("2d");
    }
    if (!this.glow || !this.glowCtx) return null;
    if (this.glow.width !== px || this.glowSize !== size) {
      this.glow.width = px;
      this.glow.height = px;
      this.glowSize = size;
      this.glowCtx.setTransform(dpr, 0, 0, dpr, 0, 0);
    }
    return this.glowCtx;
  }

  render(
    ctx: CanvasRenderingContext2D,
    width: number,
    height: number,
    options: VisualizerRenderOptions,
  ): void {
    if (width <= 0 || height <= 0) return;
    ctx.clearRect(0, 0, width, height);
    if (this.labels.locale !== i18n.currentLocale) this.applyLabels();

    const pad = Math.max(12, Math.min(32, Math.min(width, height) * 0.04));
    const gap = Math.max(8, pad * 0.6);
    const historyH = Math.max(24, Math.min(56, height * 0.08));
    const readoutsH = 46;
    const corrH = 24;
    const scopeSize = Math.max(
      40,
      Math.min(width - 2 * pad, height - 2 * pad - historyH - readoutsH - corrH - 3 * gap),
    );
    const scopeX = (width - scopeSize) / 2;
    const scopeY = pad;
    const radius = (scopeSize / 2) * 0.94;
    const cx = scopeX + scopeSize / 2;
    const cy = scopeY + scopeSize / 2;

    this.drawGraticule(ctx, cx, cy, radius);

    if (options.preview) {
      ctx.strokeStyle = this.palette.trace;
      ctx.lineWidth = 1.2;
      ctx.globalAlpha = 0.8;
      strokeTrace(ctx, PREVIEW_TRACE, cx, cy, radius);
      ctx.globalAlpha = 1;
    } else {
      this.drawTrace(ctx, scopeX, scopeY, scopeSize, radius, options.reducedMotion);
    }

    const corrY = scopeY + scopeSize + gap;
    this.drawCorrelation(ctx, cx - scopeSize / 2, corrY, scopeSize, options.preview);

    const readoutsY = corrY + corrH + gap;
    const rowWidth = Math.min(width - 2 * pad, Math.max(scopeSize, 520));
    this.drawReadouts(ctx, (width - rowWidth) / 2, readoutsY, rowWidth, options.preview);

    this.drawHistory(ctx, pad, height - pad - historyH, width - 2 * pad, historyH);
  }

  private drawGraticule(ctx: CanvasRenderingContext2D, cx: number, cy: number, r: number): void {
    const p = this.palette;
    ctx.strokeStyle = p.grid;
    ctx.lineWidth = 1;
    ctx.globalAlpha = GUIDE_ALPHA;
    ctx.beginPath();
    ctx.arc(cx, cy, r, 0, Math.PI * 2);
    // Mid (vertical) and side (horizontal) axes.
    ctx.moveTo(cx, cy - r);
    ctx.lineTo(cx, cy + r);
    ctx.moveTo(cx - r, cy);
    ctx.lineTo(cx + r, cy);
    ctx.stroke();
    // L and R diagonals, fainter: where a hard-panned signal lies.
    const d = r * Math.SQRT1_2;
    ctx.globalAlpha = GUIDE_ALPHA / 2;
    ctx.beginPath();
    ctx.moveTo(cx - d, cy - d);
    ctx.lineTo(cx + d, cy + d);
    ctx.moveTo(cx + d, cy - d);
    ctx.lineTo(cx - d, cy + d);
    ctx.stroke();
    ctx.globalAlpha = 1;
    ctx.fillStyle = p.muted;
    ctx.font = AXIS_FONT;
    ctx.textBaseline = "middle";
    ctx.textAlign = "right";
    ctx.fillText(this.labels.left, cx - d - 6, cy - d - 6);
    ctx.textAlign = "left";
    ctx.fillText(this.labels.right, cx + d + 6, cy - d - 6);
  }

  private drawTrace(
    ctx: CanvasRenderingContext2D,
    x: number,
    y: number,
    size: number,
    radius: number,
    reducedMotion: boolean,
  ): void {
    const glow = reducedMotion ? null : this.glowLayer(size);
    if (!glow || !this.glow) {
      // No afterglow: just the latest frame's points (none once paused).
      ctx.strokeStyle = this.palette.trace;
      ctx.lineWidth = 1.2;
      ctx.globalAlpha = 0.85;
      strokeTrace(ctx, this.trace, x + size / 2, y + size / 2, radius);
      ctx.globalAlpha = 1;
      return;
    }
    if (this.fadePending > 0) {
      glow.globalCompositeOperation = "destination-out";
      glow.globalAlpha = Math.min(1, this.fadePending);
      glow.fillStyle = "#000";
      glow.fillRect(0, 0, size, size);
      glow.globalCompositeOperation = "source-over";
      this.fadePending = 0;
    }
    if (this.tracePending) {
      this.tracePending = false;
      glow.globalAlpha = 0.75;
      glow.strokeStyle = this.palette.trace;
      glow.lineWidth = 1.2;
      strokeTrace(glow, this.trace, size / 2, size / 2, radius);
    }
    glow.globalAlpha = 1;
    ctx.drawImage(this.glow, x, y, size, size);
  }

  private drawCorrelation(
    ctx: CanvasRenderingContext2D,
    x: number,
    y: number,
    w: number,
    preview: boolean,
  ): void {
    const p = this.palette;
    const value = preview ? 0.7 : this.meters.correlation;
    const barH = 6;
    const mid = x + w / 2;
    ctx.fillStyle = p.grid;
    ctx.globalAlpha = GUIDE_ALPHA;
    ctx.fillRect(x, y, w, barH);
    ctx.globalAlpha = 1;
    const end = mid + (w / 2) * Math.max(-1, Math.min(1, value));
    ctx.fillStyle = value < 0 ? p.warn : p.trace;
    ctx.fillRect(Math.min(mid, end), y, Math.abs(end - mid), barH);
    ctx.fillStyle = p.text;
    ctx.fillRect(mid - 0.5, y - 2, 1, barH + 4);

    ctx.font = LABEL_FONT;
    ctx.textBaseline = "top";
    ctx.fillStyle = p.muted;
    ctx.textAlign = "left";
    ctx.fillText("−1", x, y + barH + 4);
    ctx.textAlign = "right";
    ctx.fillText("+1", x + w, y + barH + 4);
    ctx.textAlign = "center";
    ctx.fillStyle = value < 0 ? p.warn : p.muted;
    ctx.fillText(preview ? this.labels.correlationTitle : this.correlationText, mid, y + barH + 4);
  }

  private drawReadouts(
    ctx: CanvasRenderingContext2D,
    x: number,
    y: number,
    w: number,
    preview: boolean,
  ): void {
    const p = this.palette;
    const cells = preview ? this.previewCells : this.cells;
    const cell = w / cells.length;
    ctx.textAlign = "center";
    ctx.textBaseline = "top";
    for (let i = 0; i < cells.length; i++) {
      const c = cells[i];
      const cx = x + cell * (i + 0.5);
      ctx.font = LABEL_FONT;
      ctx.fillStyle = p.muted;
      ctx.fillText(c.label, cx, y, cell - 8);
      ctx.font = VALUE_FONT;
      ctx.fillStyle = c.warn ? p.warn : p.text;
      ctx.fillText(c.value, cx, y + 17, cell - 8);
    }
  }

  private drawHistory(ctx: CanvasRenderingContext2D, x: number, y: number, w: number, h: number) {
    const p = this.palette;
    ctx.strokeStyle = p.grid;
    ctx.lineWidth = 1;
    ctx.globalAlpha = GUIDE_ALPHA;
    ctx.strokeRect(x + 0.5, y + 0.5, w - 1, h - 1);
    ctx.setLineDash([3, 3]);
    ctx.beginPath();
    const refY = Math.round(historyY(HISTORY_REFERENCE_LUFS, y, h)) + 0.5;
    ctx.moveTo(x, refY);
    ctx.lineTo(x + w, refY);
    ctx.stroke();
    ctx.setLineDash([]);
    ctx.globalAlpha = 1;

    const m = this.meters;
    if (m.historyCount < 2) return;
    // Newest at the right edge, older samples scrolling left.
    const step = w / (HISTORY_LENGTH - 1);
    ctx.beginPath();
    ctx.moveTo(x + w, y + h);
    for (let age = 0; age < m.historyCount; age++) {
      ctx.lineTo(x + w - age * step, historyY(m.historyAgo(age), y, h));
    }
    ctx.lineTo(x + w - (m.historyCount - 1) * step, y + h);
    ctx.closePath();
    ctx.fillStyle = p.trace;
    ctx.globalAlpha = 0.25;
    ctx.fill();
    ctx.globalAlpha = 0.9;
    ctx.strokeStyle = p.trace;
    ctx.lineWidth = 1.2;
    ctx.beginPath();
    for (let age = 0; age < m.historyCount; age++) {
      const px = x + w - age * step;
      const py = historyY(m.historyAgo(age), y, h);
      if (age === 0) ctx.moveTo(px, py);
      else ctx.lineTo(px, py);
    }
    ctx.stroke();
    ctx.globalAlpha = 1;
  }
}

/**
 * The playbar slot: the latest trace in a small square and a correlation tick along the
 * bottom, drawn once per frame of data with no afterglow.
 */
class VectorscopeMiniInstance implements VisualizerInstance {
  private readonly trace = new ScopeTrace();
  private correlation = 0;
  private palette!: ScopePalette;

  setTheme(colors: ThemeColors): void {
    this.palette = buildPalette(colors, "bg-playerbar");
  }

  push(frame: VisualizerFrame): void {
    if (frame.scope) {
      this.trace.set(frame.scope.points);
      this.correlation = frame.scope.correlation;
    } else if (!frame.playing) {
      this.trace.clear();
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
    if (width <= 0 || height <= 0) return;
    ctx.clearRect(0, 0, width, height);
    const p = this.palette;
    const tickH = 2;
    const size = Math.max(4, height - tickH - 2);
    const cx = width / 2;
    const cy = size / 2;
    const radius = size / 2;

    ctx.strokeStyle = p.grid;
    ctx.lineWidth = 1;
    ctx.globalAlpha = GUIDE_ALPHA;
    ctx.beginPath();
    ctx.moveTo(cx, 0);
    ctx.lineTo(cx, size);
    ctx.stroke();

    ctx.strokeStyle = p.trace;
    ctx.lineWidth = 1;
    ctx.globalAlpha = 0.9;
    strokeTrace(ctx, options.preview ? PREVIEW_TRACE : this.trace, cx, cy, radius);
    ctx.globalAlpha = 1;

    const value = options.preview ? 0.7 : this.correlation;
    const y = height - tickH;
    ctx.fillStyle = p.grid;
    ctx.globalAlpha = GUIDE_ALPHA;
    ctx.fillRect(0, y, width, tickH);
    ctx.globalAlpha = 1;
    ctx.fillStyle = value < 0 ? p.warn : p.trace;
    const tickX = cx + (width / 2 - 2) * Math.max(-1, Math.min(1, value));
    ctx.fillRect(tickX - 1.5, y - 1, 3, tickH + 1);
  }
}

/** A stereo vectorscope (goniometer) with correlation, loudness and true-peak meters. */
export const VECTORSCOPE_PRESET: VisualizerPreset = {
  id: "vectorscope",
  labelKey: "visualizers.vectorscope",
  needs: ["scope"],
  create(size: VisualizerSize): VisualizerInstance {
    return size === "full" ? new VectorscopeFullInstance() : new VectorscopeMiniInstance();
  },
};
