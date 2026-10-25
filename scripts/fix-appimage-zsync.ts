import fs from "fs";
import path from "path";
import { fileURLToPath } from "url";

export function findHeaderEnd(buf: Buffer): { headerEnd: number; bodyStart: number; lineEnding: string } | null {
  for (let i = 0; i < buf.length - 1; i++) {
    // Check for \r\n\r\n
    if (
      i < buf.length - 3 &&
      buf[i] === 0x0d &&
      buf[i + 1] === 0x0a &&
      buf[i + 2] === 0x0d &&
      buf[i + 3] === 0x0a
    ) {
      return { headerEnd: i, bodyStart: i + 4, lineEnding: "\r\n" };
    }
    // Check for \n\n
    if (buf[i] === 0x0a && buf[i + 1] === 0x0a) {
      return { headerEnd: i, bodyStart: i + 2, lineEnding: "\n" };
    }
  }
  return null;
}

export function transformZsyncBuffer(buf: Buffer): {
  buffer: Buffer;
  oldFilename?: string;
  newFilename?: string;
} {
  const headerInfo = findHeaderEnd(buf);
  if (!headerInfo) {
    throw new Error("Invalid .zsync file: no header terminator found");
  }

  const { headerEnd, bodyStart, lineEnding } = headerInfo;
  const headerText = buf.subarray(0, headerEnd).toString("utf-8");
  const lines = headerText.split(/\r?\n/);

  let oldFilename: string | undefined;
  let newFilename: string | undefined;

  // First pass: find Filename:
  for (const line of lines) {
    const match = line.match(/^Filename:\s*(.+)$/i);
    if (match) {
      oldFilename = match[1].trim();
      newFilename = oldFilename.replace(/\s+/g, ".");
      break;
    }
  }

  const newLines = lines.map((line) => {
    if (line.match(/^Filename:\s*/i) && newFilename) {
      return `Filename: ${newFilename}`;
    }
    if (line.match(/^URL:\s*/i)) {
      const urlVal = line.replace(/^URL:\s*/i, "").trim();
      if (oldFilename && newFilename && urlVal.includes(oldFilename)) {
        return `URL: ${urlVal.replaceAll(oldFilename, newFilename)}`;
      }
      return `URL: ${urlVal.replace(/\s+/g, ".")}`;
    }
    return line;
  });

  const newHeaderText = newLines.join(lineEnding);
  const newHeaderBuf = Buffer.from(newHeaderText, "utf-8");
  const separatorBuf = Buffer.from(lineEnding + lineEnding, "utf-8");
  const bodyBuf = buf.subarray(bodyStart);

  const resultBuf = Buffer.concat([newHeaderBuf, separatorBuf, bodyBuf]);
  return {
    buffer: resultBuf,
    oldFilename,
    newFilename,
  };
}

export function fixZsyncFile(filePath: string): { oldPath: string; newPath: string } {
  const buf = fs.readFileSync(filePath);
  const { buffer: newBuf } = transformZsyncBuffer(buf);

  const dir = path.dirname(filePath);
  const base = path.basename(filePath);
  const dottedBase = base.replace(/\s+/g, ".");
  const newPath = path.join(dir, dottedBase);

  fs.writeFileSync(newPath, newBuf);
  if (path.resolve(filePath) !== path.resolve(newPath)) {
    fs.unlinkSync(filePath);
  }

  return { oldPath: filePath, newPath };
}

export function fixAllZsyncInDir(dir: string): Array<{ oldPath: string; newPath: string }> {
  if (!fs.existsSync(dir)) {
    throw new Error(`Directory not found: ${dir}`);
  }
  const files = fs.readdirSync(dir).filter((f) => f.endsWith(".zsync"));
  if (files.length === 0) {
    throw new Error(`No .zsync files found in ${dir}`);
  }
  const results: Array<{ oldPath: string; newPath: string }> = [];
  for (const f of files) {
    results.push(fixZsyncFile(path.join(dir, f)));
  }
  return results;
}

const isMain =
  (typeof (import.meta as { main?: boolean }).main === "boolean" &&
    (import.meta as { main?: boolean }).main) ||
  (process.argv[1] && fileURLToPath(import.meta.url) === path.resolve(process.argv[1]));

if (isMain) {
  const targetDir =
    process.argv[2] || path.resolve(process.cwd(), "target/release/bundle/appimage");
  console.log(`Fixing .zsync files in ${targetDir}...`);
  const fixed = fixAllZsyncInDir(targetDir);
  for (const item of fixed) {
    console.log(`Fixed: ${item.oldPath} -> ${item.newPath}`);
  }
}
