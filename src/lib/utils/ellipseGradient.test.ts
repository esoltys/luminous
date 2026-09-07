import { describe, it, expect } from "vitest";
import { generateEllipseGradientSvg, ellipseGradientDataUri } from "./ellipseGradient";

describe("generateEllipseGradientSvg", () => {
  it("produces a well-formed SVG with a background rect and one gradient+rect per layer", () => {
    const svg = generateEllipseGradientSvg({ seed: 1, colors: ["#111111", "#222222"], layersPerColor: 2 });

    expect(svg.startsWith("<svg")).toBe(true);
    expect(svg.endsWith("</svg>")).toBe(true);
    expect((svg.match(/<radialGradient/g) ?? []).length).toBe(4); // 2 colors * 2 layers
    // +1 for the solid background rect
    expect((svg.match(/<rect/g) ?? []).length).toBe(5);
  });

  it("is deterministic for a given seed", () => {
    const a = generateEllipseGradientSvg({ seed: "album-42" });
    const b = generateEllipseGradientSvg({ seed: "album-42" });
    expect(a).toBe(b);
  });

  it("produces different output for different seeds", () => {
    const a = generateEllipseGradientSvg({ seed: "album-42" });
    const b = generateEllipseGradientSvg({ seed: "album-43" });
    expect(a).not.toBe(b);
  });

  it("gives every gradient a unique id, even across repeated calls with the same seed and no explicit run id", () => {
    const first = generateEllipseGradientSvg({ seed: 7 });
    const second = generateEllipseGradientSvg({ seed: 7 });
    // Same seed -> identical output (and therefore identical ids) is expected...
    expect(first).toBe(second);

    // ...but an unseeded call must not collide with a seeded one.
    const unseeded = generateEllipseGradientSvg();
    const idsIn = (svg: string) => Array.from(svg.matchAll(/id="([^"]+)"/g)).map((m) => m[1]);
    const seededIds = new Set(idsIn(first));
    const unseededIds = idsIn(unseeded);
    expect(unseededIds.some((id) => seededIds.has(id))).toBe(false);
  });

  it("respects a custom size for width/height/viewBox", () => {
    const svg = generateEllipseGradientSvg({ size: 300, seed: 1 });
    expect(svg).toContain('width="300"');
    expect(svg).toContain('height="300"');
    expect(svg).toContain('viewBox="0 0 300 300"');
  });

  it("falls back to the default palette when no colors are given", () => {
    const svg = generateEllipseGradientSvg({ seed: 1 });
    expect(svg).toMatch(/#5135FF|#FF5828|#F69CFF|#FFA50F/);
  });
});

describe("ellipseGradientDataUri", () => {
  it("returns a data URI wrapping the generated SVG", () => {
    const uri = ellipseGradientDataUri({ seed: 1 });
    expect(uri.startsWith("data:image/svg+xml,")).toBe(true);
    expect(decodeURIComponent(uri.replace("data:image/svg+xml,", ""))).toContain("<svg");
  });
});
