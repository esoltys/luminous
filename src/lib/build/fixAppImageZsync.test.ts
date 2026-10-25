// @vitest-environment node
import { describe, it, expect, beforeEach, afterEach } from "vitest";
import fs from "fs";
import path from "path";
import os from "os";
import {
  toDottedFilename,
  findHeaderEnd,
  transformZsyncBuffer,
  fixZsyncFile,
  fixAllZsyncInDir,
} from "../../../scripts/fix-appimage-zsync.js";

describe("fixAppImageZsync: buffer transformation", () => {
  it("converts spaces to dots in filenames", () => {
    expect(toDottedFilename("Luminous Music Player_3.1.0_amd64.AppImage")).toBe(
      "Luminous.Music.Player_3.1.0_amd64.AppImage"
    );
    expect(toDottedFilename("Already.Dotted.AppImage")).toBe("Already.Dotted.AppImage");
  });

  it("finds header end with \\n\\n", () => {
    const buf = Buffer.from("zsync: 0.6.2\nFilename: test.AppImage\n\nBINARY_DATA");
    const result = findHeaderEnd(buf);
    expect(result).not.toBeNull();
    expect(result?.lineEnding).toBe("\n");
    expect(buf.subarray(result!.bodyStart).toString()).toBe("BINARY_DATA");
  });

  it("finds header end with \\r\\n\\r\\n", () => {
    const buf = Buffer.from("zsync: 0.6.2\r\nFilename: test.AppImage\r\n\r\nBINARY_DATA");
    const result = findHeaderEnd(buf);
    expect(result).not.toBeNull();
    expect(result?.lineEnding).toBe("\r\n");
    expect(buf.subarray(result!.bodyStart).toString()).toBe("BINARY_DATA");
  });

  it("rewrites Filename and URL headers with spaces replaced by dots", () => {
    const binaryBytes = Buffer.from([0x00, 0x01, 0x02, 0xff, 0xfe, 0x00, 0x10]);
    const header = [
      "zsync: 0.6.2",
      "Filename: Luminous Music Player_3.1.0_amd64.AppImage",
      "MTime: 1713511200",
      "Blocksize: 2048",
      "Length: 524288000",
      "Hash-Lengths: 2,2,4",
      "URL: Luminous Music Player_3.1.0_amd64.AppImage",
      "SHA-1: 97edb7d0d7daa7864c45edf14add33ec23ae94f8",
    ].join("\n");

    const input = Buffer.concat([Buffer.from(header + "\n\n", "utf-8"), binaryBytes]);
    const { buffer: output, oldFilename, newFilename } = transformZsyncBuffer(input);

    expect(oldFilename).toBe("Luminous Music Player_3.1.0_amd64.AppImage");
    expect(newFilename).toBe("Luminous.Music.Player_3.1.0_amd64.AppImage");

    const headerEnd = findHeaderEnd(output)!;
    const outputHeader = output.subarray(0, headerEnd.headerEnd).toString("utf-8");
    const outputBinary = output.subarray(headerEnd.bodyStart);

    expect(outputHeader).toContain("Filename: Luminous.Music.Player_3.1.0_amd64.AppImage");
    expect(outputHeader).toContain("URL: Luminous.Music.Player_3.1.0_amd64.AppImage");
    expect(outputHeader).not.toContain("Luminous Music Player");

    // Binary payload must be byte-for-byte identical
    expect(outputBinary).toEqual(binaryBytes);
  });

  it("handles zsync file without URL header", () => {
    const binaryBytes = Buffer.from([0xaa, 0xbb, 0xcc]);
    const header = [
      "zsync: 0.6.2",
      "Filename: Luminous Music Player_3.1.0_amd64.AppImage",
      "Blocksize: 2048",
      "Length: 100",
    ].join("\n");

    const input = Buffer.concat([Buffer.from(header + "\n\n", "utf-8"), binaryBytes]);
    const { buffer: output } = transformZsyncBuffer(input);

    const headerEnd = findHeaderEnd(output)!;
    const outputHeader = output.subarray(0, headerEnd.headerEnd).toString("utf-8");
    expect(outputHeader).toContain("Filename: Luminous.Music.Player_3.1.0_amd64.AppImage");
    expect(outputHeader).not.toContain("URL:");
    expect(output.subarray(headerEnd.bodyStart)).toEqual(binaryBytes);
  });

  it("preserves already-dotted headers (idempotency)", () => {
    const binaryBytes = Buffer.from([0x12, 0x34]);
    const header = [
      "zsync: 0.6.2",
      "Filename: Luminous.Music.Player_3.1.0_amd64.AppImage",
      "URL: Luminous.Music.Player_3.1.0_amd64.AppImage",
    ].join("\n");

    const input = Buffer.concat([Buffer.from(header + "\n\n", "utf-8"), binaryBytes]);
    const { buffer: output, oldFilename, newFilename } = transformZsyncBuffer(input);

    expect(oldFilename).toBe("Luminous.Music.Player_3.1.0_amd64.AppImage");
    expect(newFilename).toBe("Luminous.Music.Player_3.1.0_amd64.AppImage");
    expect(output).toEqual(input);
  });

  it("throws on invalid file without header end", () => {
    const invalid = Buffer.from("just plain text without empty line");
    expect(() => transformZsyncBuffer(invalid)).toThrow(/no header terminator/);
  });
});

describe("fixAppImageZsync: filesystem operations", () => {
  let tmpDir: string;

  beforeEach(() => {
    tmpDir = fs.mkdtempSync(path.join(os.tmpdir(), "zsync-test-"));
  });

  afterEach(() => {
    fs.rmSync(tmpDir, { recursive: true, force: true });
  });

  it("renames file on disk and removes old file with spaces", () => {
    const oldFileName = "Luminous Music Player_3.1.0_amd64.AppImage.zsync";
    const oldFilePath = path.join(tmpDir, oldFileName);
    const content = Buffer.concat([
      Buffer.from("zsync: 0.6.2\nFilename: Luminous Music Player_3.1.0_amd64.AppImage\n\n"),
      Buffer.from([0x01, 0x02, 0x03]),
    ]);
    fs.writeFileSync(oldFilePath, content);

    const result = fixZsyncFile(oldFilePath);
    expect(result.oldPath).toBe(oldFilePath);
    expect(path.basename(result.newPath)).toBe("Luminous.Music.Player_3.1.0_amd64.AppImage.zsync");

    expect(fs.existsSync(oldFilePath)).toBe(false);
    expect(fs.existsSync(result.newPath)).toBe(true);

    const updated = fs.readFileSync(result.newPath, "utf-8");
    expect(updated).toContain("Filename: Luminous.Music.Player_3.1.0_amd64.AppImage");
  });

  it("processes all .zsync files in a directory", () => {
    const file1 = path.join(tmpDir, "Luminous Music Player_3.1.0_amd64.AppImage.zsync");
    fs.writeFileSync(
      file1,
      Buffer.from("zsync: 0.6.2\nFilename: Luminous Music Player_3.1.0_amd64.AppImage\n\nDATA")
    );

    const results = fixAllZsyncInDir(tmpDir);
    expect(results).toHaveLength(1);
    expect(path.basename(results[0].newPath)).toBe("Luminous.Music.Player_3.1.0_amd64.AppImage.zsync");
    expect(fs.existsSync(file1)).toBe(false);
  });

  it("throws if no .zsync files are found in directory", () => {
    expect(() => fixAllZsyncInDir(tmpDir)).toThrow(/No \.zsync files found/);
  });

  it("throws if directory does not exist", () => {
    expect(() => fixAllZsyncInDir(path.join(tmpDir, "nonexistent"))).toThrow(/Directory not found/);
  });
});
