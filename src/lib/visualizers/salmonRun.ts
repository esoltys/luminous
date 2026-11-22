import type { ThemeColors } from "../stores/theme.svelte";
import { clampForContrast, hexToRgb, isLightColor, rgbToHex } from "../utils/colorUtils";
import { LEAP_S, RIPPLE_POOL, SalmonRun, type SalmonRunConfig } from "./salmonRunEngine";
import type {
  VisualizerFrame,
  VisualizerInstance,
  VisualizerPreset,
  VisualizerSize,
  VisualizerTrack,
} from "./types";

interface SalmonRunSizeConfig {
  readonly river: SalmonRunConfig;
  /**
   * Draw the river itself (water, banks, riverbed, boulders, current). Without it the salmon and
   * leaf are drawn straight onto the surface the canvas sits on.
   */
  readonly scenery: boolean;
  /** The surface the canvas sits on, which colours are derived from and contrast-checked against. */
  readonly surface: "bg-main" | "bg-playerbar";
}

const SIZES: Record<VisualizerSize, SalmonRunSizeConfig> = {
  // The immersive stage: a full reach of river with boulders, a school, leaps and ripples.
  full: {
    river: {
      fish: [5, 9],
      boulders: [4, 7],
      streaks: 60,
      pebbles: 70,
      fishLength: 0.1,
      leaf: true,
      leafSize: 0.065,
      leafCrossS: 20,
      ripples: true,
      transients: "leap",
      holdStation: false,
    },
    scenery: true,
    surface: "bg-main",
  },
  // The playbar slot (about 96×28 px): a lone salmon holding station in an unseen current, and the
  // track-change leaf drifting past. Anything more is noise at this size.
  mini: {
    river: {
      fish: [1, 1],
      boulders: [0, 0],
      streaks: 0,
      pebbles: 0,
      fishLength: 0.85,
      leaf: true,
      leafSize: 0.3,
      leafCrossS: 6,
      ripples: false,
      transients: "surge",
      holdStation: true,
    },
    scenery: false,
    surface: "bg-playerbar",
  },
};

/** Bands each bank is shaded in, darkest at the edge. */
const BANK_STRIPS = 8;
/** Fraction of the height each bank's shading covers. */
const BANK_DEPTH = 0.16;
/** Furthest a salmon turns from straight upstream, in radians. */
const MAX_VEER = 0.7;

/** Every colour the river is drawn with, as opaque hex; alpha is applied with `globalAlpha`. */
export interface SalmonRunPalette {
  readonly water: string;
  /** `BANK_STRIPS` shades from each bank's edge inward: shallower water darkening to the edge. */
  readonly bankShades: readonly string[];
  /** Lifts the water on a beat under reduced motion. */
  readonly light: string;
  readonly bed: string;
  readonly stone: string;
  readonly stoneLight: string;
  readonly shadow: string;
  readonly foam: string;
  readonly streak: string;
  readonly fishBody: string;
  readonly fishHead: string;
  readonly leaf: string;
}

const HEX6 = /^#[0-9a-f]{6}$/i;

function mix(a: string, b: string, t: number): string {
  const x = hexToRgb(a);
  const y = hexToRgb(b);
  return rgbToHex(
    Math.round(x.r + (y.r - x.r) * t),
    Math.round(x.g + (y.g - x.g) * t),
    Math.round(x.b + (y.b - x.b) * t),
  );
}

/**
 * Builds the river's colours for the surface it's drawn on, tinted by the track's cover colours
 * (`artwork`, most characteristic first; may be empty). The water is the surface blended towards a
 * river teal and the cover's muted tone, so it suits light and dark themes; current streaks take
 * a hint of the cover's vibrant colour. The salmon (sockeye red with a green head) and the leaf
 * are contrast-checked against what they sit on: the water with `scenery`, else the surface.
 */
export function buildSalmonRunPalette(
  colors: ThemeColors,
  surface: "bg-main" | "bg-playerbar",
  artwork: readonly string[],
  scenery = true,
): SalmonRunPalette {
  const rawBg = colors[surface] || colors["bg-main"];
  const bg = rawBg && HEX6.test(rawBg) ? rawBg : "#191918";
  const light = isLightColor(bg);
  const art = artwork.filter((c) => HEX6.test(c));
  const accent = HEX6.test(colors["color-accent"] || "") ? colors["color-accent"] : "#4f5bd5";

  let water = mix(bg, light ? "#7fb9c2" : "#1d4f5a", 0.55);
  const muted = art[1] ?? art[0];
  if (muted) water = mix(water, muted, 0.22);
  const ground = scenery ? water : bg;
  const stone = mix(water, light ? "#8b8478" : "#7a7c74", 0.7);
  const foam = light ? "#ffffff" : "#dff1f2";
  const bankTone = light ? "#5d6b5a" : "#0b1a1d";
  const bankShades: string[] = [];
  for (let s = 0; s < BANK_STRIPS; s++) {
    bankShades.push(mix(water, bankTone, 0.35 * (1 - (s + 0.5) / BANK_STRIPS)));
  }
  return {
    water,
    bankShades,
    light: light ? "#ffffff" : "#bfe6ea",
    bed: mix(water, light ? "#4e5a52" : "#8aa0a0", 0.35),
    stone,
    stoneLight: mix(stone, "#ffffff", 0.3),
    shadow: light ? "#1d2b2e" : "#000000",
    foam,
    // Glints of light on the surface: mostly foam with a hint of the cover. Not contrast-clamped,
    // which would darken them toward the bed on light water and turn them into twigs.
    streak: mix(art[0] ?? accent, foam, 0.7),
    fishBody: clampForContrast("#c43b2e", ground, 3.0),
    fishHead: clampForContrast("#3f6a43", ground, 2.0),
    leaf: clampForContrast("#d4602a", ground, 3.0),
  };
}

/** A maple leaf outline in unit coordinates (stem at +y), traced once at load. */
const MAPLE: readonly number[] = [
  0, -1, 0.14, -0.62, 0.38, -0.78, 0.32, -0.42, 0.78, -0.5, 0.62, -0.28, 0.96, -0.06, 0.5, 0.06,
  0.58, 0.32, 0.12, 0.22, 0.04, 0.5, -0.04, 0.5, -0.12, 0.22, -0.58, 0.32, -0.5, 0.06, -0.96, -0.06,
  -0.62, -0.28, -0.78, -0.5, -0.32, -0.42, -0.38, -0.78, -0.14, -0.62,
];

type Ctx2D = CanvasRenderingContext2D | OffscreenCanvasRenderingContext2D;

/**
 * The parts of the river that hold still for a whole track (water, banks, riverbed, boulders),
 * drawn in world units (the height is 1, the width `river.aspect`).
 */
function drawRiverbed(c: Ctx2D, river: SalmonRun, p: SalmonRunPalette): void {
  const w = river.aspect;
  c.globalAlpha = 1;
  c.fillStyle = p.water;
  c.fillRect(0, 0, w, 1);
  // Shallower toward the banks: flat bands darkening to each edge. (A full-canvas gradient fill
  // costs more than the rest of the frame together at 4K.)
  const band = BANK_DEPTH / BANK_STRIPS;
  for (let s = 0; s < BANK_STRIPS; s++) {
    c.fillStyle = p.bankShades[s];
    c.fillRect(0, s * band, w, band);
    c.fillRect(0, 1 - (s + 1) * band, w, band);
  }

  if (river.pebbleFX.length > 0) {
    c.globalAlpha = 0.35;
    c.fillStyle = p.bed;
    c.beginPath();
    for (let i = 0; i < river.pebbleFX.length; i++) {
      const x = river.pebbleFX[i] * w;
      const y = river.pebbleY[i];
      const r = river.pebbleR[i];
      c.moveTo(x + r, y);
      c.ellipse(x, y, r, r * 0.75, 0, 0, Math.PI * 2);
    }
    c.fill();
  }

  for (let b = 0; b < river.boulderCount; b++) {
    const x = river.boulderFX[b] * w;
    const y = river.boulderY[b];
    const r = river.boulderR[b];
    c.globalAlpha = 0.28;
    c.fillStyle = p.shadow;
    c.beginPath();
    c.ellipse(x + r * 0.18, y + r * 0.22, r, r * 0.86, 0, 0, Math.PI * 2);
    c.fill();
    c.globalAlpha = 1;
    c.fillStyle = p.stone;
    c.beginPath();
    c.ellipse(x, y, r, r * 0.86, 0.3, 0, Math.PI * 2);
    c.fill();
    c.globalAlpha = 0.55;
    c.fillStyle = p.stoneLight;
    c.beginPath();
    c.ellipse(x - r * 0.28, y - r * 0.26, r * 0.5, r * 0.34, 0.3, 0, Math.PI * 2);
    c.fill();
    // Foam where the current meets the upstream face.
    c.globalAlpha = 0.5;
    c.strokeStyle = p.foam;
    c.lineWidth = 0.006;
    c.beginPath();
    c.arc(x, y, r * 1.12, Math.PI * 0.7, Math.PI * 1.3);
    c.stroke();
  }
  c.globalAlpha = 1;
}

/**
 * The riverbed drawn once onto an offscreen surface at the canvas's backing size, and redrawn only
 * when the canvas size, palette or track layout changes; every other frame is one `drawImage`.
 */
class RiverbedLayer {
  private surface: HTMLCanvasElement | OffscreenCanvas | null = null;
  private surfaceCtx: Ctx2D | null = null;
  private backingW = 0;
  private backingH = 0;
  private palette: SalmonRunPalette | null = null;
  private layoutVersion = -1;

  /** Draws the riverbed over the whole `width`×`height` canvas (CSS units); false if it can't. */
  blit(
    ctx: CanvasRenderingContext2D,
    width: number,
    height: number,
    river: SalmonRun,
    p: SalmonRunPalette,
  ): boolean {
    const canvas = ctx.canvas as HTMLCanvasElement | undefined;
    if (!canvas || typeof ctx.drawImage !== "function") return false;
    const backingW = Math.max(1, canvas.width);
    const backingH = Math.max(1, canvas.height);
    if (
      !this.surfaceCtx ||
      backingW !== this.backingW ||
      backingH !== this.backingH ||
      p !== this.palette ||
      river.layoutVersion !== this.layoutVersion
    ) {
      if (!this.ensureSurface(backingW, backingH)) return false;
      const c = this.surfaceCtx!;
      c.setTransform(backingH, 0, 0, backingH, 0, 0);
      drawRiverbed(c, river, p);
      this.backingW = backingW;
      this.backingH = backingH;
      this.palette = p;
      this.layoutVersion = river.layoutVersion;
    }
    ctx.drawImage(this.surface!, 0, 0, width, height);
    return true;
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

/**
 * Draws `river` filling a `width`×`height` canvas, in world units (the height is 1). Allocates
 * nothing per frame: the riverbed is a cached layer, and the moving parts are small shapes.
 */
class SalmonRunRenderer {
  private readonly bed = new RiverbedLayer();

  constructor(private readonly scenery: boolean) {}

  render(
    ctx: CanvasRenderingContext2D,
    width: number,
    height: number,
    river: SalmonRun,
    p: SalmonRunPalette,
  ): void {
    river.setAspect(width / height);
    ctx.save();
    if (!this.scenery) {
      ctx.clearRect(0, 0, width, height);
    } else if (!this.bed.blit(ctx, width, height, river, p)) {
      ctx.scale(height, height);
      drawRiverbed(ctx, river, p);
      ctx.restore();
      ctx.save();
    }
    ctx.scale(height, height);
    ctx.globalAlpha = 1;

    if (this.scenery) {
      // Under reduced motion a beat lifts the light on the water instead of making waves.
      if (river.glow > 0.003) {
        ctx.globalAlpha = river.glow * 0.16;
        ctx.fillStyle = p.light;
        ctx.fillRect(0, 0, river.aspect, 1);
      }
      this.drawWakes(ctx, river, p);
      this.drawStreaks(ctx, river, p);
      this.drawRipples(ctx, river, p);
    }
    this.drawSchool(ctx, river, p);
    if (river.leafActive) this.drawLeaf(ctx, river, p);

    ctx.restore();
  }

  /** Vertical shift of the current at (`x`, `y`) as it parts around the boulders. */
  private deflect(river: SalmonRun, x: number, y: number): number {
    let dy = 0;
    for (let b = 0; b < river.boulderCount; b++) {
      const bx = river.boulderFX[b] * river.aspect;
      const by = river.boulderY[b];
      const r = river.boulderR[b];
      const ox = x - bx;
      const oy = y - by;
      dy += (r * r * 1.6 * oy) / (ox * ox + oy * oy + r * r);
    }
    return dy;
  }

  private drawStreaks(ctx: CanvasRenderingContext2D, river: SalmonRun, p: SalmonRunPalette): void {
    const n = river.streakX.length;
    if (n === 0) return;
    ctx.globalAlpha = (0.3 + 0.3 * river.treble) * (0.45 + 0.55 * river.activity);
    ctx.strokeStyle = p.streak;
    ctx.lineWidth = 0.0035;
    ctx.lineCap = "round";
    ctx.beginPath();
    for (let i = 0; i < n; i++) {
      const x1 = river.streakX[i];
      const len = river.streakLen[i];
      const y = river.streakY[i];
      const x0 = x1 - len;
      const xm = x1 - len * 0.5;
      ctx.moveTo(x0, y + this.deflect(river, x0, y));
      ctx.lineTo(xm, y + this.deflect(river, xm, y));
      ctx.lineTo(x1, y + this.deflect(river, x1, y));
    }
    ctx.stroke();
  }

  /** Eddy wakes downstream of each boulder, pulsing with the bass. */
  private drawWakes(ctx: CanvasRenderingContext2D, river: SalmonRun, p: SalmonRunPalette): void {
    if (river.boulderCount === 0) return;
    ctx.globalAlpha = (0.06 + 0.3 * river.bass) * (0.5 + 0.5 * river.activity);
    ctx.strokeStyle = p.foam;
    ctx.lineWidth = 0.0035;
    ctx.beginPath();
    for (let b = 0; b < river.boulderCount; b++) {
      const x = river.boulderFX[b] * river.aspect;
      const y = river.boulderY[b];
      const r = river.boulderR[b];
      // Arcs narrow as the wake trails off downstream.
      for (let k = 1; k <= 3; k++) {
        const reach = r * (1.05 + k * 0.45);
        const sweep = 0.6 - k * 0.1;
        ctx.moveTo(x + reach * Math.cos(-sweep), y + reach * Math.sin(-sweep));
        ctx.arc(x, y, reach, -sweep, sweep);
      }
    }
    ctx.stroke();
  }

  private drawRipples(ctx: CanvasRenderingContext2D, river: SalmonRun, p: SalmonRunPalette): void {
    ctx.strokeStyle = p.foam;
    ctx.lineWidth = 0.005;
    for (let i = 0; i < RIPPLE_POOL; i++) {
      const alpha = river.rippleAlpha(i);
      if (alpha <= 0) continue;
      const r = river.rippleRadius(i);
      ctx.globalAlpha = alpha * 0.8;
      ctx.beginPath();
      ctx.arc(river.rippleX[i], river.rippleY[i], r, 0, Math.PI * 2);
      if (r > 0.03) {
        ctx.moveTo(river.rippleX[i] + r * 0.6, river.rippleY[i]);
        ctx.arc(river.rippleX[i], river.rippleY[i], r * 0.6, 0, Math.PI * 2);
      }
      ctx.stroke();
    }
  }

  private drawSchool(ctx: CanvasRenderingContext2D, river: SalmonRun, p: SalmonRunPalette): void {
    const baseLen = river.config.fishLength;
    for (let i = 0; i < river.fishCount; i++) {
      const vx = river.fishVX[i];
      const vy = river.fishVY[i];
      // Always into the current, turned at most ~40° toward a bank: a salmon sidestepping a
      // boulder, or carried back after a dart, still points upstream.
      const veer = Math.atan2(vy, Math.max(-vx, 0.04));
      const angle = Math.PI - Math.max(-MAX_VEER, Math.min(MAX_VEER, veer));
      const leap = river.fishLeap[i];
      const lift = leap > 0 ? Math.sin(Math.PI * (1 - leap / LEAP_S)) : 0;
      const len = baseLen * river.fishScale[i] * (1 + 0.45 * lift);
      const flex = Math.sin(river.fishTail[i]) * len * 0.09;

      ctx.save();
      ctx.translate(river.fishX[i], river.fishY[i]);
      ctx.rotate(angle);

      if (this.scenery) {
        // Shadow on the riverbed, further away while the fish is in the air.
        const drop = len * (0.12 + 0.6 * lift);
        ctx.globalAlpha = 0.22 * (1 - 0.5 * lift);
        ctx.fillStyle = p.shadow;
        ctx.beginPath();
        ctx.ellipse(-drop * 0.3, drop, len * 0.45, len * 0.12, 0, 0, Math.PI * 2);
        ctx.fill();
      }

      ctx.globalAlpha = 1;
      ctx.fillStyle = p.fishBody;
      ctx.beginPath();
      ctx.moveTo(0.5 * len, 0);
      ctx.bezierCurveTo(
        0.36 * len,
        -0.13 * len,
        0.04 * len,
        -0.15 * len,
        -0.32 * len,
        -0.035 * len + flex * 0.5,
      );
      ctx.lineTo(-0.52 * len, -0.13 * len + flex);
      ctx.lineTo(-0.44 * len, flex * 0.9);
      ctx.lineTo(-0.52 * len, 0.13 * len + flex);
      ctx.lineTo(-0.32 * len, 0.035 * len + flex * 0.5);
      ctx.bezierCurveTo(0.04 * len, 0.15 * len, 0.36 * len, 0.13 * len, 0.5 * len, 0);
      ctx.fill();

      ctx.fillStyle = p.fishHead;
      ctx.beginPath();
      ctx.ellipse(0.34 * len, 0, 0.16 * len, 0.085 * len, 0, 0, Math.PI * 2);
      ctx.fill();

      ctx.restore();
    }
  }

  private drawLeaf(ctx: CanvasRenderingContext2D, river: SalmonRun, p: SalmonRunPalette): void {
    // The outline spans 2 units, so half the leaf's width scales it.
    const scale = river.config.leafSize * 0.5;
    ctx.save();
    ctx.translate(river.leafX, river.leafY);
    ctx.rotate(river.leafAngle);
    ctx.scale(scale, scale);
    if (this.scenery) {
      ctx.globalAlpha = 0.25;
      ctx.fillStyle = p.shadow;
      this.traceMaple(ctx, 0.15, 0.25);
      ctx.fill();
    }
    ctx.globalAlpha = 0.95;
    ctx.fillStyle = p.leaf;
    this.traceMaple(ctx, 0, 0);
    ctx.fill();
    ctx.strokeStyle = p.leaf;
    ctx.lineWidth = 0.08;
    ctx.beginPath();
    ctx.moveTo(0, 0.45);
    ctx.lineTo(0, 0.9);
    ctx.stroke();
    ctx.restore();
  }

  private traceMaple(ctx: CanvasRenderingContext2D, ox: number, oy: number): void {
    ctx.beginPath();
    ctx.moveTo(MAPLE[0] + ox, MAPLE[1] + oy);
    for (let k = 2; k < MAPLE.length; k += 2) ctx.lineTo(MAPLE[k] + ox, MAPLE[k + 1] + oy);
    ctx.closePath();
  }
}

class SalmonRunInstance implements VisualizerInstance {
  private readonly river: SalmonRun;
  private readonly renderer: SalmonRunRenderer;
  private colors: ThemeColors | null = null;
  private artwork: readonly string[] = [];
  private palette!: SalmonRunPalette;

  constructor(private readonly config: SalmonRunSizeConfig) {
    this.river = new SalmonRun(config.river);
    this.renderer = new SalmonRunRenderer(config.scenery);
  }

  setTheme(colors: ThemeColors): void {
    this.colors = colors;
    this.rebuildPalette();
  }

  setTrack(track: VisualizerTrack | null): void {
    // With nothing loaded the river stays as it was; the next track still brings a leaf.
    if (track) this.river.setTrack(track.key);
    const artwork = track?.artwork ?? [];
    if (!sameColors(artwork, this.artwork)) {
      this.artwork = artwork;
      this.rebuildPalette();
    }
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

  private rebuildPalette(): void {
    if (!this.colors) return;
    this.palette = buildSalmonRunPalette(
      this.colors,
      this.config.surface,
      this.artwork,
      this.config.scenery,
    );
  }
}

function sameColors(a: readonly string[], b: readonly string[]): boolean {
  if (a.length !== b.length) return false;
  for (let i = 0; i < a.length; i++) if (a[i] !== b[i]) return false;
  return true;
}

/**
 * A salmon run seen from above: a clear mountain river tinted by the cover art, with salmon
 * swimming upstream past boulders. Mid and treble set their pace, treble transients make one
 * surge or leap, beats drop ripples, and a maple leaf drifts by when the track changes. The
 * playbar slot shows a lone salmon holding station and the leaf.
 */
export const SALMON_RUN_PRESET: VisualizerPreset = {
  id: "salmon-run",
  labelKey: "visualizers.salmonRun",
  needs: ["spectrum"],
  create(size: VisualizerSize): VisualizerInstance {
    return new SalmonRunInstance(SIZES[size]);
  },
};
