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
  type Needle,
  VuMeters,
  deflectionForVu,
  referenceForTargetLufs,
} from "./vuMeterEngine";

/** Panel width over height: two wide meters, a peak column between them and a label strip below. */
const PANEL_ASPECT = 3.6;
const FONT_STACK = '"Fira Sans", system-ui, sans-serif';
/** The needle's rest position in a still preview, per channel. */
const PREVIEW_NEEDLES: readonly [number, number] = [0.5, 0.62];

/** Where the scale marks go, as VU values: labelled marks first, then the unlabelled ones. */
const SCALE_MARKS: readonly { vu: number; label: string | null }[] = [
  { vu: -20, label: "−20" },
  { vu: -10, label: "10" },
  { vu: -7, label: "7" },
  { vu: -5, label: "5" },
  { vu: -3, label: "3" },
  { vu: 0, label: "0" },
  { vu: 3, label: "3" },
  { vu: 5, label: "5+" },
  { vu: -15, label: null },
  { vu: -9, label: null },
  { vu: -8, label: null },
  { vu: -6, label: null },
  { vu: -4, label: null },
  { vu: -2, label: null },
  { vu: -1, label: null },
  { vu: 1, label: null },
  { vu: 2, label: null },
  { vu: 4, label: null },
];
/** The percentage scale under the VU scale: 100% is 0 VU. */
const PERCENT_MARKS = [0, 20, 40, 60, 80, 100] as const;
/** Points the red zone's band is drawn through, since it follows the scale's slight arch. */
const BAND_SEGMENTS = 12;

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

/**
 * One meter's geometry. The scale is printed flat and spread across the whole meter, as on
 * the deck: a mark at needle position `f` (0 = left stop, 1 = end of scale) sits at `x(f)`
 * on one horizontal line, so the left stop block and the red zone are level. The needle pivots far below the window, `depth` under
 * the scale, so it only tilts as it sweeps, and every mark leans along the needle that
 * would point at it: the fan of ticks in the reference.
 */
class Dial {
  readonly depth: number;

  constructor(
    readonly cx: number,
    readonly halfSpan: number,
    readonly baseY: number,
  ) {
    this.depth = halfSpan * 1.5;
  }

  get pivotY(): number {
    return this.baseY + this.depth;
  }

  x(f: number): number {
    return this.cx + (f - 0.5) * 2 * this.halfSpan;
  }

  /** The y of the scale line; the same for every mark. */
  y(_f: number): number {
    return this.baseY;
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
  const halfSpan = meterW * 0.43;
  const baseY = win.y + win.h * 0.52;
  return {
    width,
    height,
    win,
    dials: [
      new Dial(win.x + meterW / 2, halfSpan, baseY),
      new Dial(win.x + win.w - meterW / 2, halfSpan, baseY),
    ],
    numbersY: win.y + win.h * 0.27,
    lamps: {
      x: win.x + win.w / 2,
      w: win.w * 0.045,
      h: win.h * 0.055,
      yHigh: win.y + win.h * 0.4,
      yLow: win.y + win.h * 0.62,
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

/** Strokes the stretch of a dial's scale from `f0` to `f1`, `dy` below the marks' base. */
function strokeBand(
  ctx: CanvasRenderingContext2D,
  dial: Dial,
  f0: number,
  f1: number,
  dy: number,
): void {
  ctx.beginPath();
  for (let i = 0; i <= BAND_SEGMENTS; i++) {
    const f = f0 + ((f1 - f0) * i) / BAND_SEGMENTS;
    if (i === 0) ctx.moveTo(dial.x(f), dial.y(f) + dy);
    else ctx.lineTo(dial.x(f), dial.y(f) + dy);
  }
  ctx.stroke();
}

/** One meter's static scale: marks, red zone, percentage dots and the "VU" legend. */
function drawScale(
  ctx: CanvasRenderingContext2D,
  p: VuPalette,
  layout: Layout,
  dial: Dial,
  unitLabel: string,
): void {
  const wh = layout.win.h;

  // The red zone: a band from 0 VU to the end of the scale.
  ctx.strokeStyle = p.red;
  ctx.lineWidth = wh * 0.04;
  ctx.lineCap = "butt";
  strokeBand(ctx, dial, deflectionForVu(0), 1, wh * 0.02);

  ctx.strokeStyle = p.ink;
  ctx.fillStyle = p.ink;
  ctx.font = `500 ${wh * 0.13}px ${FONT_STACK}`;
  ctx.textAlign = "center";
  ctx.textBaseline = "middle";
  for (const { vu, label } of SCALE_MARKS) {
    const f = Math.min(1, deflectionForVu(vu));
    const major = label !== null;
    const a = dial.angle(f);
    const length = wh * (major ? 0.16 : 0.11);
    const x = dial.x(f);
    const y = dial.y(f);
    ctx.lineWidth = Math.max(1, wh * (major ? 0.011 : 0.008));
    ctx.beginPath();
    ctx.moveTo(x, y);
    ctx.lineTo(x + Math.sin(a) * length, y - Math.cos(a) * length);
    ctx.stroke();
    // The numbers run in a straight row, whatever the scale beneath does.
    if (label) ctx.fillText(label, x, layout.numbersY);
  }
  // The leftmost stop is a solid wedge running left from the first tick's base, thicker at
  // its outer end and level underneath with the red zone.
  {
    const x = dial.x(0);
    const bottom = dial.y(0) + wh * 0.04;
    ctx.beginPath();
    ctx.moveTo(x - wh * 0.1, dial.y(0) - wh * 0.005);
    ctx.lineTo(x + wh * 0.012, dial.y(0) + wh * 0.012);
    ctx.lineTo(x + wh * 0.012, bottom);
    ctx.lineTo(x - wh * 0.1, bottom);
    ctx.closePath();
    ctx.fill();
  }

  // Percentage dots on a line under the scale, their numbers in a row beneath.
  ctx.font = `500 ${wh * 0.065}px ${FONT_STACK}`;
  for (const pct of PERCENT_MARKS) {
    const f = deflectionForVu(0) * (pct / 100);
    ctx.beginPath();
    ctx.arc(dial.x(f), dial.y(f) + wh * 0.1, Math.max(1, wh * 0.012), 0, Math.PI * 2);
    ctx.fill();
    ctx.fillText(pct === 100 ? "100%" : String(pct), dial.x(f), dial.baseY + wh * 0.18);
  }

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

/** Both needles' positions for this frame: live, or fixed in a preview. */
function needlePositions(
  meters: VuMeters,
  preview: boolean,
): readonly [Needle | number, Needle | number] {
  return preview ? PREVIEW_NEEDLES : [meters.left, meters.right];
}

function positionOf(n: Needle | number): number {
  return typeof n === "number" ? n : n.position;
}

/** The immersive stage: the K-550 meter panel with soft shadows for depth. */
class VuMeterFullInstance implements VisualizerInstance {
  private readonly meters = new VuMeters();
  private palette!: VuPalette;
  private labels = readLabels();
  private layout: Layout | null = null;
  // The static panel and the glass sheen, each rendered once per size/theme/language.
  private base: HTMLCanvasElement | null = null;
  private glass: HTMLCanvasElement | null = null;
  private cacheKey = "";
  private themeVersion = 0;

  setTheme(colors: ThemeColors): void {
    this.palette = buildPalette(colors);
    this.themeVersion++;
  }

  push(frame: VisualizerFrame): void {
    if (frame.scope) {
      this.meters.push(
        frame.scope.points,
        referenceForTargetLufs(loudnessStore.targetLufs),
        frame.scope.truePeakDb,
      );
    } else if (!frame.playing) {
      this.meters.rest();
    }
  }

  step(dtMs: number, reducedMotion: boolean): boolean {
    return this.meters.step(dtMs, reducedMotion);
  }

  /** Rebuild the cached layers if the size, theme, language or pixel ratio changed. */
  private ensureCache(width: number, height: number): Layout | null {
    const dpr = (typeof window !== "undefined" && window.devicePixelRatio) || 1;
    if (this.labels.locale !== i18n.currentLocale) this.labels = readLabels();
    const key = `${width}x${height}@${dpr}/${this.themeVersion}/${this.labels.locale}`;
    if (key === this.cacheKey && this.layout && this.base && this.glass) return this.layout;
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
    this.cacheKey = key;
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
    const positions = needlePositions(this.meters, options.preview);
    const thickness = Math.max(1.2, win.h * 0.012);
    const lift = win.h * 0.02;
    const tipY = win.y + win.h * 0.3;
    strokeNeedle(ctx, p, layout.dials[0], positionOf(positions[0]), tipY, thickness, lift);
    strokeNeedle(ctx, p, layout.dials[1], positionOf(positions[1]), tipY, thickness, lift);
    ctx.restore();

    if (!options.preview) this.drawLamps(ctx, layout);
    ctx.drawImage(this.glass, -margin, -margin, cacheW, cacheH);
    ctx.restore();
  }

  private drawLamps(ctx: CanvasRenderingContext2D, layout: Layout): void {
    const { x, w, h, yHigh, yLow } = layout.lamps;
    const lamps = [
      [this.meters.lampHigh.level, yHigh],
      [this.meters.lampLow.level, yLow],
    ] as const;
    for (const [level, y] of lamps) {
      if (level <= 0) continue;
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
}

/** The playbar slot: two tiny dials with shadowed needles, no text. */
class VuMeterMiniInstance implements VisualizerInstance {
  private readonly meters = new VuMeters();
  private palette!: VuPalette;
  private dials: readonly [Dial, Dial] | null = null;
  private dialsFor = "";

  setTheme(colors: ThemeColors): void {
    this.palette = buildPalette(colors);
  }

  push(frame: VisualizerFrame): void {
    if (frame.scope) {
      this.meters.push(
        frame.scope.points,
        referenceForTargetLufs(loudnessStore.targetLufs),
        frame.scope.truePeakDb,
      );
    } else if (!frame.playing) {
      this.meters.rest();
    }
  }

  step(dtMs: number, reducedMotion: boolean): boolean {
    return this.meters.step(dtMs, reducedMotion);
  }

  private dialsFor_(width: number, height: number): readonly [Dial, Dial] {
    const key = `${width}x${height}`;
    if (!this.dials || this.dialsFor !== key) {
      const halfSpan = width * 0.21;
      this.dials = [
        new Dial(width * 0.25, halfSpan, height * 0.55),
        new Dial(width * 0.75, halfSpan, height * 0.55),
      ];
      this.dialsFor = key;
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

    const dials = this.dialsFor_(width, height);
    const positions = needlePositions(this.meters, options.preview);
    for (let i = 0; i < 2; i++) {
      // The full scale as a thin guide, with the red zone drawn over its end.
      ctx.strokeStyle = p.ink;
      ctx.globalAlpha = 0.45;
      ctx.lineWidth = 1;
      strokeBand(ctx, dials[i], 0, 1, 0);
      ctx.globalAlpha = 1;
      ctx.strokeStyle = p.red;
      ctx.lineWidth = Math.max(1.5, height * 0.09);
      ctx.lineCap = "butt";
      strokeBand(ctx, dials[i], deflectionForVu(0), 1, 0);
      strokeNeedle(ctx, p, dials[i], positionOf(positions[i]), height * 0.3, 1.1, 1);
    }
    ctx.restore();
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
