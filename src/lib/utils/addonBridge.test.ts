import { describe, it, expect } from "vitest";
import { addonFrameUrl, spectrumMessage } from "./addonBridge";

describe("addonFrameUrl", () => {
  it("uses the http .localhost form on Windows and the custom scheme elsewhere", () => {
    expect(addonFrameUrl("fixture-addon", "overlay.html", true)).toBe(
      "http://luminous-addon.localhost/fixture-addon/overlay.html"
    );
    expect(addonFrameUrl("fixture-addon", "overlay.html", false)).toBe(
      "luminous-addon://localhost/fixture-addon/overlay.html"
    );
  });

  it("encodes each path segment but keeps the separators", () => {
    expect(addonFrameUrl("a", "dir/my overlay.html", false)).toBe(
      "luminous-addon://localhost/a/dir/my%20overlay.html"
    );
  });
});

describe("spectrumMessage", () => {
  it("averages bass, mid and treble over the logo's bin ranges", () => {
    const bins = [
      ...Array(8).fill(1),
      ...Array(12).fill(0.5),
      ...Array(12).fill(0.25)
    ];
    const msg = spectrumMessage(bins);
    expect(msg).toMatchObject({ kind: "spectrum", bass: 1, mid: 0.5, treble: 0.25 });
    expect((msg as { bars: number[] }).bars).toHaveLength(32);
  });

  it("clamps out-of-range and non-finite bins", () => {
    const msg = spectrumMessage([2, -1, NaN]) as { bass: number; bars: number[] };
    expect(msg.bars).toEqual([1, 0, 0]);
    expect(msg.bass).toBeGreaterThanOrEqual(0);
    expect(msg.bass).toBeLessThanOrEqual(1);
  });
});
