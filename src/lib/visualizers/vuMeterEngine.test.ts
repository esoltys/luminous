import { describe, expect, it } from "vitest";
import {
  NEEDLE_MAX,
  Needle,
  VU_REFERENCE_RANGE_DBFS,
  VU_REFERENCE_DBFS,
  VuMeters,
  channelLevels,
  deflectionForPercent,
  deflectionForRms,
  deflectionForVu,
  referenceForLoudness,
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

  it("follows the K-550 scale: 0 VU about 70% across, +5 VU at the end", () => {
    expect(deflectionForVu(-20)).toBe(0);
    expect(deflectionForVu(0)).toBeCloseTo(0.696, 6);
    expect(deflectionForVu(5)).toBeCloseTo(1, 6);
    expect(deflectionForRms(REFERENCE_AMPLITUDE)).toBeCloseTo(deflectionForVu(0), 6);
  });

  it("rises steadily through the scale and is pinned past it", () => {
    let last = -1;
    for (let vu = -20; vu <= 5; vu += 0.5) {
      const f = deflectionForVu(vu);
      expect(f).toBeGreaterThanOrEqual(last);
      last = f;
    }
    expect(deflectionForVu(7)).toBeGreaterThan(1);
    expect(deflectionForRms(1)).toBe(NEEDLE_MAX);
  });

  it("keeps music at the normalization target out of the red", () => {
    // -16 LUFS stereo is about -19 dBFS RMS per channel; the reference leaves headroom above that.
    const atTarget = 10 ** (-19 / 20);
    expect(deflectionForRms(atTarget, referenceForLoudness(true, -16))).toBeLessThan(
      deflectionForVu(-3),
    );
  });
});

describe("reference level", () => {
  it("follows the target only while normalization is on", () => {
    expect(referenceForLoudness(true, -16)).toBeLessThan(referenceForLoudness(true, -9));
    expect(referenceForLoudness(false, -16)).toBe(referenceForLoudness(false, -23));
  });

  it("uses a manual level over either automatic one, within the allowed range", () => {
    expect(referenceForLoudness(true, -16, -10)).toBe(-10);
    expect(referenceForLoudness(false, -16, -10)).toBe(-10);
    expect(referenceForLoudness(true, -16, -90)).toBe(VU_REFERENCE_RANGE_DBFS.min);
    expect(referenceForLoudness(true, -16, 5)).toBe(VU_REFERENCE_RANGE_DBFS.max);
  });

  it("moves 0 VU with the manual level", () => {
    const rms = 10 ** (-12 / 20);
    // Calibrated so -12 dBFS reads 0 VU, then 6 dB lower.
    expect(deflectionForRms(rms, referenceForLoudness(true, -16, -12))).toBeCloseTo(
      deflectionForVu(0),
      6,
    );
    expect(deflectionForRms(rms, referenceForLoudness(true, -16, -6))).toBeCloseTo(
      deflectionForVu(-6),
      6,
    );
  });

  it("keeps a typical unnormalized master out of the red", () => {
    const reference = referenceForLoudness(false, -16);
    // -9 LUFS stereo is about -12 dBFS RMS per channel.
    expect(deflectionForRms(10 ** (-12 / 20), reference)).toBeLessThan(deflectionForVu(-3));
    // A very loud -6 LUFS master still stays below 0 VU on average.
    expect(deflectionForRms(10 ** (-9 / 20), reference)).toBeLessThan(deflectionForVu(0));
  });
});

describe("percentage scale", () => {
  it("puts 100% at 0 VU and 0% at the left stop", () => {
    expect(deflectionForPercent(100)).toBeCloseTo(deflectionForVu(0), 6);
    expect(deflectionForPercent(0)).toBe(0);
  });

  it("is spaced by voltage, so the marks are not evenly spread", () => {
    // 50% of the voltage is 6.02 dB down.
    expect(deflectionForPercent(50)).toBeCloseTo(deflectionForVu(-6.0206), 4);
    const gaps = [20, 40, 60, 80, 100].map(
      (p, i, a) => deflectionForPercent(p) - deflectionForPercent(i ? a[i - 1] : 0),
    );
    expect(new Set(gaps.map((g) => g.toFixed(3))).size).toBe(gaps.length);
  });
});

describe("channelLevels", () => {
  it("reads each channel separately", () => {
    const { rmsL, rmsR } = channelLevels(square(0.5, 0.1));
    expect(rmsL).toBeCloseTo(0.5, 6);
    expect(rmsR).toBeCloseTo(0.1, 6);
  });

  it("handles no data", () => {
    expect(channelLevels([])).toEqual({ rmsL: 0, rmsR: 0 });
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
    m.push(square(1, 1)); // full scale: both lamps
    expect(m.lampLow.level).toBe(1);
    expect(m.lampHigh.level).toBe(1);
    m.step(200, false);
    expect(m.lampHigh.level).toBe(1);
    for (let i = 0; i < 100; i++) m.step(20, false);
    expect(m.lampHigh.level).toBe(0);
    expect(m.lampLow.level).toBe(0);
  });

  it("lights only the lower lamp for a loud but unpinned level", () => {
    const m = new VuMeters();
    m.push(square(0.35, 0.35)); // -9.1 dBFS RMS: +3.9 VU at the default reference, short of the pin
    expect(m.lampLow.level).toBe(1);
    expect(m.lampHigh.level).toBe(0);
  });

  it("does not light the +3 lamp for a quiet frame with one full-scale-ish sample", () => {
    const m = new VuMeters();
    const points = square(0.05, 0.05);
    points[10] = 0.99; // a lone spike: a limited master's ceiling, not a loud passage
    m.push(points);
    expect(m.lampLow.level).toBe(0);
    expect(m.lampHigh.level).toBe(0);
  });

  it("turns lamps off without a fade under reduced motion", () => {
    const m = new VuMeters();
    m.push(square(1, 1));
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
