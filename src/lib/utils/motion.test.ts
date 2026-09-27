import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";

function stubReducedMotion(reduce: boolean) {
  vi.stubGlobal("matchMedia", (query: string) => ({
    matches: reduce && query.includes("prefers-reduced-motion: reduce"),
    media: query,
    addEventListener: () => {},
    removeEventListener: () => {},
  }));
}

// The transition functions return a `css(t, u)` generator; sampling it
// mid-way shows which properties the transition actually moves.
function cssAt(config: { css?: (t: number, u: number) => string }, t = 0.5): string {
  return config.css?.(t, 1 - t) ?? "";
}

describe("motion transitions", () => {
  let node: HTMLElement;

  beforeEach(() => {
    // The media query is created once per module instance.
    vi.resetModules();
    node = document.createElement("div");
    document.body.appendChild(node);
  });

  afterEach(() => {
    node.remove();
    vi.unstubAllGlobals();
  });

  describe("without reduced motion", () => {
    beforeEach(() => stubReducedMotion(false));

    it("fly moves the element", async () => {
      const { fly } = await import("./motion");
      const config = fly(node, { x: 24, duration: 200 });
      expect(config.duration).toBe(200);
      expect(cssAt(config)).toContain("transform");
    });

    it("scale scales the element", async () => {
      const { scale } = await import("./motion");
      expect(cssAt(scale(node, { start: 0.97, duration: 200 }))).toContain("scale(");
    });

    it("slide keeps its duration", async () => {
      const { slide } = await import("./motion");
      expect(slide(node, { axis: "x", duration: 250 }).duration).toBe(250);
    });
  });

  describe("with reduced motion", () => {
    beforeEach(() => stubReducedMotion(true));

    it("reports the preference", async () => {
      const { prefersReducedMotion } = await import("./motion");
      expect(prefersReducedMotion()).toBe(true);
    });

    it("fly becomes an opacity-only fade of the same duration and delay", async () => {
      const { fly } = await import("./motion");
      const config = fly(node, { x: 24, y: 40, duration: 200, delay: 50 });
      expect(config.duration).toBe(200);
      expect(config.delay).toBe(50);
      const css = cssAt(config);
      expect(css).toContain("opacity");
      expect(css).not.toContain("transform");
    });

    it("scale becomes an opacity-only fade", async () => {
      const { scale } = await import("./motion");
      const css = cssAt(scale(node, { start: 0.97, duration: 200 }));
      expect(css).toContain("opacity");
      expect(css).not.toContain("transform");
    });

    it("slide changes size instantly", async () => {
      const { slide } = await import("./motion");
      expect(slide(node, { axis: "x", duration: 250 }).duration).toBe(0);
    });
  });

  it("treats a missing matchMedia (non-browser) as no preference", async () => {
    vi.stubGlobal("matchMedia", undefined);
    const { prefersReducedMotion } = await import("./motion");
    expect(prefersReducedMotion()).toBe(false);
  });
});
