import { i18n } from "../stores/i18n.svelte";
import { loudnessStore } from "../stores/loudness.svelte";
import type { ThemeColors } from "../stores/theme.svelte";
import { mixHex } from "../utils/colorUtils";
import type {
  VisualizerFrame,
  VisualizerInstance,
  VisualizerPreset,
  VisualizerRenderOptions,
  VisualizerSize,
} from "./types";
import {
  NEEDLE_MAX,
  SCALE_MARK_VUS,
  VuMeters,
  deflectionForPercent,
  deflectionForVu,
  referenceForLoudness,
} from "./vuMeterEngine";

/** The level, in dBFS per channel, that reads 0 VU right now. */
function currentReferenceDbfs(): number {
  return referenceForLoudness(loudnessStore.enabled, loudnessStore.targetLufs);
}

/** Panel width over height: two wide meters, a peak column between them and a label strip below. */
const PANEL_ASPECT = 3.6;
const FONT_STACK = '"Fira Sans", system-ui, sans-serif';
/** The needle's rest position in a still preview, per channel. */
const PREVIEW_NEEDLES: readonly [number, number] = [0.5, 0.62];

/** The text printed above a labelled mark: "−20" at the left stop, "5+" at the end, else the value. */
function markLabel(vu: number): string {
  if (vu === SCALE_MARK_VUS[0]) return `−${Math.abs(vu)}`;
  if (vu === SCALE_MARK_VUS[SCALE_MARK_VUS.length - 1]) return `${vu}+`;
  return String(Math.abs(vu));
}

/** The unlabelled ticks between the labelled marks, as VU values. */
const MINOR_MARK_VUS = [-15, -9, -8, -6, -4, -2, -1, 1, 2, 4];

/** Every tick on the scale: the labelled marks the engine's scale defines, then the minor ones. */
const SCALE_MARKS: readonly { vu: number; label: string | null }[] = [
  ...SCALE_MARK_VUS.map((vu) => ({ vu, label: markLabel(vu) })),
  ...MINOR_MARK_VUS.map((vu) => ({ vu, label: null })),
];
/** The percentage scale under the VU scale: modulation, where 100% is 0 VU. */
const PERCENT_MARKS = [0, 20, 40, 60, 80, 100] as const;

interface VuPalette {
  /** Window face: pale silver, tinted a little toward the theme accent. */
  face: string;
  ink: string;
  red: string;
  needle: string;
  /** Backlight glow along the top of the window. */
  glow: string;
  frameLight: string;
  frameDark: string;
  lampOff: string;
  lampOn: string;
}

function buildPalette(colors: ThemeColors): VuPalette {
  const accent = colors["color-accent"] || "#8b5cf6";
  return {
    face: mixHex("#d9dbd6", accent, 0.1),
    ink: "#2b2c2a",
    red: "#b8322a",
    needle: "#dc4a2a",
    glow: accent,
    frameLight: "#c9cbc7",
    frameDark: "#8d8f8b",
    lampOff: "#3a3b39",
    lampOn: "#ff5a3c",
  };
}

/** How far under the scale the needle pivots, in half-spans: the larger, the less it tilts. */
const DEPTH_PER_HALF_SPAN = 1.8;

/**
 * One meter's geometry. The scale is printed flat and spread across the whole meter, as on
 * the deck: a mark at needle position `f` (0 = left stop, 1 = end of scale) sits at `x(f)`
 * on the one horizontal line `baseY`, so the left stop block and the red zone are level.
 * The needle pivots far below the window, `depth` under the scale, so it only tilts as it
 * sweeps (about 29 degrees at the ends), and every mark leans along the needle that would
 * point at it: the fan of ticks in the reference.
 */
class Dial {
  readonly depth: number;

  constructor(
    readonly cx: number,
    readonly halfSpan: number,
    readonly baseY: number,
  ) {
    this.depth = halfSpan * DEPTH_PER_HALF_SPAN;
  }

  get pivotY(): number {
    return this.baseY + this.depth;
  }

  x(f: number): number {
    return this.cx + (f - 0.5) * 2 * this.halfSpan;
  }

  /**
   * How far right of its base a point on a mark's own line sits when it is `dy` above the
   * scale line (negative below it). Marks, numbers and percentage dots for one position
   * all lie on that line, so they stay aligned however far the fan leans.
   */
  lean(f: number, dy: number): number {
    return (dy * (this.x(f) - this.cx)) / this.depth;
  }

  /** The needle's tilt from vertical, in radians, when it points at position `f`. */
  angle(f: number): number {
    return Math.atan2(this.x(f) - this.cx, this.depth);
  }
}

/** Everything about the panel's geometry that depends only on its size, in panel-local px. */
interface Layout {
  width: number;
  height: number;
  /** The window the meters sit in. */
  win: { x: number; y: number; w: number; h: number };
  dials: readonly [Dial, Dial];
  /** The y of the row of scale numbers, straight across the meter. */
  numbersY: number;
  /** The peak lamps: centre x, and the y of the +8 and +3 lamps. */
  lamps: { x: number; w: number; h: number; yHigh: number; yLow: number };
  /** Room around the panel for its outer drop shadow. */
  margin: number;
}

function computeLayout(width: number, height: number): Layout {
  const bezel = height * 0.045;
  const win = { x: bezel, y: bezel, w: width - 2 * bezel, h: height * 0.66 };
  const meterW = (win.w - win.w * 0.09) / 2;
  const halfSpan = meterW * 0.37;
  const baseY = win.y + win.h * 0.41;
  return {
    width,
    height,
    win,
    dials: [
      new Dial(win.x + meterW / 2, halfSpan, baseY),
      new Dial(win.x + win.w - meterW / 2, halfSpan, baseY),
    ],
    numbersY: win.y + win.h * 0.18,
    lamps: {
      x: win.x + win.w / 2,
      w: win.w * 0.045,
      h: win.h * 0.055,
      yHigh: win.y + win.h * 0.29,
      yLow: win.y + win.h * 0.51,
    },
    margin: Math.max(12, height * 0.12),
  };
}

function roundRectPath(
  ctx: CanvasRenderingContext2D,
  x: number,
  y: number,
  w: number,
  h: number,
  r: number,
): void {
  ctx.beginPath();
  ctx.roundRect(x, y, w, h, r);
}

/**
 * Fills the stretch of a dial's scale from `f0` to `f1` between `top` and `top + height`.
 * Each end is cut along the line of the tick at that position, continued down through the
 * band, so the band's ends lean exactly as the marks above them do.
 */
function fillBand(
  ctx: CanvasRenderingContext2D,
  dial: Dial,
  f0: number,
  f1: number,
  top: number,
  height: number,
): void {
  const below = dial.baseY - (top + height);
  const above = dial.baseY - top;
  ctx.beginPath();
  ctx.moveTo(dial.x(f0) + dial.lean(f0, above), top);
  ctx.lineTo(dial.x(f1) + dial.lean(f1, above), top);
  ctx.lineTo(dial.x(f1) + dial.lean(f1, below), top + height);
  ctx.lineTo(dial.x(f0) + dial.lean(f0, below), top + height);
  ctx.closePath();
  ctx.fill();
}

/**
 * The leftmost stop: a solid block as thick as the red zone and level with it, ending at the
 * first tick's base. Both its ends lean along that tick, like the red zone's.
 */
function drawLeftStop(
  ctx: CanvasRenderingContext2D,
  p: VuPalette,
  dial: Dial,
  thickness: number,
): void {
  const x = dial.x(0);
  const lean = dial.lean(0, -thickness);
  ctx.fillStyle = p.ink;
  ctx.beginPath();
  ctx.moveTo(x - thickness, dial.baseY);
  ctx.lineTo(x, dial.baseY);
  ctx.lineTo(x + lean, dial.baseY + thickness);
  ctx.lineTo(x - thickness + lean, dial.baseY + thickness);
  ctx.closePath();
  ctx.fill();
}

/**
 * The percentage scale: a dot per mark on a line under the scale, its number in a row
 * beneath. Each dot and number sits on the line of the VU mark it corresponds to.
 */
function drawPercentScale(
  ctx: CanvasRenderingContext2D,
  p: VuPalette,
  dial: Dial,
  wh: number,
): void {
  const dotY = dial.baseY + wh * 0.1;
  const labelY = dial.baseY + wh * 0.18;
  ctx.fillStyle = p.ink;
  ctx.font = `500 ${wh * 0.065}px ${FONT_STACK}`;
  for (const pct of PERCENT_MARKS) {
    const f = deflectionForPercent(pct);
    ctx.beginPath();
    ctx.arc(
      dial.x(f) + dial.lean(f, dial.baseY - dotY),
      dotY,
      Math.max(1, wh * 0.012),
      0,
      Math.PI * 2,
    );
    ctx.fill();
    ctx.fillText(
      pct === 100 ? "100%" : String(pct),
      dial.x(f) + dial.lean(f, dial.baseY - labelY),
      labelY,
    );
  }
}

/** One meter's static scale: marks, red zone, percentage scale and the "VU" legend. */
function drawScale(
  ctx: CanvasRenderingContext2D,
  p: VuPalette,
  layout: Layout,
  dial: Dial,
  unitLabel: string,
): void {
  const wh = layout.win.h;

  // The red zone: a band from 0 VU to the end of the scale.
  const bandHeight = wh * 0.04;
  ctx.fillStyle = p.red;
  fillBand(ctx, dial, deflectionForVu(0), 1, dial.baseY, bandHeight);

  ctx.strokeStyle = p.ink;
  ctx.fillStyle = p.ink;
  ctx.font = `500 ${wh * 0.13}px ${FONT_STACK}`;
  ctx.textAlign = "center";
  ctx.textBaseline = "middle";
  for (const { vu, label } of SCALE_MARKS) {
    const f = Math.min(1, deflectionForVu(vu));
    const major = label !== null;
    const a = dial.angle(f);
    const length = wh * (major ? 0.14 : 0.1);
    const x = dial.x(f);
    const y = dial.baseY;
    ctx.lineWidth = Math.max(1, wh * (major ? 0.011 : 0.008));
    ctx.beginPath();
    ctx.moveTo(x, y);
    ctx.lineTo(x + Math.sin(a) * length, y - Math.cos(a) * length);
    ctx.stroke();
    // The numbers run in a straight row, each on the line of the tick beneath it.
    if (label) ctx.fillText(label, x + dial.lean(f, y - layout.numbersY), layout.numbersY);
  }
  drawLeftStop(ctx, p, dial, bandHeight);
  drawPercentScale(ctx, p, dial, wh);

  ctx.fillStyle = p.ink;
  ctx.font = `600 ${wh * 0.14}px ${FONT_STACK}`;
  ctx.fillText(unitLabel, dial.cx, layout.win.y + wh * 0.82);
}

/** The text and fixed furniture of the panel; built once per size, theme and language. */
interface PanelLabels {
  locale: string;
  left: string;
  right: string;
  unit: string;
  peak: string;
}

function readLabels(): PanelLabels {
  return {
    locale: i18n.currentLocale,
    left: i18n.t("visualizers.vu.left"),
    right: i18n.t("visualizers.vu.right"),
    unit: i18n.t("visualizers.vu.unit"),
    peak: i18n.t("visualizers.vu.peak"),
  };
}

/** Draws the shadowed, recessed layers that never change between frames. */
function drawBase(
  ctx: CanvasRenderingContext2D,
  p: VuPalette,
  layout: Layout,
  labels: PanelLabels,
): void {
  const { width, height, win } = layout;
  const corner = height * 0.035;

  // Faceplate, with a soft drop shadow onto the stage behind it.
  ctx.save();
  ctx.shadowColor = "rgba(0, 0, 0, 0.55)";
  ctx.shadowBlur = layout.margin * 0.8;
  ctx.shadowOffsetY = layout.margin * 0.3;
  const plate = ctx.createLinearGradient(0, 0, 0, height);
  plate.addColorStop(0, p.frameLight);
  plate.addColorStop(1, p.frameDark);
  ctx.fillStyle = plate;
  roundRectPath(ctx, 0, 0, width, height, corner);
  ctx.fill();
  ctx.restore();
  // A fine highlight along the plate's top edge.
  ctx.strokeStyle = "rgba(255, 255, 255, 0.55)";
  ctx.lineWidth = 1;
  roundRectPath(ctx, 0.5, 0.5, width - 1, height - 1, corner);
  ctx.stroke();

  // The window: face colour, backlight glow, then an inset shadow so it reads as recessed.
  ctx.save();
  roundRectPath(ctx, win.x, win.y, win.w, win.h, corner * 0.6);
  ctx.clip();
  ctx.fillStyle = p.face;
  ctx.fillRect(win.x, win.y, win.w, win.h);
  const glow = ctx.createLinearGradient(0, win.y, 0, win.y + win.h * 0.7);
  glow.addColorStop(0, p.glow);
  glow.addColorStop(1, "transparent");
  ctx.globalAlpha = 0.3;
  ctx.fillStyle = glow;
  ctx.fillRect(win.x, win.y, win.w, win.h);
  ctx.globalAlpha = 1;

  for (const dial of layout.dials) drawScale(ctx, p, layout, dial, labels.unit);
  drawLampFurniture(ctx, p, layout, labels);

  // Inset shadow: a frame-shaped slab outside the window casting inward.
  ctx.shadowColor = "rgba(0, 0, 0, 0.6)";
  ctx.shadowBlur = win.h * 0.06;
  ctx.shadowOffsetY = win.h * 0.025;
  ctx.fillStyle = "#000";
  ctx.beginPath();
  ctx.rect(win.x - 80, win.y - 80, win.w + 160, win.h + 160);
  ctx.roundRect(win.x, win.y, win.w, win.h, corner * 0.6);
  ctx.fill("evenodd");
  ctx.restore();

  // The lower strip's LEFT / RIGHT legends.
  ctx.fillStyle = p.ink;
  ctx.font = `600 ${win.h * 0.09}px ${FONT_STACK}`;
  ctx.textAlign = "center";
  ctx.textBaseline = "middle";
  const stripY = win.y + win.h + (height - win.y - win.h) / 2;
  ctx.fillText(labels.left, layout.dials[0].cx, stripY);
  ctx.fillText(labels.right, layout.dials[1].cx, stripY);
}

/** The peak lamps' wells and the PEAK legend; the lit lamps are drawn live. */
function drawLampFurniture(
  ctx: CanvasRenderingContext2D,
  p: VuPalette,
  layout: Layout,
  labels: PanelLabels,
): void {
  const { x, w, h, yHigh, yLow } = layout.lamps;
  const fontPx = layout.win.h * 0.065;
  ctx.fillStyle = p.ink;
  ctx.font = `500 ${fontPx}px ${FONT_STACK}`;
  ctx.textAlign = "center";
  ctx.textBaseline = "middle";
  ctx.fillText("+8 dB", x, yHigh - h * 1.6);
  ctx.fillText("+3 dB", x, yLow - h * 1.6);
  ctx.font = `600 ${layout.win.h * 0.12}px ${FONT_STACK}`;
  ctx.fillText(labels.peak, x, layout.win.y + layout.win.h * 0.82);
  for (const y of [yHigh, yLow]) {
    ctx.fillStyle = p.lampOff;
    roundRectPath(ctx, x - w / 2, y - h / 2, w, h, h * 0.3);
    ctx.fill();
  }
}

/** A faint glass sheen over the whole window, drawn on top of the needles. */
function drawGlass(ctx: CanvasRenderingContext2D, layout: Layout): void {
  const { win, height } = layout;
  const corner = height * 0.035 * 0.6;
  ctx.save();
  roundRectPath(ctx, win.x, win.y, win.w, win.h, corner);
  ctx.clip();
  const sheen = ctx.createLinearGradient(win.x, win.y, win.x + win.w * 0.3, win.y + win.h);
  sheen.addColorStop(0, "rgba(255, 255, 255, 0.28)");
  sheen.addColorStop(0.45, "rgba(255, 255, 255, 0.05)");
  sheen.addColorStop(0.46, "rgba(255, 255, 255, 0)");
  sheen.addColorStop(1, "rgba(255, 255, 255, 0.08)");
  ctx.fillStyle = sheen;
  ctx.fillRect(win.x, win.y, win.w, win.h);
  ctx.restore();
}

/**
 * A needle with its shadow, from the dial's distant pivot up to `tipY`. The light comes
 * from the upper left, so the shadow falls to the lower right on the dial face, further
 * away the further the needle leans that way.
 */
function strokeNeedle(
  ctx: CanvasRenderingContext2D,
  p: VuPalette,
  dial: Dial,
  position: number,
  tipY: number,
  thickness: number,
  lift: number,
): void {
  const a = dial.angle(Math.min(NEEDLE_MAX, position));
  const length = (dial.pivotY - tipY) / Math.cos(a);
  const dx = Math.sin(a);
  const dy = -Math.cos(a);
  const tipX = dial.cx + dx * length;
  const sx = lift * (1.1 + 0.5 * dx);
  const sy = lift * 1.3;
  ctx.lineCap = "round";
  // Two passes of shadow, a wide faint one under a narrower darker one, stand in for blur.
  ctx.strokeStyle = "rgba(0, 0, 0, 0.12)";
  ctx.lineWidth = thickness * 3.2;
  ctx.beginPath();
  ctx.moveTo(dial.cx + sx, dial.pivotY + sy);
  ctx.lineTo(tipX + sx, tipY + sy);
  ctx.stroke();
  ctx.strokeStyle = "rgba(0, 0, 0, 0.22)";
  ctx.lineWidth = thickness * 1.6;
  ctx.beginPath();
  ctx.moveTo(dial.cx + sx * 0.7, dial.pivotY + sy * 0.7);
  ctx.lineTo(tipX + sx * 0.7, tipY + sy * 0.7);
  ctx.stroke();
  ctx.strokeStyle = p.needle;
  ctx.lineWidth = thickness;
  ctx.beginPath();
  ctx.moveTo(dial.cx, dial.pivotY);
  ctx.lineTo(tipX, tipY);
  ctx.stroke();
}

/**
 * What the full and mini meters share: the needles and lamps, the palette, and how scope
 * frames and time drive them. A subclass only decides how to draw them.
 */
abstract class VuMeterBase implements VisualizerInstance {
  protected readonly meters = new VuMeters();
  protected palette!: VuPalette;
  /** Bumped by every theme change, so a subclass's cached drawing knows to be redone. */
  protected themeVersion = 0;

  setTheme(colors: ThemeColors): void {
    this.palette = buildPalette(colors);
    this.themeVersion++;
  }

  push(frame: VisualizerFrame): void {
    if (frame.scope) {
      this.meters.push(frame.scope.points, currentReferenceDbfs());
    } else if (!frame.playing) {
      this.meters.rest();
    }
  }

  step(dtMs: number, reducedMotion: boolean): boolean {
    return this.meters.step(dtMs, reducedMotion);
  }

  /** Where needle `i` (0 = left, 1 = right) is drawn: live, or fixed in a still preview. */
  protected needlePosition(i: 0 | 1, preview: boolean): number {
    if (preview) return PREVIEW_NEEDLES[i];
    return (i === 0 ? this.meters.left : this.meters.right).position;
  }

  abstract render(
    ctx: CanvasRenderingContext2D,
    width: number,
    height: number,
    options: VisualizerRenderOptions,
  ): void;
}

/**
 * The immersive stage: the K-550 meter panel with soft shadows for depth. The panel and its
 * glass sheen are drawn once into offscreen layers, so a frame only blits them and draws the
 * needles and lamps, without allocating.
 */
class VuMeterFullInstance extends VuMeterBase {
  private labels = readLabels();
  private layout: Layout | null = null;
  private base: HTMLCanvasElement | null = null;
  private glass: HTMLCanvasElement | null = null;
  // What the cached layers were drawn for; compared field by field on every frame.
  private cachedWidth = 0;
  private cachedHeight = 0;
  private cachedDpr = 0;
  private cachedTheme = -1;
  private cachedLocale = "";

  /** Rebuild the cached layers if the size, theme, language or pixel ratio changed. */
  private ensureCache(width: number, height: number): Layout | null {
    const dpr = (typeof window !== "undefined" && window.devicePixelRatio) || 1;
    if (this.labels.locale !== i18n.currentLocale) this.labels = readLabels();
    if (
      this.layout &&
      this.base &&
      this.glass &&
      this.cachedWidth === width &&
      this.cachedHeight === height &&
      this.cachedDpr === dpr &&
      this.cachedTheme === this.themeVersion &&
      this.cachedLocale === this.labels.locale
    ) {
      return this.layout;
    }
    if (typeof document === "undefined") return null;

    const pad = Math.max(12, Math.min(32, Math.min(width, height) * 0.04));
    const panelW = Math.max(60, Math.min(width - 2 * pad, (height - 2 * pad) * PANEL_ASPECT));
    const layout = computeLayout(panelW, panelW / PANEL_ASPECT);
    const pxW = Math.max(1, Math.round((layout.width + 2 * layout.margin) * dpr));
    const pxH = Math.max(1, Math.round((layout.height + 2 * layout.margin) * dpr));
    const base = this.base ?? document.createElement("canvas");
    const glass = this.glass ?? document.createElement("canvas");
    for (const canvas of [base, glass]) {
      canvas.width = pxW;
      canvas.height = pxH;
    }
    const baseCtx = base.getContext("2d");
    const glassCtx = glass.getContext("2d");
    if (!baseCtx || !glassCtx) return null;
    for (const c of [baseCtx, glassCtx]) {
      c.setTransform(dpr, 0, 0, dpr, layout.margin * dpr, layout.margin * dpr);
    }
    drawBase(baseCtx, this.palette, layout, this.labels);
    drawGlass(glassCtx, layout);

    this.base = base;
    this.glass = glass;
    this.layout = layout;
    this.cachedWidth = width;
    this.cachedHeight = height;
    this.cachedDpr = dpr;
    this.cachedTheme = this.themeVersion;
    this.cachedLocale = this.labels.locale;
    return layout;
  }

  render(
    ctx: CanvasRenderingContext2D,
    width: number,
    height: number,
    options: VisualizerRenderOptions,
  ): void {
    if (width <= 0 || height <= 0) return;
    ctx.clearRect(0, 0, width, height);
    const layout = this.ensureCache(width, height);
    if (!layout || !this.base || !this.glass) return;
    const p = this.palette;
    const { win, margin } = layout;
    const cacheW = layout.width + 2 * margin;
    const cacheH = layout.height + 2 * margin;

    ctx.save();
    ctx.translate((width - layout.width) / 2, (height - layout.height) / 2);
    ctx.drawImage(this.base, -margin, -margin, cacheW, cacheH);

    // Needles, clipped to the window: they disappear into the lower bezel.
    ctx.save();
    ctx.beginPath();
    ctx.rect(win.x, win.y, win.w, win.h);
    ctx.clip();
    const thickness = Math.max(1.2, win.h * 0.012);
    const lift = win.h * 0.02;
    const tipY = win.y + win.h * 0.19;
    const preview = options.preview;
    strokeNeedle(ctx, p, layout.dials[0], this.needlePosition(0, preview), tipY, thickness, lift);
    strokeNeedle(ctx, p, layout.dials[1], this.needlePosition(1, preview), tipY, thickness, lift);
    ctx.restore();

    if (!preview) {
      this.drawLamp(ctx, layout, this.meters.lampHigh.level, layout.lamps.yHigh);
      this.drawLamp(ctx, layout, this.meters.lampLow.level, layout.lamps.yLow);
    }
    ctx.drawImage(this.glass, -margin, -margin, cacheW, cacheH);
    ctx.restore();
  }

  /** A lit peak lamp at height `y`, glowing and fading with its `level` (0 draws nothing). */
  private drawLamp(ctx: CanvasRenderingContext2D, layout: Layout, level: number, y: number): void {
    if (level <= 0) return;
    const { x, w, h } = layout.lamps;
    ctx.save();
    ctx.shadowColor = this.palette.lampOn;
    ctx.shadowBlur = h * 3 * level;
    ctx.globalAlpha = level;
    ctx.fillStyle = this.palette.lampOn;
    roundRectPath(ctx, x - w / 2, y - h / 2, w, h, h * 0.3);
    ctx.fill();
    ctx.restore();
  }
}

/** The playbar slot: two tiny dials with shadowed needles, no text. */
class VuMeterMiniInstance extends VuMeterBase {
  private dials: readonly [Dial, Dial] | null = null;
  private dialsWidth = 0;
  private dialsHeight = 0;

  private dialsForSize(width: number, height: number): readonly [Dial, Dial] {
    if (!this.dials || this.dialsWidth !== width || this.dialsHeight !== height) {
      const halfSpan = width * 0.21;
      this.dials = [
        new Dial(width * 0.25, halfSpan, height * 0.55),
        new Dial(width * 0.75, halfSpan, height * 0.55),
      ];
      this.dialsWidth = width;
      this.dialsHeight = height;
    }
    return this.dials;
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
    ctx.save();
    roundRectPath(ctx, 0, 0, width, height, 3);
    ctx.clip();
    ctx.fillStyle = p.face;
    ctx.fillRect(0, 0, width, height);
    // A dark top edge and a light bottom edge make the window look recessed.
    ctx.fillStyle = "rgba(0, 0, 0, 0.3)";
    ctx.fillRect(0, 0, width, 2);
    ctx.fillStyle = "rgba(255, 255, 255, 0.35)";
    ctx.fillRect(0, height - 1, width, 1);

    const dials = this.dialsForSize(width, height);
    const band = Math.max(1.5, height * 0.09);
    this.drawDial(ctx, dials[0], 0, band, height, options.preview);
    this.drawDial(ctx, dials[1], 1, band, height, options.preview);
    ctx.restore();
  }

  /** One dial: the scale as a thin guide, the red zone over its end, then the needle. */
  private drawDial(
    ctx: CanvasRenderingContext2D,
    dial: Dial,
    i: 0 | 1,
    band: number,
    height: number,
    preview: boolean,
  ): void {
    const p = this.palette;
    ctx.strokeStyle = p.ink;
    ctx.globalAlpha = 0.45;
    ctx.lineWidth = 1;
    ctx.beginPath();
    ctx.moveTo(dial.x(0), dial.baseY);
    ctx.lineTo(dial.x(1), dial.baseY);
    ctx.stroke();
    ctx.globalAlpha = 1;
    ctx.fillStyle = p.red;
    fillBand(ctx, dial, deflectionForVu(0), 1, dial.baseY - band / 2, band);
    strokeNeedle(ctx, p, dial, this.needlePosition(i, preview), height * 0.3, 1.1, 1);
  }
}

/** A pair of analogue VU meters with peak lamps, after the Yamaha K-550 cassette deck. */
export const VU_METER_PRESET: VisualizerPreset = {
  id: "vu-meter",
  labelKey: "visualizers.vuMeter",
  needs: ["scope"],
  create(size: VisualizerSize): VisualizerInstance {
    // Idempotent: loads the normalization target that places 0 VU.
    void loudnessStore.init();
    return size === "full" ? new VuMeterFullInstance() : new VuMeterMiniInstance();
  },
};
