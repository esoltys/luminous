import { describe, expect, it } from "vitest";
import { LUMINOUS_DARK_COLORS, LUMINOUS_LIGHT_COLORS } from "../stores/theme.svelte";
import { calculateContrastRatio } from "../utils/colorUtils";
import { SALMON_RUN_PRESET, buildSalmonRunPalette } from "./salmonRun";

describe("buildSalmonRunPalette", () => {
  for (const [name, colors] of [
    ["dark", LUMINOUS_DARK_COLORS],
    ["light", LUMINOUS_LIGHT_COLORS],
  ] as const) {
    it(`keeps the salmon and leaf visible against the water in the ${name} theme`, () => {
      const p = buildSalmonRunPalette(colors, "bg-main", []);
      expect(calculateContrastRatio(p.fishBody, p.water)).toBeGreaterThanOrEqual(3);
      expect(calculateContrastRatio(p.leaf, p.water)).toBeGreaterThanOrEqual(3);
    });

    it(`keeps the playbar's lone salmon and leaf visible on the playbar in the ${name} theme`, () => {
      const p = buildSalmonRunPalette(colors, "bg-playerbar", [], false);
      const bar = colors["bg-playerbar"];
      expect(calculateContrastRatio(p.fishBody, bar)).toBeGreaterThanOrEqual(3);
      expect(calculateContrastRatio(p.leaf, bar)).toBeGreaterThanOrEqual(3);
    });
  }

  it("tints the water and current from the cover colours", () => {
    const plain = buildSalmonRunPalette(LUMINOUS_DARK_COLORS, "bg-main", []);
    const tinted = buildSalmonRunPalette(LUMINOUS_DARK_COLORS, "bg-main", ["#e0b020", "#803060"]);
    expect(tinted.water).not.toBe(plain.water);
    expect(tinted.streak).not.toBe(plain.streak);
  });

  it("ignores colours that aren't six-digit hex", () => {
    const plain = buildSalmonRunPalette(LUMINOUS_DARK_COLORS, "bg-main", []);
    expect(buildSalmonRunPalette(LUMINOUS_DARK_COLORS, "bg-main", ["red", ""])).toEqual(plain);
  });
});

describe("SALMON_RUN_PRESET", () => {
  it("creates an instance for both sizes that takes a track", () => {
    for (const size of ["full", "mini"] as const) {
      const instance = SALMON_RUN_PRESET.create(size);
      instance.setTheme(LUMINOUS_DARK_COLORS);
      instance.setTrack?.({ key: "1", artwork: [] });
      instance.setTrack?.(null);
      instance.push({ bands: new Array(32).fill(0.3), onset: 0, playing: true, scope: null });
      expect(instance.step(16, false)).toBe(true);
    }
  });
});
