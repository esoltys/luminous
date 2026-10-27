// @vitest-environment node
import { describe, it, expect, beforeEach, afterEach } from "vitest";
import { createHash } from "crypto";
import fs from "fs";
import path from "path";
import os from "os";
import {
  toDottedFilename,
  readZsyncHeaders,
  verifyZsync,
  generateAppImageZsyncs,
  type ZsyncmakeRunner,
} from "../../../scripts/fix-appimage-zsync.js";

const APPIMAGE = "Luminous Music Player_3.2.0_amd64.AppImage";
const ASSET = "Luminous.Music.Player_3.2.0_amd64.AppImage";

/** Builds a .zsync like zsyncmake 0.6.2 does: ASCII headers, a blank line, then binary checksums. */
function zsyncFor(appImage: Buffer, overrides: Record<string, string> = {}): Buffer {
  const headers = {
    "zsync": "0.6.2",
    "Filename": ASSET,
    "MTime": "Tue, 27 Oct 2026 17:31:00 +0000",
    "Blocksize": "2048",
    "Length": String(appImage.length),
    "Hash-Lengths": "2,2,4",
    "URL": ASSET,
    "SHA-1": createHash("sha1").update(appImage).digest("hex"),
    ...overrides,
  };
  const text = Object.entries(headers).map(([k, v]) => `${k}: ${v}`).join("\n");
  return Buffer.concat([Buffer.from(`${text}\n\n`, "latin1"), Buffer.from([0x00, 0xff, 0x10, 0x0a, 0x0a])]);
}

/** A fake zsyncmake that honours -o and writes the given .zsync content. */
function fakeZsyncmake(content: (appImage: Buffer) => Buffer, calls: string[][] = []): ZsyncmakeRunner {
  return (args) => {
    calls.push(args);
    const out = args[args.indexOf("-o") + 1];
    fs.writeFileSync(out, content(fs.readFileSync(args[args.length - 1])));
  };
}

describe("fixAppImageZsync: headers", () => {
  it("converts spaces to dots in filenames", () => {
    expect(toDottedFilename(APPIMAGE)).toBe(ASSET);
    expect(toDottedFilename(ASSET)).toBe(ASSET);
  });

  it("reads the header block and stops at the blank line before the binary checksums", () => {
    const headers = readZsyncHeaders(zsyncFor(Buffer.from("app")));
    expect(headers.get("Filename")).toBe(ASSET);
    expect(headers.get("Length")).toBe("3");
    expect(headers.size).toBe(8);
  });

  it("reads CRLF headers", () => {
    const headers = readZsyncHeaders(Buffer.from(`Filename: ${ASSET}\r\nLength: 3\r\n\r\nDATA`));
    expect(headers.get("Length")).toBe("3");
  });

  it("throws on a file without a header terminator", () => {
    expect(() => readZsyncHeaders(Buffer.from("just plain text"))).toThrow(/no header terminator/);
  });
});

describe("fixAppImageZsync: generating and verifying", () => {
  let tmpDir: string;
  let appImagePath: string;

  beforeEach(() => {
    tmpDir = fs.mkdtempSync(path.join(os.tmpdir(), "zsync-test-"));
    appImagePath = path.join(tmpDir, APPIMAGE);
    fs.writeFileSync(appImagePath, Buffer.from("fake AppImage bytes"));
  });

  afterEach(() => {
    fs.rmSync(tmpDir, { recursive: true, force: true });
  });

  it("runs zsyncmake on the AppImage under its dotted asset name, writing beside it", () => {
    const calls: string[][] = [];
    const written = generateAppImageZsyncs(tmpDir, fakeZsyncmake((a) => zsyncFor(a), calls));

    const zsyncPath = path.join(tmpDir, `${ASSET}.zsync`);
    expect(written).toEqual([zsyncPath]);
    expect(calls).toEqual([["-f", ASSET, "-u", ASSET, "-o", zsyncPath, appImagePath]]);
    expect(fs.existsSync(zsyncPath)).toBe(true);
  });

  it("replaces a .zsync already in the directory", () => {
    const stale = path.join(tmpDir, `${APPIMAGE}.zsync`);
    fs.writeFileSync(stale, "stale");

    generateAppImageZsyncs(tmpDir, fakeZsyncmake((a) => zsyncFor(a)));

    expect(fs.existsSync(stale)).toBe(false);
    expect(fs.readdirSync(tmpDir).filter((f) => f.endsWith(".zsync"))).toEqual([`${ASSET}.zsync`]);
  });

  it("rejects a .zsync whose SHA-1 doesn't match the AppImage", () => {
    const run = fakeZsyncmake((a) => zsyncFor(a, { "SHA-1": "0".repeat(40) }));
    expect(() => generateAppImageZsyncs(tmpDir, run)).toThrow(/SHA-1 is 0{40}, AppImage is [0-9a-f]{40}/);
  });

  it("rejects a .zsync whose Length doesn't match the AppImage", () => {
    const run = fakeZsyncmake((a) => zsyncFor(a, { Length: "1" }));
    expect(() => generateAppImageZsyncs(tmpDir, run)).toThrow(/Length is 1, AppImage is 19 bytes/);
  });

  it("rejects a .zsync that still names the AppImage with spaces", () => {
    const zsyncPath = path.join(tmpDir, `${ASSET}.zsync`);
    fs.writeFileSync(zsyncPath, zsyncFor(fs.readFileSync(appImagePath), { Filename: APPIMAGE, URL: APPIMAGE }));
    expect(() => verifyZsync(zsyncPath, appImagePath, ASSET)).toThrow(/Filename is "Luminous Music Player.*URL is/);
  });

  it("throws if the directory holds no AppImage", () => {
    fs.rmSync(appImagePath);
    expect(() => generateAppImageZsyncs(tmpDir, fakeZsyncmake((a) => zsyncFor(a)))).toThrow(/No \.AppImage files found/);
  });

  it("throws if the directory does not exist", () => {
    expect(() => generateAppImageZsyncs(path.join(tmpDir, "nonexistent"))).toThrow(/Directory not found/);
  });
});
