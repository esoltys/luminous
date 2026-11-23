import { describe, expect, it } from "vitest";
import { LUMINOUS_DARK_COLORS, LUMINOUS_LIGHT_COLORS } from "../stores/theme.svelte";
import { calculateContrastRatio } from "../utils/colorUtils";
import { SALMON_RUN_PRESET, buildSalmonRunPalette } from "./salmonRun";

/** A 2D context that records the corners of every filled path, and counts fills. */
function recordingContext(width: number, height: number) {
  const points: [number, number][] = [];
  let fills = 0;
  const ctx = {
    canvas: { width, height },
    fillStyle: "",
    clearRect() {},
    fillRect(x: number, y: number) {
      points.push([x, y]);
      fills++;
    },
    beginPath() {},
    closePath() {},
    moveTo(x: number, y: number) {
      points.push([x, y]);
    },
    lineTo(x: number, y: number) {
      points.push([x, y]);
    },
    fill() {
      fills++;
    },
  };
  return { ctx: ctx as unknown as CanvasRenderingContext2D, points, fills: () => fills };
}

describe("buildSalmonRunPalette", () => {
  for (const [name, colors] of [
    ["dark", LUMINOUS_DARK_COLORS],
    ["light", LUMINOUS_LIGHT_COLORS],
  ] as const) {
    for (const surface of ["bg-main", "bg-playerbar"] as const) {
      it(`keeps the salmon and leaf visible on ${surface} in the ${name} theme`, () => {
        const p = buildSalmonRunPalette(colors, surface);
        const bg = colors[surface];
        expect(calculateContrastRatio(p.fishBody, bg)).toBeGreaterThanOrEqual(4.5);
        expect(calculateContrastRatio(p.fishHead, bg)).toBeGreaterThanOrEqual(3);
        expect(calculateContrastRatio(p.leaf, bg)).toBeGreaterThanOrEqual(4.5);
      });
    }

    it(`sets the salmon apart from the water in the ${name} theme`, () => {
      const p = buildSalmonRunPalette(colors, "bg-main");
      expect(calculateContrastRatio(p.fishBodyFaces.top, p.water.top)).toBeGreaterThanOrEqual(2);
    });
  }

  it("takes its colours from the theme", () => {
    const plain = buildSalmonRunPalette(LUMINOUS_DARK_COLORS, "bg-main");
    const other = buildSalmonRunPalette(
      { ...LUMINOUS_DARK_COLORS, "color-accent": "#e0b020", "color-accent-hover": "#f0c040" },
      "bg-main",
    );
    expect(other.fishBody).not.toBe(plain.fishBody);
    expect(other.water).not.toEqual(plain.water);
  });

  it("washes more light over the water at each reduced-motion light step", () => {
    const p = buildSalmonRunPalette(LUMINOUS_DARK_COLORS, "bg-main");
    const alpha = p.lightWash.map((fill) => Number(/, ([\d.]+)\)$/.exec(fill)![1]));
    for (let l = 1; l < alpha.length; l++) expect(alpha[l]).toBeGreaterThan(alpha[l - 1]);
  });
});

describe("SALMON_RUN_PRESET", () => {
  it("creates an instance for both sizes that takes a track", () => {
    for (const size of ["full", "mini"] as const) {
      const instance = SALMON_RUN_PRESET.create(size);
      instance.setTheme(LUMINOUS_DARK_COLORS);
      instance.setTrack?.({ key: "1" });
      instance.setTrack?.(null);
      instance.push({ bands: new Array(32).fill(0.3), onset: 0, playing: true, scope: null });
      expect(instance.step(16, false)).toBe(true);
    }
  });

  it("fits the whole ladder inside the stage, whatever its shape", () => {
    for (const [width, height] of [
      [1600, 900],
      [900, 1600],
      [3840, 2160],
      [320, 180],
    ]) {
      const instance = SALMON_RUN_PRESET.create("full");
      instance.setTheme(LUMINOUS_DARK_COLORS);
      instance.setTrack?.({ key: "fit" });
      instance.push({ bands: new Array(32).fill(0.5), onset: 0.8, playing: true, scope: null });
      for (let t = 0; t < 120; t++) instance.step(16, false);
      const { ctx, points, fills } = recordingContext(width, height);
      instance.render(ctx, width, height, { reducedMotion: false, preview: false });
      expect(fills()).toBeGreaterThan(500);
      const xs = points.map(([x]) => x);
      const ys = points.map(([, y]) => y);
      expect(Math.min(...xs)).toBeGreaterThanOrEqual(0);
      expect(Math.max(...xs)).toBeLessThanOrEqual(width);
      expect(Math.min(...ys)).toBeGreaterThanOrEqual(0);
      expect(Math.max(...ys)).toBeLessThanOrEqual(height);
      // The ladder fills the stage along at least one side.
      const spread = Math.max(
        (Math.max(...xs) - Math.min(...xs)) / width,
        (Math.max(...ys) - Math.min(...ys)) / height,
      );
      expect(spread).toBeGreaterThan(0.85);
    }
  });

  it("draws the playbar's lone salmon on its slot", () => {
    const instance = SALMON_RUN_PRESET.create("mini");
    instance.setTheme(LUMINOUS_DARK_COLORS);
    instance.setTrack?.({ key: "1" });
    const { ctx, points } = recordingContext(96, 28);
    instance.render(ctx, 96, 28, { reducedMotion: false, preview: true });
    expect(points.length).toBeGreaterThan(10);
  });
});
