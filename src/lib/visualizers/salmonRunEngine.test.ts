import { describe, expect, it } from "vitest";
import {
  LEAP_S,
  RIPPLE_LIFE_S,
  RIPPLE_POOL,
  SalmonRun,
  SeededRandom,
  seedFromKey,
  type SalmonRunConfig,
} from "./salmonRunEngine";
import type { VisualizerFrame } from "./types";

const FULL: SalmonRunConfig = {
  fish: [5, 9],
  boulders: [4, 7],
  streaks: 20,
  pebbles: 10,
  fishLength: 0.075,
  leaf: true,
  leafSize: 0.065,
  leafCrossS: 20,
  ripples: true,
  transients: "leap",
  holdStation: false,
};

/** The playbar's lone salmon. */
const LONE: SalmonRunConfig = {
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
};

const QUIET = new Array(32).fill(0.2);
const LOUD_TREBLE = [...new Array(20).fill(0.2), ...new Array(12).fill(1)];

function frame(bands: readonly number[], onset = 0, playing = true): VisualizerFrame {
  return { bands, onset, playing, scope: null };
}

/** Steps `river` for `seconds` at 60 fps; returns the last step's result. */
function run(river: SalmonRun, seconds: number, reducedMotion = false): boolean {
  let moving = true;
  for (let t = 0; t < seconds * 60; t++) moving = river.step(1000 / 60, reducedMotion);
  return moving;
}

function boulders(river: SalmonRun): number[] {
  return Array.from({ length: river.boulderCount }, (_, i) => [
    river.boulderFX[i],
    river.boulderY[i],
    river.boulderR[i],
  ]).flat();
}

function activeRipples(river: SalmonRun): number {
  let n = 0;
  for (let i = 0; i < RIPPLE_POOL; i++) if (river.rippleStrength[i] > 0) n++;
  return n;
}

function leaping(river: SalmonRun): boolean {
  for (let i = 0; i < river.fishCount; i++) if (river.fishLeap[i] > 0) return true;
  return false;
}

describe("SeededRandom", () => {
  it("repeats the same sequence for the same seed", () => {
    const a = new SeededRandom(seedFromKey("42"));
    const b = new SeededRandom(seedFromKey("42"));
    for (let i = 0; i < 20; i++) expect(a.next()).toBe(b.next());
  });

  it("keeps int() within its inclusive bounds", () => {
    const rng = new SeededRandom(7);
    for (let i = 0; i < 500; i++) {
      const v = rng.int(5, 9);
      expect(v).toBeGreaterThanOrEqual(5);
      expect(v).toBeLessThanOrEqual(9);
    }
  });
});

describe("SalmonRun layout", () => {
  it("lays out the same river for the same track and a different one for the next", () => {
    const a = new SalmonRun(FULL);
    const b = new SalmonRun(FULL);
    a.setTrack("101");
    b.setTrack("101");
    expect(boulders(a)).toEqual(boulders(b));
    expect(a.fishCount).toBe(b.fishCount);

    b.setTrack("202");
    expect(boulders(b)).not.toEqual(boulders(a));
  });

  it("keeps the school and boulders within the configured counts", () => {
    for (const key of ["1", "2", "3", "4", "5", "6"]) {
      const river = new SalmonRun(FULL);
      river.setTrack(key);
      expect(river.fishCount).toBeGreaterThanOrEqual(5);
      expect(river.fishCount).toBeLessThanOrEqual(9);
      expect(river.boulderCount).toBeLessThanOrEqual(7);
    }
  });

  it("sends a leaf downstream on a track change but not for the first track", () => {
    const river = new SalmonRun(FULL);
    river.setTrack("1");
    expect(river.leafActive).toBe(false);
    river.setTrack("1");
    expect(river.leafActive).toBe(false);
    river.setTrack("2");
    expect(river.leafActive).toBe(true);
    const startX = river.leafX;
    run(river, 1);
    expect(river.leafX).toBeGreaterThan(startX);
    run(river, FULL.leafCrossS + 1);
    expect(river.leafActive).toBe(false);
  });

  it("drifts the leaf across in about its crossing time, whatever the canvas shape", () => {
    const river = new SalmonRun(LONE);
    river.setAspect(3.7);
    river.setTrack("1");
    river.setTrack("2");
    run(river, LONE.leafCrossS * 0.9);
    expect(river.leafActive).toBe(true);
    run(river, LONE.leafCrossS * 0.2);
    expect(river.leafActive).toBe(false);
  });

  it("drops no leaf when the size has none", () => {
    const river = new SalmonRun({ ...FULL, leaf: false });
    river.setTrack("1");
    river.setTrack("2");
    expect(river.leafActive).toBe(false);
  });

  it("keeps things in place across the width when the canvas reshapes", () => {
    const river = new SalmonRun(FULL);
    river.setTrack("1");
    river.setAspect(2);
    const fraction = river.fishX[0] / river.aspect;
    river.setAspect(4);
    expect(river.fishX[0] / river.aspect).toBeCloseTo(fraction, 5);
  });
});

describe("SalmonRun motion", () => {
  it("swims the school upstream while playing", () => {
    const river = new SalmonRun(FULL);
    river.setTrack("1");
    river.setAspect(16 / 9);
    river.push(frame(QUIET));
    // Through crowding and boulders alike, nobody drifts downstream.
    for (let t = 0; t < 600; t++) {
      river.step(1000 / 60, false);
      if (t < 120) continue;
      for (let i = 0; i < river.fishCount; i++) expect(river.fishVX[i]).toBeLessThan(0);
    }
  });

  it("calms and stops asking for frames once paused", () => {
    const river = new SalmonRun(FULL);
    river.setTrack("1");
    river.push(frame(QUIET));
    expect(run(river, 2)).toBe(true);
    river.push(frame(QUIET, 0, false));
    expect(run(river, 6)).toBe(false);
    expect(river.activity).toBe(0);
  });

  it("eases back in on resume rather than jumping to full speed", () => {
    const river = new SalmonRun(FULL);
    river.setTrack("1");
    river.push(frame(QUIET));
    river.step(1000 / 60, false);
    expect(river.activity).toBeGreaterThan(0);
    expect(river.activity).toBeLessThan(0.1);
  });

  it("drops a ripple on each beat, capped at the pool size", () => {
    const river = new SalmonRun(FULL);
    river.setTrack("1");
    for (let k = 0; k < RIPPLE_POOL + 5; k++) {
      river.push(frame(QUIET, 0.5));
      river.step(1000 / 60, false);
    }
    expect(activeRipples(river)).toBe(RIPPLE_POOL);
    river.push(frame(QUIET, 0, false));
    run(river, RIPPLE_LIFE_S + 0.2);
    expect(activeRipples(river)).toBe(0);
    // Steady quiet music is no transient, so nobody leapt.
    expect(leaping(river)).toBe(false);
  });

  it("makes a salmon leap on a sharp treble transient, with a splash", () => {
    const river = new SalmonRun(FULL);
    river.setTrack("1");
    river.push(frame(QUIET));
    run(river, 1);
    river.push(frame(LOUD_TREBLE));
    river.step(1000 / 60, false);
    expect(leaping(river)).toBe(true);
    expect(activeRipples(river)).toBeGreaterThan(0);
    run(river, LEAP_S + 0.1);
    expect(leaping(river)).toBe(false);
  });

  it("never leaps, surges or ripples under reduced motion, and shifts the light instead", () => {
    const river = new SalmonRun(FULL);
    river.setTrack("1");
    river.push(frame(QUIET));
    run(river, 1, true);
    river.push(frame(LOUD_TREBLE, 1));
    river.step(1000 / 60, true);
    expect(leaping(river)).toBe(false);
    for (let i = 0; i < river.fishCount; i++) expect(river.fishSurge[i]).toBe(0);
    expect(activeRipples(river)).toBe(0);
    expect(river.glow).toBeGreaterThan(0);
  });

  it("keeps a station-holding salmon in frame, facing into the current", () => {
    const river = new SalmonRun(LONE);
    river.setAspect(3.7);
    river.setTrack("1");
    expect(river.fishCount).toBe(1);
    river.push(frame(QUIET));
    for (let t = 0; t < 60 * 60; t++) {
      if (t % 90 === 0) river.push(frame(t % 180 === 0 ? LOUD_TREBLE : QUIET, 0.9));
      river.step(1000 / 60, false);
      expect(river.fishX[0]).toBeGreaterThan(LONE.fishLength * 0.5);
      expect(river.fishX[0]).toBeLessThan(river.aspect - LONE.fishLength * 0.5);
      expect(river.fishY[0]).toBeGreaterThan(0.25);
      expect(river.fishY[0]).toBeLessThan(0.75);
    }
  });

  it("darts a station-holding salmon forward on a transient, without leaping or rippling", () => {
    const river = new SalmonRun(LONE);
    river.setAspect(3.7);
    river.setTrack("1");
    river.push(frame(QUIET));
    run(river, 2);
    river.push(frame(LOUD_TREBLE, 1));
    river.step(1000 / 60, false);
    expect(river.fishSurge[0]).toBeGreaterThan(0);
    expect(leaping(river)).toBe(false);
    expect(activeRipples(river)).toBe(0);
    const before = river.fishVX[0];
    run(river, 0.2);
    expect(river.fishVX[0]).toBeLessThan(before);
  });
});
