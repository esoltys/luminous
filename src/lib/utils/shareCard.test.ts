import { describe, it, expect } from "vitest";
import { buildShareCardSvg, buildStatsShareCardSvg, buildMosaicCoverHtml, SHARE_ASPECT_RATIOS } from "./shareCard";

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

  it("renders a mosaic cover layout on horizontal aspect ratios (16:9 and 4:3)", () => {
    for (const ratio of ["16:9", "4:3"] as const) {
      const { svg } = buildShareCardSvg({
        ...baseOptions,
        aspectRatio: ratio,
        coverStackDataUris: ["data:image/png;base64,AAA", "data:image/png;base64,BBB"],
      });
      expect(svg).not.toContain("rotate(-5deg)");
      expect(svg).not.toContain("rotate(5deg)");
      expect(svg).toContain("data:image/png;base64,AAA");
      expect(svg).toContain("data:image/png;base64,BBB");
      expect(svg).toContain("grid-template-columns:2fr 1fr");
    }
  });

  it("keeps the fanned stack on portrait aspect ratios (9:16, 3:4)", () => {
    for (const ratio of ["9:16", "3:4"] as const) {
      const { svg } = buildShareCardSvg({
        ...baseOptions,
        aspectRatio: ratio,
        coverStackDataUris: ["data:image/png;base64,AAA", "data:image/png;base64,BBB"],
      });
      expect(svg).toContain("rotate(5deg)");
    }
  });

  it("caps mosaic width on horizontal frames so adjacent text has sufficient space and is not pushed offscreen", () => {
    const { svg } = buildShareCardSvg({
      ...baseOptions,
      aspectRatio: "4:3",
      includeTrackList: false,
      coverStackDataUris: [
        "data:image/png;base64,1",
        "data:image/png;base64,2",
        "data:image/png;base64,3",
        "data:image/png;base64,4",
      ],
    });
    const match = svg.match(/width:(\d+)px;height:(\d+)px;border-radius/);
    expect(match).not.toBeNull();
    const mosaicWidth = Number(match![1]);
    const cardWidth = 1440;
    const cardPad = Math.round(1440 * 0.06);
    const contentGap = Math.round(1440 * 0.035);
    const availWidth = cardWidth - 2 * cardPad - contentGap;
    expect(mosaicWidth).toBeLessThanOrEqual(availWidth * 0.55);
    expect(availWidth - mosaicWidth).toBeGreaterThan(500);
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

  it("renders a section's cover stack when given one, and omits it otherwise", () => {
    const { svg } = buildStatsShareCardSvg({
      ...baseStatsOptions,
      aspectRatio: "1:1",
      sections: [
        {
          title: "Top Artists",
          items: [{ label: "Artist A" }],
          coverStackDataUris: ["data:image/png;base64,AAA", "data:image/png;base64,BBB"],
        },
        { title: "Top Genres", items: [{ label: "Rock" }] },
      ],
    });
    expect(svg).toContain("data:image/png;base64,AAA");
    expect(svg).toContain("data:image/png;base64,BBB");
    expect(svg).toContain("rotate(-5deg)");
  });

  it("renders a mosaic cover for sections on horizontal aspect ratios (16:9 and 4:3)", () => {
    for (const ratio of ["16:9", "4:3"] as const) {
      const { svg } = buildStatsShareCardSvg({
        ...baseStatsOptions,
        aspectRatio: ratio,
        sections: [
          {
            title: "Top Artists",
            items: [{ label: "Artist A" }],
            coverStackDataUris: ["data:image/png;base64,AAA", "data:image/png;base64,BBB"],
          },
        ],
      });
      expect(svg).not.toContain("rotate(-5deg)");
      expect(svg).not.toContain("rotate(5deg)");
      expect(svg).toContain("data:image/png;base64,AAA");
      expect(svg).toContain("data:image/png;base64,BBB");
      expect(svg).toContain("grid-template-columns:2fr 1fr");
    }
  });

  it("renders a fanned cover stack for sections on portrait/square aspect ratios (1:1, 9:16, 3:4)", () => {
    for (const ratio of ["1:1", "9:16", "3:4"] as const) {
      const { svg } = buildStatsShareCardSvg({
        ...baseStatsOptions,
        aspectRatio: ratio,
        sections: [
          {
            title: "Top Artists",
            items: [{ label: "Artist A" }],
            coverStackDataUris: ["data:image/png;base64,AAA", "data:image/png;base64,BBB"],
          },
        ],
      });
      expect(svg).toContain("rotate(-5deg)");
    }
  });
});

describe("buildMosaicCoverHtml", () => {
  it("returns empty string when no covers are provided", () => {
    expect(buildMosaicCoverHtml(null, [], 100)).toBe("");
  });

  it("falls back to a single image for 1 cover", () => {
    const html = buildMosaicCoverHtml("data:image/png;base64,ONE", null, 100);
    expect(html).toContain('src="data:image/png;base64,ONE"');
    expect(html).toContain("width:100px;height:100px");
    expect(html).not.toContain("grid-template-columns");
  });

  it("renders 1 full tile + 1 quarter tile for 2 covers (ratio 1.5)", () => {
    const html = buildMosaicCoverHtml(null, ["data:image/png;base64,1", "data:image/png;base64,2"], 100);
    expect(html).toContain("grid-template-columns:2fr 1fr");
    expect(html).toContain("width:150px;height:100px");
    expect(html).toContain('src="data:image/png;base64,1"');
    expect(html).toContain('src="data:image/png;base64,2"');
  });

  it("renders 1 full tile + 2 stacked quarter tiles for 3 covers (ratio 1.5)", () => {
    const html = buildMosaicCoverHtml(
      null,
      ["data:image/png;base64,1", "data:image/png;base64,2", "data:image/png;base64,3"],
      100
    );
    expect(html).toContain("grid-template-columns:2fr 1fr");
    expect(html).toContain("width:150px;height:100px");
    expect(html).toContain('src="data:image/png;base64,1"');
    expect(html).toContain('src="data:image/png;base64,2"');
    expect(html).toContain('src="data:image/png;base64,3"');
  });

  it("renders 1 full tile + 3 quarter tiles for 4 covers (ratio 2.0)", () => {
    const html = buildMosaicCoverHtml(
      null,
      [
        "data:image/png;base64,1",
        "data:image/png;base64,2",
        "data:image/png;base64,3",
        "data:image/png;base64,4",
      ],
      100
    );
    expect(html).toContain("grid-template-columns:2fr 2fr");
    expect(html).toContain("grid-template-columns:1fr 1fr");
    expect(html).toContain("width:200px;height:100px");
  });

  it("renders 1 full tile + 4 quarter tiles for 5 covers (ratio 2.0)", () => {
    const html = buildMosaicCoverHtml(
      null,
      [
        "data:image/png;base64,1",
        "data:image/png;base64,2",
        "data:image/png;base64,3",
        "data:image/png;base64,4",
        "data:image/png;base64,5",
      ],
      100
    );
    expect(html).toContain("grid-template-columns:2fr 2fr");
    expect(html).toContain("grid-template-columns:1fr 1fr");
    expect(html).toContain("width:200px;height:100px");
    expect(html).toContain('src="data:image/png;base64,5"');
  });

  it("caps the mosaic at 5 covers when more are supplied", () => {
    const html = buildMosaicCoverHtml(
      null,
      [
        "data:image/png;base64,1",
        "data:image/png;base64,2",
        "data:image/png;base64,3",
        "data:image/png;base64,4",
        "data:image/png;base64,5",
        "data:image/png;base64,6",
      ],
      100
    );
    expect(html).toContain('src="data:image/png;base64,5"');
    expect(html).not.toContain('src="data:image/png;base64,6"');
  });

  it("uses drop-shadow and synchronously-decoded images so WebKitGTK renders covers correctly", () => {
    const uris = ["data:image/png;base64,1", "data:image/png;base64,2", "data:image/png;base64,3"];
    for (const html of [
      buildMosaicCoverHtml("data:image/png;base64,ONE", null, 100),
      buildMosaicCoverHtml(null, uris, 100),
    ]) {
      expect(html).not.toContain("box-shadow");
      expect(html).toContain("drop-shadow(");
      const imgs = html.match(/<img /g)?.length ?? 0;
      expect(html.match(/<img decoding="sync" /g)?.length).toBe(imgs);
    }
  });
});
