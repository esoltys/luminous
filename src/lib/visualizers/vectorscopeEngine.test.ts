import { describe, expect, it } from "vitest";
import type { ScopeFrame } from "../utils/scopeFeed";
import {
  LEVEL_FLOOR_DB,
  ScopeMeters,
  ScopeTrace,
  formatBalance,
  formatCorrelation,
  formatLevel,
} from "./vectorscopeEngine";

function scope(overrides: Partial<ScopeFrame> = {}): ScopeFrame {
  return {
    points: [],
    correlation: 0.5,
    momentaryLufs: -14,
    shortTermLufs: -15,
    truePeakDb: -3,
    balanceDb: 0,
    restarted: false,
    ...overrides,
  };
}

const LABELS = { left: "L", right: "R", centre: "C" };

describe("ScopeMeters", () => {
  it("holds the highest true peak until the meters restart", () => {
    const meters = new ScopeMeters();
    meters.push(scope({ truePeakDb: -1.5 }));
    meters.push(scope({ truePeakDb: -6 }));
    expect(meters.peakHoldDb).toBe(-1.5);

    meters.push(scope({ truePeakDb: -6, restarted: true }));
    expect(meters.peakHoldDb).toBe(-6);
  });

  it("samples short-term loudness into the history every sixth frame, newest first", () => {
    const meters = new ScopeMeters();
    for (let i = 0; i < 12; i++) meters.push(scope({ shortTermLufs: -20 + i }));
    expect(meters.historyCount).toBe(2);
    expect(meters.historyAgo(0)).toBe(-14);
    expect(meters.historyAgo(1)).toBe(-20);
  });

  it("clears the history when the meters restart", () => {
    const meters = new ScopeMeters();
    for (let i = 0; i < 12; i++) meters.push(scope());
    meters.push(scope({ restarted: true }));
    expect(meters.historyCount).toBe(1);
  });
});

describe("ScopeTrace", () => {
  it("draws mono as a vertical line and an inverted channel as a horizontal one", () => {
    const trace = new ScopeTrace();
    trace.set([0.5, 0.5]);
    expect(trace.points[0]).toBeCloseTo(0);
    expect(trace.points[1]).toBeGreaterThan(0);

    trace.set([0.5, -0.5]);
    expect(trace.points[0]).toBeLessThan(0);
    expect(trace.points[1]).toBeCloseTo(0);
  });

  it("lifts quiet material by at most 12 dB and leaves loud material alone", () => {
    const trace = new ScopeTrace();
    trace.set([0.01, 0.01]);
    expect(trace.gain).toBe(4);

    const loud = new ScopeTrace();
    loud.set([1, 1]);
    expect(loud.gain).toBe(1);
  });
});

describe("readout formatting", () => {
  it("uses a true minus sign and shows silence as a dash", () => {
    expect(formatLevel(-14.24)).toBe("−14.2");
    expect(formatLevel(0.3)).toBe("0.3");
    expect(formatLevel(-0.01)).toBe("0.0");
    expect(formatLevel(LEVEL_FLOOR_DB)).toBe("–");
  });

  it("names the louder side for balance and centres small differences", () => {
    expect(formatBalance(-1.24, LABELS)).toBe("L 1.2 dB");
    expect(formatBalance(2, LABELS)).toBe("R 2.0 dB");
    expect(formatBalance(0.04, LABELS)).toBe("C");
  });

  it("signs the correlation", () => {
    expect(formatCorrelation(0.873)).toBe("+0.87");
    expect(formatCorrelation(-0.5)).toBe("−0.50");
    expect(formatCorrelation(0)).toBe("0.00");
  });
});
