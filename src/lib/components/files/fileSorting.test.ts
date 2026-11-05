import { describe, it, expect } from "vitest";
import { sortFileEntries, type FileSortColumn } from "./fileSorting";
import type { FileEntry } from "../../types";

function mockEntry(overrides: Partial<FileEntry>): FileEntry {
  return {
    name: overrides.name ?? "file.mp3",
    isDir: overrides.isDir ?? false,
    size: overrides.size ?? 1000,
    modified: overrides.modified ?? null,
    fileType: overrides.fileType ?? "MP3",
    kind: overrides.kind ?? "audio",
    opensInApp: overrides.opensInApp ?? false,
    status: overrides.status ?? "inLibrary",
    songId: overrides.songId ?? 1,
    missingTag: overrides.missingTag ?? null,
    location: overrides.location ?? "/music",
  };
}

describe("fileSorting", () => {
  const folderA = mockEntry({ name: "Albums", isDir: true, size: null, fileType: null });
  const folderZ = mockEntry({ name: "Z-Archive", isDir: true, size: null, fileType: null });
  const track1 = mockEntry({ name: "01 Intro.flac", isDir: false, size: 5000, fileType: "FLAC", status: "inLibrary" });
  const track2 = mockEntry({ name: "02 Solo.mp3", isDir: false, size: 2000, fileType: "MP3", status: "notScanned" });
  const track3 = mockEntry({ name: "cover.jpg", isDir: false, size: 10000, fileType: "JPG", status: "albumArt" });

  const allEntries = [track2, folderZ, track1, folderA, track3];

  it("always keeps folders on top when sorting by name ascending", () => {
    const sorted = sortFileEntries(allEntries, "name", true);
    expect(sorted.map((e) => e.name)).toEqual([
      "Albums",
      "Z-Archive",
      "01 Intro.flac",
      "02 Solo.mp3",
      "cover.jpg",
    ]);
  });

  it("always keeps folders on top when sorting by name descending", () => {
    const sorted = sortFileEntries(allEntries, "name", false);
    expect(sorted.map((e) => e.name)).toEqual([
      "Z-Archive",
      "Albums",
      "cover.jpg",
      "02 Solo.mp3",
      "01 Intro.flac",
    ]);
  });

  it("sorts by size ascending while keeping folders at top in alphabetical order", () => {
    const sorted = sortFileEntries(allEntries, "size", true);
    expect(sorted.map((e) => e.name)).toEqual([
      "Albums",
      "Z-Archive",
      "02 Solo.mp3",   // 2000
      "01 Intro.flac", // 5000
      "cover.jpg",     // 10000
    ]);
  });

  it("sorts by size descending while keeping folders at top in alphabetical order", () => {
    const sorted = sortFileEntries(allEntries, "size", false);
    expect(sorted.map((e) => e.name)).toEqual([
      "Albums",
      "Z-Archive",
      "cover.jpg",     // 10000
      "01 Intro.flac", // 5000
      "02 Solo.mp3",   // 2000
    ]);
  });

  it("sorts by fileType ascending with name tiebreaker", () => {
    const track4 = mockEntry({ name: "03 Outro.flac", isDir: false, size: 4000, fileType: "FLAC" });
    const sorted = sortFileEntries([track4, track1, track2, track3, folderA], "fileType", true);
    expect(sorted.map((e) => e.name)).toEqual([
      "Albums",
      "01 Intro.flac", // FLAC
      "03 Outro.flac", // FLAC
      "cover.jpg",     // JPG
      "02 Solo.mp3",   // MP3
    ]);
  });

  it("sorts by status ascending using custom label resolver", () => {
    const getStatusLabel = (e: FileEntry) => (e.status === "inLibrary" ? "A-InLib" : "B-Other");
    const sorted = sortFileEntries([track1, track2, folderA], "status", true, getStatusLabel);
    expect(sorted.map((e) => e.name)).toEqual([
      "Albums",
      "01 Intro.flac",
      "02 Solo.mp3",
    ]);
  });
});
