import { describe, it, expect } from "vitest";
import { canOpen } from "./folderActions";
import type { FileEntry } from "../../types";

function entry(over: Partial<FileEntry>): FileEntry {
  return {
    name: "album.md",
    isDir: false,
    size: 10,
    modified: null,
    fileType: "MD",
    kind: "notes",
    status: "notAudio",
    songId: null,
    missingTag: null,
    location: "C:/Music/Live/album.md",
    ...over,
  };
}

describe("canOpen", () => {
  it("opens a sidecar on this computer", () => {
    expect(canOpen(entry({}))).toBe(true);
    expect(canOpen(entry({ name: "rip.cue", kind: "cueSheet", status: "inLibrary" }))).toBe(true);
  });

  it("leaves a sidecar on a server or device alone: there is nothing local to open", () => {
    expect(canOpen(entry({ location: null }))).toBe(false);
  });

  it("plays audio rather than opening it", () => {
    expect(canOpen(entry({ name: "01.flac", kind: "audio", status: "inLibrary", songId: 7 }))).toBe(false);
  });

  it("never opens a folder", () => {
    expect(canOpen(entry({ name: "Disc 2", isDir: true, kind: null, status: null }))).toBe(false);
  });
});
