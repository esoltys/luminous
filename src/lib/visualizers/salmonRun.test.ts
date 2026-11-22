import { describe, expect, it } from "vitest";
import { LUMINOUS_DARK_COLORS, LUMINOUS_LIGHT_COLORS } from "../stores/theme.svelte";
import { calculateContrastRatio } from "../utils/colorUtils";
import { SALMON_RUN_PRESET, buildSalmonRunPalette } from "./salmonRun";
import { voxelUnlitCellColor } from "./voxelGrid";

describe("buildSalmonRunPalette", () => {
  for (const [name, colors] of [
    ["dark", LUMINOUS_DARK_COLORS],
    ["light", LUMINOUS_LIGHT_COLORS],
  ] as const) {
    for (const surface of ["bg-main", "bg-playerbar"] as const) {
      it(`keeps the salmon and leaf visible on ${surface} in the ${name} theme`, () => {
        const p = buildSalmonRunPalette(colors, surface);
        const bg = colors[surface];
        expect(calculateContrastRatio(p.fishBody, bg)).toBeGreaterThanOrEqual(3);
        expect(calculateContrastRatio(p.fishHead, bg)).toBeGreaterThanOrEqual(3);
        expect(calculateContrastRatio(p.leaf, bg)).toBeGreaterThanOrEqual(4.5);
      });
    }
  }

  it("draws open water in the unlit cell every voxel preset shares", () => {
    const p = buildSalmonRunPalette(LUMINOUS_DARK_COLORS, "bg-main");
    expect(p.bed[0]).toContain(voxelUnlitCellColor(LUMINOUS_DARK_COLORS));
  });

  it("takes its colours from the theme", () => {
    const plain = buildSalmonRunPalette(LUMINOUS_DARK_COLORS, "bg-main");
    const other = buildSalmonRunPalette(
      { ...LUMINOUS_DARK_COLORS, "color-accent": "#e0b020", "color-accent-hover": "#f0c040" },
      "bg-main",
    );
    expect(other.fishBody).not.toBe(plain.fishBody);
    expect(other.streak).not.toEqual(plain.streak);
  });

  it("lifts the water under each reduced-motion light step", () => {
    const p = buildSalmonRunPalette(LUMINOUS_DARK_COLORS, "bg-main");
    const water = p.bed.map((fills) => Number(/, ([\d.]+)\)$/.exec(fills.at(-1)!)![1]));
    for (let l = 1; l < water.length; l++) expect(water[l]).toBeGreaterThan(water[l - 1]);
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
});
