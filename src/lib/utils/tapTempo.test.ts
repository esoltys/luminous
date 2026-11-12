import { describe, expect, it } from "vitest";
import { registerTap, TAP_RESET_MS } from "./tapTempo";

function tapAll(times: number[]) {
  let taps: number[] = [];
  let bpm: number | null = null;
  for (const t of times) ({ taps, bpm } = registerTap(taps, t));
  return { taps, bpm };
}

describe("registerTap", () => {
  it("reports nothing for a single tap", () => {
    expect(tapAll([1000]).bpm).toBeNull();
  });

  it("reads 120 BPM from 500 ms intervals", () => {
    expect(tapAll([0, 500, 1000, 1500]).bpm).toBe(120);
  });

  it("uses the median so one stray tap does not skew the tempo", () => {
    expect(tapAll([0, 500, 1000, 1500, 2300, 2800]).bpm).toBe(120);
  });

  it("starts over after a pause longer than the reset window", () => {
    const { taps, bpm } = tapAll([
      0,
      500,
      1000,
      1000 + TAP_RESET_MS + 1,
      1000 + TAP_RESET_MS + 1 + 400,
    ]);
    expect(taps).toHaveLength(2);
    expect(bpm).toBe(150);
  });

  it("keeps counting when the pause is exactly the reset window", () => {
    expect(tapAll([0, TAP_RESET_MS]).bpm).toBe(30);
  });

  it("only considers the most recent taps", () => {
    const slow = Array.from({ length: 12 }, (_, i) => i * 1000);
    const start = slow[slow.length - 1];
    const fast = Array.from({ length: 12 }, (_, i) => start + (i + 1) * 500);
    expect(tapAll([...slow, ...fast]).bpm).toBe(120);
  });

  it("ignores simultaneous taps and absurd tempos", () => {
    expect(tapAll([100, 100]).bpm).toBeNull();
    expect(tapAll([0, 100]).bpm).toBeNull(); // 600 BPM
  });
});
