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
  /** The surface the canvas sits on, which the river is tinted from and contrast-checked against. */
  readonly surface: "bg-main" | "bg-playerbar";
}

const SIZES: Record<VisualizerSize, SalmonRunSizeConfig> = {
  // The immersive stage: a full reach of river with boulders, a school and leaps.
  full: {
    river: {
      fish: [5, 9],
      boulders: [4, 7],
      streaks: 60,
      pebbles: 70,
      fishLength: 0.075,
      leaf: true,
      leaps: true,
    },
    surface: "bg-main",
  },
  // The playbar slot (about 96×28 px): a strip of current with a few salmon and beat ripples.
  mini: {
    river: {
      fish: [2, 3],
      boulders: [0, 0],
      streaks: 10,
      pebbles: 8,
      fishLength: 0.34,
      leaf: false,
      leaps: false,
    },
    surface: "bg-playerbar",
  },
};

/** Every colour the river is drawn with, as opaque hex; alpha is applied with `globalAlpha`. */
export interface SalmonRunPalette {
  readonly water: string;
  readonly bank: string;
  readonly bed: string;
  readonly stone: string;
  readonly stoneLight: string;
  readonly shadow: string;
  readonly foam: string;
  readonly streak: string;
  readonly fishBody: string;
  readonly fishHead: string;
  readonly leaf: string;
  readonly light: string;
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
 * the cover's vibrant colour; salmon (sockeye red with a green head) and the leaf are
 * contrast-checked against the water.
 */
export function buildSalmonRunPalette(
  colors: ThemeColors,
  surface: "bg-main" | "bg-playerbar",
  artwork: readonly string[],
): SalmonRunPalette {
  const rawBg = colors[surface] || colors["bg-main"];
  const bg = rawBg && HEX6.test(rawBg) ? rawBg : "#191918";
  const light = isLightColor(bg);
  const art = artwork.filter((c) => HEX6.test(c));
  const accent = HEX6.test(colors["color-accent"] || "") ? colors["color-accent"] : "#4f5bd5";

  let water = mix(bg, light ? "#7fb9c2" : "#1d4f5a", 0.55);
  const muted = art[1] ?? art[0];
  if (muted) water = mix(water, muted, 0.22);
  const stone = mix(water, light ? "#8b8478" : "#7a7c74", 0.7);
  const foam = light ? "#ffffff" : "#dff1f2";
  return {
    water,
    bank: mix(water, light ? "#5d6b5a" : "#0b1a1d", 0.35),
    bed: mix(water, light ? "#4e5a52" : "#8aa0a0", 0.35),
    stone,
    stoneLight: mix(stone, "#ffffff", 0.3),
    shadow: light ? "#1d2b2e" : "#000000",
    foam,
    streak: clampForContrast(mix(art[0] ?? accent, foam, 0.45), water, 1.8),
    fishBody: clampForContrast("#c43b2e", water, 3.0),
    fishHead: clampForContrast("#3f6a43", water, 2.0),
    leaf: clampForContrast("#d4602a", water, 3.0),
    light: light ? "#ffffff" : "#bfe6ea",
  };
}

/** A maple leaf outline in unit coordinates (stem at +y), traced once at load. */
const MAPLE: readonly number[] = [
  0, -1, 0.14, -0.62, 0.38, -0.78, 0.32, -0.42, 0.78, -0.5, 0.62, -0.28, 0.96, -0.06, 0.5, 0.06,
  0.58, 0.32, 0.12, 0.22, 0.04, 0.5, -0.04, 0.5, -0.12, 0.22, -0.58, 0.32, -0.5, 0.06, -0.96, -0.06,
  -0.62, -0.28, -0.78, -0.5, -0.32, -0.42, -0.38, -0.78, -0.14, -0.62,
];

/**
 * Draws `river` filling a `width`×`height` canvas. Works in world units (the height is 1) and
 * allocates nothing beyond the bank gradient, which is cached per palette.
 */
class SalmonRunRenderer {
  private gradient: CanvasGradient | null = null;
  private gradientFor: SalmonRunPalette | null = null;

  render(
    ctx: CanvasRenderingContext2D,
    width: number,
    height: number,
    river: SalmonRun,
    p: SalmonRunPalette,
  ): void {
    river.setAspect(width / height);
    const w = river.aspect;
    ctx.save();
    ctx.scale(height, height);
    ctx.globalAlpha = 1;

    ctx.fillStyle = p.water;
    ctx.fillRect(0, 0, w, 1);
    if (this.gradientFor !== p || !this.gradient) {
      // Deeper mid-channel, shallower toward the banks. In world units, so any canvas size shares it.
      const g = ctx.createLinearGradient(0, 0, 0, 1);
      g.addColorStop(0, p.bank);
      g.addColorStop(0.16, p.water);
      g.addColorStop(0.84, p.water);
      g.addColorStop(1, p.bank);
      this.gradient = g;
      this.gradientFor = p;
    }
    ctx.fillStyle = this.gradient;
    ctx.fillRect(0, 0, w, 1);

    // The light on the water: bass lifts it a little, and under reduced motion beats do too.
    const lightLevel = river.bass * 0.05 + river.glow * 0.16;
    if (lightLevel > 0.003) {
      ctx.globalAlpha = lightLevel;
      ctx.fillStyle = p.light;
      ctx.fillRect(0, 0, w, 1);
    }

    this.drawBed(ctx, river, p);
    this.drawStreaks(ctx, river, p);
    this.drawBoulders(ctx, river, p);
    this.drawRipples(ctx, river, p);
    this.drawSchool(ctx, river, p);
    if (river.leafActive) this.drawLeaf(ctx, river, p);

    ctx.restore();
  }

  private drawBed(ctx: CanvasRenderingContext2D, river: SalmonRun, p: SalmonRunPalette): void {
    const n = river.pebbleFX.length;
    if (n === 0) return;
    ctx.globalAlpha = 0.35;
    ctx.fillStyle = p.bed;
    ctx.beginPath();
    for (let i = 0; i < n; i++) {
      const x = river.pebbleFX[i] * river.aspect;
      const y = river.pebbleY[i];
      const r = river.pebbleR[i];
      ctx.moveTo(x + r, y);
      ctx.ellipse(x, y, r, r * 0.75, 0, 0, Math.PI * 2);
    }
    ctx.fill();
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
    ctx.globalAlpha = (0.18 + 0.22 * river.treble) * (0.45 + 0.55 * river.activity);
    ctx.strokeStyle = p.streak;
    ctx.lineWidth = 0.004;
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

  private drawBoulders(ctx: CanvasRenderingContext2D, river: SalmonRun, p: SalmonRunPalette): void {
    for (let b = 0; b < river.boulderCount; b++) {
      const x = river.boulderFX[b] * river.aspect;
      const y = river.boulderY[b];
      const r = river.boulderR[b];

      // Eddy wake downstream, pulsing with the bass.
      ctx.globalAlpha = (0.1 + 0.45 * river.bass) * (0.5 + 0.5 * river.activity);
      ctx.strokeStyle = p.foam;
      ctx.lineWidth = 0.005;
      ctx.beginPath();
      for (let k = 1; k <= 3; k++) {
        const reach = r * (0.9 + k * 0.55);
        ctx.moveTo(x + reach * Math.cos(-0.7), y + reach * Math.sin(-0.7));
        ctx.arc(x, y, reach, -0.7, 0.7);
      }
      ctx.stroke();

      ctx.globalAlpha = 0.28;
      ctx.fillStyle = p.shadow;
      ctx.beginPath();
      ctx.ellipse(x + r * 0.18, y + r * 0.22, r, r * 0.86, 0, 0, Math.PI * 2);
      ctx.fill();

      ctx.globalAlpha = 1;
      ctx.fillStyle = p.stone;
      ctx.beginPath();
      ctx.ellipse(x, y, r, r * 0.86, 0.3, 0, Math.PI * 2);
      ctx.fill();
      ctx.globalAlpha = 0.55;
      ctx.fillStyle = p.stoneLight;
      ctx.beginPath();
      ctx.ellipse(x - r * 0.28, y - r * 0.26, r * 0.5, r * 0.34, 0.3, 0, Math.PI * 2);
      ctx.fill();

      // Foam where the current meets the upstream face.
      ctx.globalAlpha = 0.5;
      ctx.strokeStyle = p.foam;
      ctx.lineWidth = 0.006;
      ctx.beginPath();
      ctx.arc(x, y, r * 1.12, Math.PI * 0.7, Math.PI * 1.3);
      ctx.stroke();
    }
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
      // A fish at a standstill keeps facing upstream.
      const angle = Math.abs(vx) + Math.abs(vy) < 1e-5 ? Math.PI : Math.atan2(vy, vx);
      const leap = river.fishLeap[i];
      const lift = leap > 0 ? Math.sin(Math.PI * (1 - leap / LEAP_S)) : 0;
      const len = baseLen * river.fishScale[i] * (1 + 0.45 * lift);
      const flex = Math.sin(river.fishTail[i]) * len * 0.09;
      const x = river.fishX[i];
      const y = river.fishY[i];

      ctx.save();
      ctx.translate(x, y);
      ctx.rotate(angle);

      // Shadow on the riverbed, further away while the fish is in the air.
      const drop = len * (0.12 + 0.6 * lift);
      ctx.globalAlpha = 0.22 * (1 - 0.5 * lift);
      ctx.fillStyle = p.shadow;
      ctx.beginPath();
      ctx.ellipse(-drop * 0.3, drop, len * 0.45, len * 0.12, 0, 0, Math.PI * 2);
      ctx.fill();

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
    const size = 0.045;
    ctx.save();
    ctx.translate(river.leafX, river.leafY);
    ctx.rotate(river.leafAngle);
    ctx.scale(size, size);
    ctx.globalAlpha = 0.25;
    ctx.fillStyle = p.shadow;
    this.traceMaple(ctx, 0.15, 0.25);
    ctx.fill();
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
  private readonly renderer = new SalmonRunRenderer();
  private colors: ThemeColors | null = null;
  private artwork: readonly string[] = [];
  private palette!: SalmonRunPalette;

  constructor(private readonly config: SalmonRunSizeConfig) {
    this.river = new SalmonRun(config.river);
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
    this.palette = buildSalmonRunPalette(this.colors, this.config.surface, this.artwork);
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
 * surge or leap, beats drop ripples, and a maple leaf drifts by when the track changes.
 */
export const SALMON_RUN_PRESET: VisualizerPreset = {
  id: "salmon-run",
  labelKey: "visualizers.salmonRun",
  needs: ["spectrum"],
  create(size: VisualizerSize): VisualizerInstance {
    return new SalmonRunInstance(SIZES[size]);
  },
};
