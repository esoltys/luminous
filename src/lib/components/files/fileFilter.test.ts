import { describe, it, expect } from "vitest";
import { highlightSegments, matchesFileFilter, parseFileFilter } from "./fileFilter";
import type { FileEntry } from "../../types";

function entry(
  name: string,
  isDir = false,
  status: FileEntry["status"] = "inLibrary",
  fileType: string | null = null
): FileEntry {
  return {
    name,
    isDir,
    size: isDir ? null : 1000,
    modified: null,
    fileType: fileType ?? (isDir ? null : name.split(".").pop()?.toUpperCase() ?? null),
    kind: isDir ? null : "audio",
    opensInApp: false,
    status,
    songId: status === "inLibrary" ? 1 : null,
    missingTag: null,
    location: `C:/Music/${name}`,
  };
}

describe("fileFilter", () => {
  it("parses ext:, status:, and free-text terms while ignoring incomplete tokens", () => {
    const parsed = parseFileFilter("ext:flac ext:.cue status:library status: it white");
    expect(parsed.extensions).toEqual(["flac", "cue"]);
    expect(parsed.statuses).toEqual(["inLibrary"]);
    expect(parsed.terms).toEqual(["it", "white"]);
    expect(parsed.freeText).toBe("it white");
    expect(parsed.isActive).toBe(true);

    const incompleteOnly = parseFileFilter("ext: status:");
    expect(incompleteOnly.isActive).toBe(false);
  });

  it("matches entries by extension, status, and substring terms", () => {
    const flacInLib = entry("10 In the White.flac", false, "inLibrary");
    const flacUnscanned = entry("11 The Itch.flac", false, "notScanned");
    const coverJpg = entry("cover.jpg", false, "albumArt");
    const folder = entry("Artwork", true, null);

    const byExtAndText = parseFileFilter("ext:flac it");
    expect(matchesFileFilter(flacInLib, byExtAndText)).toBe(true);
    expect(matchesFileFilter(flacUnscanned, byExtAndText)).toBe(true);
    expect(matchesFileFilter(coverJpg, byExtAndText)).toBe(false);
    expect(matchesFileFilter(folder, byExtAndText)).toBe(false);

    const byStatus = parseFileFilter("status:unscanned");
    expect(matchesFileFilter(flacInLib, byStatus)).toBe(false);
    expect(matchesFileFilter(flacUnscanned, byStatus)).toBe(true);
  });

  it("splits text into highlighted and unhighlighted segments", () => {
    expect(highlightSegments("10 In the White.flac", ["it"])).toEqual([
      { text: "10 In the Wh", match: false },
      { text: "it", match: true },
      { text: "e.flac", match: false },
    ]);
    expect(highlightSegments("11 The Itch.flac", ["it"])).toEqual([
      { text: "11 The ", match: false },
      { text: "It", match: true },
      { text: "ch.flac", match: false },
    ]);
  });
});
