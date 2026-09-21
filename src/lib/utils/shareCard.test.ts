import { describe, it, expect } from "vitest";
import { buildShareCardSvg, buildStatsShareCardSvg, SHARE_ASPECT_RATIOS } from "./shareCard";

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

  it("shows a track's secondary text (e.g. artist) alongside its title, for playlist cards spanning multiple artists", () => {
    const { svg } = buildShareCardSvg({
      ...baseOptions,
      aspectRatio: "1:1",
      tracks: [{ number: 1, title: "Opening Track", secondary: "Some Artist" }],
    });
    expect(svg).toContain("Opening Track");
    expect(svg).toContain("Some Artist");
  });

  it("caps visible tracks and shows a +N more overflow row", () => {
    const tracks = Array.from({ length: 20 }, (_, i) => ({ number: i + 1, title: `Track ${i + 1}` }));
    const { svg } = buildShareCardSvg({ ...baseOptions, aspectRatio: "16:9", tracks });
    expect(svg).toContain("Track 1<");
    expect(svg).toMatch(/\+\d+ more/);
  });

  it("fans a long track list out into multiple CSS columns, more on wider frames", () => {
    const tracks = Array.from({ length: 20 }, (_, i) => ({ number: i + 1, title: `Track ${i + 1}` }));
    const landscape = buildShareCardSvg({ ...baseOptions, aspectRatio: "16:9", tracks });
    const portrait = buildShareCardSvg({ ...baseOptions, aspectRatio: "9:16", tracks });
    expect(landscape.svg).toContain("column-count:3");
    expect(portrait.svg).toContain("column-count:2");
  });

  it("keeps a short track list to a single column", () => {
    const tracks = [{ number: 1, title: "Only Track" }];
    const { svg } = buildShareCardSvg({ ...baseOptions, aspectRatio: "16:9", tracks });
    expect(svg).toContain("column-count:1");
  });

  it("is deterministic for a given seed", () => {
    const a = buildShareCardSvg({ ...baseOptions, aspectRatio: "1:1" });
    const b = buildShareCardSvg({ ...baseOptions, aspectRatio: "1:1" });
    expect(a.svg).toBe(b.svg);
  });

  it("renders a fanned cover stack when 2+ stack covers are given", () => {
    const { svg } = buildShareCardSvg({
      ...baseOptions,
      aspectRatio: "1:1",
      coverStackDataUris: ["data:image/png;base64,AAA", "data:image/png;base64,BBB", "data:image/png;base64,CCC"],
    });
    expect(svg).toContain("data:image/png;base64,AAA");
    expect(svg).toContain("data:image/png;base64,BBB");
    expect(svg).toContain("data:image/png;base64,CCC");
    expect(svg).toContain("rotate(5deg)");
  });

  it("falls back to a single cover when the stack has fewer than 2 entries", () => {
    const { svg } = buildShareCardSvg({
      ...baseOptions,
      aspectRatio: "1:1",
      coverDataUri: "data:image/png;base64,SOLO",
      coverStackDataUris: ["data:image/png;base64,SOLO"],
    });
    expect(svg).toContain("data:image/png;base64,SOLO");
    expect(svg).not.toContain("rotate(5deg)");
  });

  it("fans the cover stack away from the text column on landscape ratios", () => {
    const { svg } = buildShareCardSvg({
      ...baseOptions,
      aspectRatio: "16:9",
      coverStackDataUris: ["data:image/png;base64,AAA", "data:image/png;base64,BBB"],
    });
    expect(svg).toContain("rotate(-5deg)");
    expect(svg).not.toContain("rotate(5deg)");
  });

  function coverPixelWidth(svg: string): number {
    const match = svg.match(/width:(\d+)px;height:\d+px;object-fit:cover/);
    if (!match) throw new Error("cover image not found in svg");
    return Number(match[1]);
  }

  const withCover = { ...baseOptions, coverDataUri: "data:image/png;base64,COVER" };

  it("shrinks the cover on portrait cards with no subtitle/metadata (e.g. a minimal artist card)", () => {
    const full = buildShareCardSvg({ ...withCover, aspectRatio: "9:16", includeTrackList: false });
    const titleOnly = buildShareCardSvg({
      ...withCover,
      aspectRatio: "9:16",
      subtitle: "",
      metadataLine: "",
      includeTrackList: false,
    });
    expect(coverPixelWidth(titleOnly.svg)).toBeLessThan(coverPixelWidth(full.svg));
  });

  it("keeps the existing album (title+subtitle+metadata) cover size unchanged on portrait cards", () => {
    const { svg } = buildShareCardSvg({ ...withCover, aspectRatio: "9:16", includeTrackList: false });
    // 1080 * 0.72 * min(1.5, 1920/1080/1.33) = 1080 * 0.72 * 1.3363... rounds to 1039
    expect(coverPixelWidth(svg)).toBe(1039);
  });

  it("shrinks the cover as a long track list needs more of the frame for itself", () => {
    const shortList = Array.from({ length: 10 }, (_, i) => ({ number: i + 1, title: `Track ${i + 1}` }));
    const longList = Array.from({ length: 50 }, (_, i) => ({ number: i + 1, title: `Track ${i + 1}` }));
    const short = buildShareCardSvg({ ...withCover, aspectRatio: "1:1", tracks: shortList, includeTrackList: true });
    const long = buildShareCardSvg({ ...withCover, aspectRatio: "1:1", tracks: longList, includeTrackList: true });
    expect(coverPixelWidth(long.svg)).toBeLessThan(coverPixelWidth(short.svg));
  });
});

describe("buildStatsShareCardSvg", () => {
  const baseStatsOptions = {
    theme: "dark" as const,
    seed: "stats-7d",
    rangeLabel: "Past 7 Days",
    totalMinutesLabel: "123 minutes listened",
    sections: [
      { title: "Top Artists", items: [{ label: "Artist A", secondary: null }] },
      { title: "Top Albums", items: [{ label: "Album A", secondary: "Artist A" }] },
      { title: "Top Songs", items: [{ label: "Song A", secondary: "Artist A" }] },
      { title: "Top Genres", items: [{ label: "Rock" }] },
    ],
    clockBuckets: [
      { label: "Morning", count: 3 },
      { label: "Afternoon", count: 8 },
      { label: "Evening", count: 5 },
      { label: "Late Night", count: 1 },
    ],
  };

  it("renders each aspect ratio at its declared pixel dimensions", () => {
    for (const ratio of SHARE_ASPECT_RATIOS) {
      const { svg, width, height } = buildStatsShareCardSvg({ ...baseStatsOptions, aspectRatio: ratio.id });
      expect(width).toBe(ratio.width);
      expect(height).toBe(ratio.height);
      expect(svg).toContain(`width="${ratio.width}"`);
      expect(svg).toContain(`height="${ratio.height}"`);
    }
  });

  it("sizes text off whichever dimension is smaller, so a landscape frame's constrained height doesn't overflow", () => {
    const landscape = buildStatsShareCardSvg({ ...baseStatsOptions, aspectRatio: "16:9" });
    const square = buildStatsShareCardSvg({ ...baseStatsOptions, aspectRatio: "1:1" });
    const titleFontSize = (svg: string): number => {
      const match = svg.match(/font-size:(\d+)px;font-weight:800;color:[^;]+;text-align:center/);
      if (!match) throw new Error("title not found in svg");
      return Number(match[1]);
    };
    // 1920x1080 and 1080x1080 share the same smaller dimension (1080), so
    // sizing off that (not the 16:9 frame's much wider 1920) should produce
    // an identical title size on both.
    expect(titleFontSize(landscape.svg)).toBe(titleFontSize(square.svg));
  });

  it("includes each section's title and items", () => {
    const { svg } = buildStatsShareCardSvg({ ...baseStatsOptions, aspectRatio: "1:1" });
    expect(svg).toContain("Top Artists");
    expect(svg).toContain("Artist A");
    expect(svg).toContain("Song A");
    expect(svg).toContain("Rock");
  });

  it("escapes section item text", () => {
    const { svg } = buildStatsShareCardSvg({
      ...baseStatsOptions,
      aspectRatio: "1:1",
      sections: [{ title: "Top Artists", items: [{ label: '<script>alert("x")</script>' }] }],
    });
    expect(svg).not.toContain("<script>alert");
    expect(svg).toContain("&lt;script&gt;");
  });

  it("is deterministic for a given seed", () => {
    const a = buildStatsShareCardSvg({ ...baseStatsOptions, aspectRatio: "1:1" });
    const b = buildStatsShareCardSvg({ ...baseStatsOptions, aspectRatio: "1:1" });
    expect(a.svg).toBe(b.svg);
  });
});
