import { describe, expect, it } from "vitest";
import {
  NEEDLE_MAX,
  Needle,
  VU_REFERENCE_DBFS,
  VuMeters,
  channelLevels,
  deflectionForRms,
  deflectionForVu,
} from "./vuMeterEngine";

/** Interleaved pairs of a constant-amplitude square wave, so RMS equals the amplitude. */
function square(l: number, r: number, pairs = 64): number[] {
  const out: number[] = [];
  for (let i = 0; i < pairs; i++) out.push(i % 2 ? -l : l, i % 2 ? -r : r);
  return out;
}

const REFERENCE_AMPLITUDE = 10 ** (VU_REFERENCE_DBFS / 20);

describe("deflection", () => {
  it("is zero for silence", () => {
    expect(deflectionForRms(0)).toBe(0);
    expect(deflectionForRms(NaN)).toBe(0);
  });

  it("puts 0 VU below the end of the scale and +5 VU at it", () => {
    expect(deflectionForRms(REFERENCE_AMPLITUDE)).toBeCloseTo(deflectionForVu(0), 6);
    expect(deflectionForVu(5)).toBeCloseTo(1, 6);
    expect(deflectionForVu(0)).toBeLessThan(deflectionForVu(3));
  });

  it("is linear in amplitude and pinned past the scale", () => {
    expect(deflectionForRms(REFERENCE_AMPLITUDE * 1.5)).toBeCloseTo(
      1.5 * deflectionForRms(REFERENCE_AMPLITUDE),
      6,
    );
    expect(deflectionForRms(1)).toBe(NEEDLE_MAX);
  });
});

describe("channelLevels", () => {
  it("reads each channel separately", () => {
    const { rmsL, rmsR, peak } = channelLevels(square(0.5, 0.1));
    expect(rmsL).toBeCloseTo(0.5, 6);
    expect(rmsR).toBeCloseTo(0.1, 6);
    expect(peak).toBeCloseTo(0.5, 6);
  });

  it("handles no data", () => {
    expect(channelLevels([])).toEqual({ rmsL: 0, rmsR: 0, peak: 0 });
  });
});

describe("Needle", () => {
  function settle(needle: Needle, ms: number): number {
    let peak = 0;
    for (let t = 0; t < ms; t += 5) {
      needle.step(5, false);
      peak = Math.max(peak, needle.position);
    }
    return peak;
  }

  it("reaches 99% of a step in about 300 ms with slight overshoot", () => {
    const needle = new Needle();
    needle.setTarget(0.5);
    const peak = settle(needle, 200);
    expect(needle.position).toBeLessThan(0.495);
    settle(needle, 300);
    expect(needle.position).toBeGreaterThan(0.495);
    expect(peak).toBeLessThan(0.5 * 1.03);
  });

  it("settles to rest and reports it", () => {
    const needle = new Needle();
    needle.setTarget(0.4);
    settle(needle, 2000);
    expect(needle.atRest).toBe(true);
  });

  it("never goes below the left stop", () => {
    const needle = new Needle();
    needle.setTarget(0.8);
    settle(needle, 600);
    needle.setTarget(0);
    for (let t = 0; t < 1500; t += 5) {
      needle.step(5, false);
      expect(needle.position).toBeGreaterThanOrEqual(0);
    }
  });

  it("jumps straight to the target when motion is reduced", () => {
    const needle = new Needle();
    needle.setTarget(0.6);
    needle.step(16, true);
    expect(needle.position).toBe(0.6);
    expect(needle.atRest).toBe(true);
  });
});

describe("VuMeters", () => {
  it("moves the channels independently", () => {
    const m = new VuMeters();
    m.push(square(REFERENCE_AMPLITUDE, 0));
    for (let i = 0; i < 100; i++) m.step(10, false);
    expect(m.left.position).toBeGreaterThan(0.4);
    expect(m.right.position).toBe(0);
  });

  it("moves 0 VU with the reference level", () => {
    const quiet = new VuMeters();
    const hot = new VuMeters();
    const signal = square(0.1, 0.1); // -20 dBFS RMS
    quiet.push(signal, -23); // 3 dB above 0 VU
    hot.push(signal, -14); // 6 dB below 0 VU
    for (let i = 0; i < 100; i++) {
      quiet.step(10, false);
      hot.step(10, false);
    }
    expect(quiet.left.position).toBeGreaterThan(deflectionForVu(0));
    expect(hot.left.position).toBeLessThan(deflectionForVu(0));
  });

  it("lights the lamps on peaks, holds, then fades out", () => {
    const m = new VuMeters();
    m.push(square(0.95, 0.95)); // about -0.4 dBFS: both lamps
    expect(m.lampLow.level).toBe(1);
    expect(m.lampHigh.level).toBe(1);
    m.step(200, false);
    expect(m.lampHigh.level).toBe(1);
    for (let i = 0; i < 100; i++) m.step(20, false);
    expect(m.lampHigh.level).toBe(0);
    expect(m.lampLow.level).toBe(0);
  });

  it("lights only the lower lamp for a moderate peak", () => {
    const m = new VuMeters();
    m.push(square(0.6, 0.6)); // about -4.4 dBFS
    expect(m.lampLow.level).toBe(1);
    expect(m.lampHigh.level).toBe(0);
  });

  it("turns lamps off without a fade under reduced motion", () => {
    const m = new VuMeters();
    m.push(square(0.95, 0.95));
    m.step(300, true);
    expect(m.lampHigh.level).toBe(0);
  });

  it("returns the needles to rest on silence and stops asking for frames", () => {
    const m = new VuMeters();
    m.push(square(REFERENCE_AMPLITUDE, REFERENCE_AMPLITUDE));
    for (let i = 0; i < 100; i++) m.step(10, false);
    m.rest();
    let busy = true;
    for (let i = 0; i < 400 && busy; i++) busy = m.step(10, false);
    expect(busy).toBe(false);
    expect(m.left.position).toBeCloseTo(0, 2);
  });
});
