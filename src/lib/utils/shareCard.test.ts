import { describe, it, expect } from "vitest";
import { buildShareCardSvg, SHARE_ASPECT_RATIOS } from "./shareCard";

const baseOptions = {
  theme: "dark" as const,
  seed: "album-1",
  coverDataUri: null,
  title: "Test Album",
  subtitle: "Test Artist",
  metadataLine: "2024 • 10 songs • 42m",
  includeTrackList: true,
};

describe("buildShareCardSvg", () => {
  it("renders each aspect ratio at its declared pixel dimensions", () => {
    for (const ratio of SHARE_ASPECT_RATIOS) {
      const { svg, width, height } = buildShareCardSvg({ ...baseOptions, aspectRatio: ratio.id });
      expect(width).toBe(ratio.width);
      expect(height).toBe(ratio.height);
      expect(svg).toContain(`width="${ratio.width}"`);
      expect(svg).toContain(`height="${ratio.height}"`);
    }
  });

  it("escapes title/subtitle text to avoid breaking the embedded HTML", () => {
    const { svg } = buildShareCardSvg({
      ...baseOptions,
      aspectRatio: "1:1",
      title: '<script>alert("x")</script>',
    });
    expect(svg).not.toContain("<script>alert");
    expect(svg).toContain("&lt;script&gt;");
  });

  it("omits the track list block when includeTrackList is false", () => {
    const { svg } = buildShareCardSvg({
      ...baseOptions,
      aspectRatio: "1:1",
      includeTrackList: false,
      tracks: [{ number: 1, title: "Opening Track" }],
    });
    expect(svg).not.toContain("Opening Track");
  });

  it("caps visible tracks and shows a +N more overflow row", () => {
    const tracks = Array.from({ length: 20 }, (_, i) => ({ number: i + 1, title: `Track ${i + 1}` }));
    const { svg } = buildShareCardSvg({ ...baseOptions, aspectRatio: "16:9", tracks });
    expect(svg).toContain("Track 1<");
    expect(svg).toMatch(/\+\d+ more/);
  });

  it("is deterministic for a given seed", () => {
    const a = buildShareCardSvg({ ...baseOptions, aspectRatio: "1:1" });
    const b = buildShareCardSvg({ ...baseOptions, aspectRatio: "1:1" });
    expect(a.svg).toBe(b.svg);
  });
});
