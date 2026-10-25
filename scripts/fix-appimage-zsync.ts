/**
 * Fix AppImage .zsync header metadata and companion filename for GitHub Releases.
 *
 * Background & Nonobvious Context:
 * When linuxdeploy-plugin-appimage builds an AppImage with LDAI_UPDATE_INFORMATION,
 * it runs zsyncmake against the local output binary (e.g. `Luminous Music Player_3.1.0_amd64.AppImage`).
 * This generates a companion `.zsync` file containing metadata headers (`Filename:`, optional `URL:`)
 * followed by binary rolling checksum blocks.
 *
 * When assets are uploaded to GitHub Releases, GitHub automatically transliterates spaces
 * to dots in asset filenames (e.g. `Luminous.Music.Player_3.1.0_amd64.AppImage`).
 * Without adjusting the `.zsync` file, AppImage delta updaters (AppImageUpdate, Gear Lever)
 * look for the space-delimited filename and receive HTTP 404.
 *
 * This module adjusts the ASCII header to reference the dotted filename and renames the file
 * on disk, guaranteeing that the binary checksum payload remains byte-for-byte untouched.
 */
import fs from "fs";
import path from "path";
import { fileURLToPath } from "url";

/**
 * Transliterates spaces to dots in filenames to match GitHub Releases asset upload naming.
 */
export function toDottedFilename(filename: string): string {
  return filename.replace(/\s+/g, ".");
}

/**
 * Locates the boundary between the ASCII header and the binary block payload in a .zsync file.
 * The zsync specification mandates that headers are terminated by a double newline.
 *
 * @returns Header end offset, body start offset, and detected newline sequence, or null if invalid.
 */
export function findHeaderEnd(
  zsyncBuffer: Buffer
): { headerEnd: number; bodyStart: number; lineEnding: string } | null {
  for (let i = 0; i < zsyncBuffer.length - 1; i++) {
    if (
      i < zsyncBuffer.length - 3 &&
      zsyncBuffer[i] === 0x0d &&
      zsyncBuffer[i + 1] === 0x0a &&
      zsyncBuffer[i + 2] === 0x0d &&
      zsyncBuffer[i + 3] === 0x0a
    ) {
      return { headerEnd: i, bodyStart: i + 4, lineEnding: "\r\n" };
    }
    if (zsyncBuffer[i] === 0x0a && zsyncBuffer[i + 1] === 0x0a) {
      return { headerEnd: i, bodyStart: i + 2, lineEnding: "\n" };
    }
  }
  return null;
}

/**
 * Rewrites the Filename and URL headers in a .zsync buffer while preserving the binary payload.
 *
 * @param zsyncBuffer - Raw buffer of the entire .zsync file.
 * @returns The transformed buffer along with original and rewritten filenames.
 * @throws Error if the buffer does not have a valid header terminator.
 */
export function transformZsyncBuffer(zsyncBuffer: Buffer): {
  buffer: Buffer;
  oldFilename?: string;
  newFilename?: string;
} {
  const headerInfo = findHeaderEnd(zsyncBuffer);
  if (!headerInfo) {
    throw new Error("Invalid .zsync file: no header terminator found");
  }

  const { headerEnd, bodyStart, lineEnding } = headerInfo;
  const headerText = zsyncBuffer.subarray(0, headerEnd).toString("utf-8");
  const headerLines = headerText.split(/\r?\n/);

  let oldFilename: string | undefined;
  let newFilename: string | undefined;

  for (const line of headerLines) {
    const match = line.match(/^Filename:\s*(.+)$/i);
    if (match) {
      oldFilename = match[1].trim();
      newFilename = toDottedFilename(oldFilename);
      break;
    }
  }

  const transformedLines = headerLines.map((line) => {
    if (line.match(/^Filename:\s*/i) && newFilename) {
      return `Filename: ${newFilename}`;
    }
    if (line.match(/^URL:\s*/i)) {
      const urlValue = line.replace(/^URL:\s*/i, "").trim();
      if (oldFilename && newFilename && urlValue.includes(oldFilename)) {
        return `URL: ${urlValue.replaceAll(oldFilename, newFilename)}`;
      }
      return `URL: ${toDottedFilename(urlValue)}`;
    }
    return line;
  });

  const newHeaderText = transformedLines.join(lineEnding);
  const newHeaderBuf = Buffer.from(newHeaderText, "utf-8");
  const separatorBuf = Buffer.from(lineEnding + lineEnding, "utf-8");
  const bodyBuf = zsyncBuffer.subarray(bodyStart);

  const resultBuf = Buffer.concat([newHeaderBuf, separatorBuf, bodyBuf]);
  return {
    buffer: resultBuf,
    oldFilename,
    newFilename,
  };
}

/**
 * Rewrites a single .zsync file on disk, replaces its header values with dotted names,
 * and renames the file on disk to eliminate spaces.
 *
 * @param filePath - Path to the target .zsync file.
 * @returns Object recording the original and transformed file paths.
 */
export function fixZsyncFile(filePath: string): { oldPath: string; newPath: string } {
  const fileBuffer = fs.readFileSync(filePath);
  const { buffer: transformedBuffer } = transformZsyncBuffer(fileBuffer);

  const directory = path.dirname(filePath);
  const originalBasename = path.basename(filePath);
  const dottedBasename = toDottedFilename(originalBasename);
  const newPath = path.join(directory, dottedBasename);

  fs.writeFileSync(newPath, transformedBuffer);
  if (path.resolve(filePath) !== path.resolve(newPath)) {
    fs.unlinkSync(filePath);
  }

  return { oldPath: filePath, newPath };
}

/**
 * Scans a directory for all .zsync files and normalizes their headers and filenames.
 *
 * @param directory - Directory containing bundle artifacts (e.g. target/release/bundle/appimage).
 * @returns Array of transformed file paths.
 * @throws Error if the directory does not exist or contains no .zsync files.
 */
export function fixAllZsyncInDir(directory: string): Array<{ oldPath: string; newPath: string }> {
  if (!fs.existsSync(directory)) {
    throw new Error(`Directory not found: ${directory}`);
  }
  const files = fs.readdirSync(directory).filter((f) => f.endsWith(".zsync"));
  if (files.length === 0) {
    throw new Error(`No .zsync files found in ${directory}`);
  }
  const results: Array<{ oldPath: string; newPath: string }> = [];
  for (const file of files) {
    results.push(fixZsyncFile(path.join(directory, file)));
  }
  return results;
}

const isMainModule =
  (typeof (import.meta as { main?: boolean }).main === "boolean" &&
    (import.meta as { main?: boolean }).main) ||
  (process.argv[1] && fileURLToPath(import.meta.url) === path.resolve(process.argv[1]));

if (isMainModule) {
  const targetDir =
    process.argv[2] || path.resolve(process.cwd(), "target/release/bundle/appimage");
  console.log(`Fixing .zsync files in ${targetDir}...`);
  const fixedFiles = fixAllZsyncInDir(targetDir);
  for (const fileResult of fixedFiles) {
    console.log(`Fixed: ${fileResult.oldPath} -> ${fileResult.newPath}`);
  }
}
