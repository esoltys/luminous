import { describe, expect, it } from "vitest";
import {
  DROP,
  FishLadder,
  LEAP_S,
  MAX_POOLS,
  NOTCH_WIDTH,
  PATIENCE_S,
  MAX_RIPPLES,
  RIVER_WIDTH,
} from "./fishLadderEngine";
import type { VisualizerFrame } from "./types";

const QUIET = new Array(32).fill(0.2);
const LOUD_TREBLE = [...new Array(20).fill(0.2), ...new Array(12).fill(1)];
/** Treble just over the transient threshold: a transient too weak to clear a weir. */
const NUDGE_TREBLE = [...new Array(20).fill(0.2), ...new Array(12).fill(0.36)];
const FRAME_MS = 1000 / 60;

function frame(bands: readonly number[], onset = 0, playing = true): VisualizerFrame {
  return { bands, onset, playing, scope: null };
}

/** Steps `ladder` for `seconds` at 60 fps; returns the last step's result. */
function run(ladder: FishLadder, seconds: number, reducedMotion = false): boolean {
  let moving = true;
  for (let t = 0; t < seconds * 60; t++) moving = ladder.step(FRAME_MS, reducedMotion);
  return moving;
}

function layout(ladder: FishLadder): number[] {
  const out: number[] = [ladder.poolCount, ladder.boulderCount, ladder.fishCount];
  for (let k = 0; k < ladder.poolCount; k++) {
    out.push(ladder.poolStart[k], ladder.poolEnd[k], ladder.notchY[k]);
  }
  for (let b = 0; b < ladder.boulderCount; b++) out.push(ladder.boulderX[b], ladder.boulderY[b]);
  return out;
}

function activeRipples(ladder: FishLadder): number {
  let n = 0;
  for (let i = 0; i < MAX_RIPPLES; i++) if (ladder.rippleStrength[i] > 0) n++;
  return n;
}

function leapers(ladder: FishLadder): number[] {
  const out: number[] = [];
  for (let i = 0; i < ladder.fishCount; i++) if (ladder.fishLeapS[i] > 0) out.push(i);
  return out;
}

/** A ladder playing quiet music until its in-flight starting leap has landed. */
function settled(key = "a"): FishLadder {
  const ladder = new FishLadder();
  ladder.setTrack(key);
  ladder.push(frame(QUIET));
  run(ladder, LEAP_S + 0.1);
  expect(leapers(ladder)).toEqual([]);
  return ladder;
}

/** The salmon waiting at a weir, ready to leap, if any. */
function waiting(ladder: FishLadder): number {
  for (let i = 0; i < ladder.fishCount; i++) if (ladder.fishWait[i] > 0) return i;
  return -1;
}

describe("FishLadder layout", () => {
  it("lays out the same ladder for the same track and a different one for another", () => {
    const a = new FishLadder();
    a.setTrack("track-1");
    const b = new FishLadder();
    b.setTrack("track-1");
    expect(layout(a)).toEqual(layout(b));
    b.setTrack("track-2");
    expect(layout(b)).not.toEqual(layout(a));
  });

  it("steps each pool down toward the viewer, end to end", () => {
    for (const key of ["a", "b", "c", "d", "e", "f"]) {
      const ladder = new FishLadder();
      ladder.setTrack(key);
      expect(ladder.poolCount).toBeGreaterThanOrEqual(4);
      expect(ladder.poolCount).toBeLessThanOrEqual(MAX_POOLS);
      expect(ladder.poolStart[0]).toBe(0);
      for (let k = 1; k < ladder.poolCount; k++) {
        expect(ladder.poolStart[k]).toBe(ladder.poolEnd[k - 1]);
        expect(ladder.poolSurface[k - 1] - ladder.poolSurface[k]).toBe(DROP);
      }
      expect(ladder.length).toBe(ladder.poolEnd[ladder.poolCount - 1]);
      for (let k = 0; k < ladder.poolCount; k++) {
        expect(ladder.notchY[k]).toBeGreaterThanOrEqual(0);
        expect(ladder.notchY[k] + NOTCH_WIDTH).toBeLessThanOrEqual(RIVER_WIDTH);
      }
    }
  });

  it("starts with a salmon mid-leap and another waiting, so the first frame shows the climb", () => {
    const ladder = new FishLadder();
    ladder.setTrack("a");
    expect(leapers(ladder)).toEqual([0]);
    expect(ladder.fishZ[0]).toBeGreaterThan(ladder.poolSurface[ladder.fishPool[0]]);
    expect(activeRipples(ladder)).toBe(0);
  });

  it("floats a leaf down the ladder on a track change, but not for the first track", () => {
    const ladder = new FishLadder();
    ladder.setTrack("a");
    expect(ladder.leafActive).toBe(false);
    ladder.setTrack("a");
    expect(ladder.leafActive).toBe(false);
    ladder.setTrack("b");
    expect(ladder.leafActive).toBe(true);
    let lowest = ladder.leafZ;
    for (let t = 0; t < 30 * 60 && ladder.leafActive; t++) {
      ladder.step(FRAME_MS, false);
      expect(ladder.leafZ).toBeLessThanOrEqual(lowest + 1e-6);
      lowest = ladder.leafZ;
    }
    expect(ladder.leafActive).toBe(false);
    expect(lowest).toBe(ladder.poolSurface[ladder.poolCount - 1]);
  });
});

describe("FishLadder climbing", () => {
  it("only moves a salmon up the ladder by leaping one step, or back to the foot from the top", () => {
    for (const key of ["a", "b", "c"]) {
      const ladder = new FishLadder();
      ladder.setTrack(key);
      const pool = Array.from(ladder.fishPool.slice(0, ladder.fishCount));
      let climbs = 0;
      for (let t = 0; t < 90 * 60; t++) {
        ladder.push(frame(t % 40 === 0 ? LOUD_TREBLE : QUIET, t % 30 === 0 ? 0.8 : 0));
        const wasLeaping = Array.from(
          { length: ladder.fishCount },
          (_, i) => ladder.fishLeapS[i] > 0,
        );
        ladder.step(FRAME_MS, false);
        for (let i = 0; i < ladder.fishCount; i++) {
          const k = ladder.fishPool[i];
          if (k !== pool[i]) {
            if (k === pool[i] - 1) {
              expect(wasLeaping[i]).toBe(true);
              expect(ladder.fishLeapS[i]).toBe(0);
              climbs++;
            } else {
              // Out of the top pool, rejoining below the bottom one.
              expect([pool[i], k]).toEqual([0, ladder.poolCount - 1]);
              expect(ladder.fishX[i]).toBeGreaterThan(ladder.length);
            }
            pool[i] = k;
          }
          if (ladder.fishLeapS[i] === 0) {
            // A swimming salmon is in its own pool's water (or still coming in at either end).
            expect(ladder.fishZ[i]).toBe(ladder.poolSurface[k]);
            if (k > 0) expect(ladder.fishX[i]).toBeGreaterThan(ladder.poolStart[k]);
            if (k < ladder.poolCount - 1) expect(ladder.fishX[i]).toBeLessThan(ladder.poolEnd[k]);
            expect(ladder.fishY[i]).toBeGreaterThan(0);
            expect(ladder.fishY[i]).toBeLessThan(RIVER_WIDTH);
          }
        }
      }
      expect(climbs).toBeGreaterThan(3);
    }
  });

  it("keeps climbing to quiet music, a salmon leaping once it has waited long enough", () => {
    const ladder = settled();
    const start = Array.from(ladder.fishPool.slice(0, ladder.fishCount));
    let climbed = false;
    for (let t = 0; t < (PATIENCE_S[1] + 4) * 60 && !climbed; t++) {
      ladder.step(FRAME_MS, false);
      for (let i = 0; i < ladder.fishCount; i++) if (ladder.fishPool[i] < start[i]) climbed = true;
    }
    expect(climbed).toBe(true);
  });

  it("sends the waiting salmon up a step on a strong transient", () => {
    const ladder = settled();
    run(ladder, 1);
    const i = waiting(ladder);
    expect(i).toBeGreaterThanOrEqual(0);
    const k = ladder.fishPool[i];
    ladder.push(frame(LOUD_TREBLE));
    ladder.step(FRAME_MS, false);
    expect(leapers(ladder)).toEqual([i]);
    expect(activeRipples(ladder)).toBeGreaterThan(0);
    run(ladder, LEAP_S + 0.1);
    expect(ladder.fishPool[i]).toBe(k - 1);
  });

  it("drops a salmon back into its pool when the transient is too weak to clear the weir", () => {
    const ladder = settled();
    run(ladder, 1);
    const i = waiting(ladder);
    const k = ladder.fishPool[i];
    ladder.push(frame(NUDGE_TREBLE));
    ladder.step(FRAME_MS, false);
    expect(leapers(ladder)).toEqual([i]);
    let highest = 0;
    for (let t = 0; t < LEAP_S * 60; t++) {
      ladder.step(FRAME_MS, false);
      highest = Math.max(highest, ladder.fishZ[i]);
    }
    expect(ladder.fishPool[i]).toBe(k);
    expect(highest).toBeGreaterThan(ladder.poolSurface[k]);
    expect(highest).toBeLessThan(ladder.poolSurface[k - 1]);
  });

  it("never leaps or ripples under reduced motion, and shifts the light instead", () => {
    const ladder = new FishLadder();
    ladder.setTrack("a");
    const start = Array.from(ladder.fishPool.slice(0, ladder.fishCount));
    ladder.step(FRAME_MS, true);
    // The starting leap lands at once rather than being shown in flight.
    expect(leapers(ladder)).toEqual([]);
    start[0]--;
    for (let s = 0; s < 20; s++) {
      ladder.push(frame(LOUD_TREBLE, 1));
      ladder.step(FRAME_MS, true);
      expect(ladder.glow).toBeGreaterThan(0);
      ladder.push(frame(QUIET));
      run(ladder, 1, true);
      expect(leapers(ladder)).toEqual([]);
      expect(activeRipples(ladder)).toBe(0);
    }
    for (let i = 0; i < ladder.fishCount; i++) {
      // A salmon may only have left the top pool to rejoin at the foot.
      if (ladder.fishPool[i] !== start[i]) expect(start[i]).toBe(0);
    }
  });
});

describe("FishLadder motion", () => {
  it("drops a ripple at the foot of a fall on each beat, capped at the pool size", () => {
    const ladder = settled();
    run(ladder, 1.5);
    for (let s = 0; s < MAX_RIPPLES + 4; s++) {
      ladder.push(frame(QUIET, 0.6));
      ladder.step(FRAME_MS, false);
    }
    expect(activeRipples(ladder)).toBe(MAX_RIPPLES);
  });

  it("calms and stops asking for frames once paused, and eases back in on resume", () => {
    const ladder = settled();
    expect(run(ladder, 2)).toBe(true);
    ladder.push(frame(QUIET, 0, false));
    expect(run(ladder, 6)).toBe(false);
    const x = Array.from(ladder.fishX.slice(0, ladder.fishCount));
    ladder.push(frame(QUIET));
    ladder.step(FRAME_MS, false);
    for (let i = 0; i < ladder.fishCount; i++) {
      expect(Math.abs(ladder.fishX[i] - x[i])).toBeLessThan(0.01);
    }
  });

  it("brightens the falls with the music and holds them still under reduced motion", () => {
    const ladder = settled();
    const phase = ladder.fallPhase;
    run(ladder, 1);
    expect(ladder.fallPhase).not.toBe(phase);
    const still = ladder.fallPhase;
    run(ladder, 1, true);
    expect(ladder.fallPhase).toBe(still);
  });
});
