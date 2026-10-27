/**
 * Generate the AppImage's .zsync control file for GitHub Releases (#1703, #1834).
 *
 * Background & Nonobvious Context:
 * AppImageUpdate reads the update information embedded in the AppImage
 * (`gh-releases-zsync|…|Luminous.Music.Player_*_amd64.AppImage.zsync`, from
 * LDAI_UPDATE_INFORMATION) and fetches the matching .zsync from the release, which
 * names the AppImage to download delta blocks from.
 *
 * appimagetool (inside linuxdeploy-plugin-appimage) does run zsyncmake, but in its
 * own working directory, not next to the AppImage: under tauri-action that's
 * `src-tauri/`, so the file never reached the bundle directory and v3.2.0's release
 * build failed. It also names the AppImage with spaces, while GitHub renames spaces
 * to dots on upload, so its Filename:/URL: headers would 404.
 *
 * So this script ignores that byproduct and runs zsyncmake itself against the final
 * AppImage in the bundle directory, under the dotted name the release asset will
 * have. It then checks the result's SHA-1 and Length headers against the AppImage,
 * the one check that proves the .zsync describes the file that ships.
 */
import { execFileSync } from "child_process";
import { createHash } from "crypto";
import fs from "fs";
import path from "path";
import { fileURLToPath } from "url";

/**
 * Transliterates spaces to dots in filenames to match GitHub Releases asset upload naming.
 */
export function toDottedFilename(filename: string): string {
  return filename.replace(/\s+/g, ".");
}

/** Parses the ASCII header of a .zsync file (everything before the first blank line). */
export function readZsyncHeaders(zsyncBuffer: Buffer): Map<string, string> {
  const text = zsyncBuffer.toString("latin1");
  const end = text.search(/\r?\n\r?\n/);
  if (end < 0) throw new Error("Invalid .zsync file: no header terminator found");
  const headers = new Map<string, string>();
  for (const line of text.slice(0, end).split(/\r?\n/)) {
    const colon = line.indexOf(":");
    if (colon > 0) headers.set(line.slice(0, colon).trim(), line.slice(colon + 1).trim());
  }
  return headers;
}

function sha1OfFile(filePath: string): string {
  const hash = createHash("sha1");
  const fd = fs.openSync(filePath, "r");
  try {
    const chunk = Buffer.alloc(1 << 20);
    let read: number;
    while ((read = fs.readSync(fd, chunk, 0, chunk.length, null)) > 0) hash.update(chunk.subarray(0, read));
  } finally {
    fs.closeSync(fd);
  }
  return hash.digest("hex");
}

/**
 * Throws unless the .zsync describes `appImagePath` under `assetName`: Filename and URL
 * name the release asset, and SHA-1 and Length match the AppImage's bytes.
 */
export function verifyZsync(zsyncPath: string, appImagePath: string, assetName: string): void {
  const headers = readZsyncHeaders(fs.readFileSync(zsyncPath));
  const problems: string[] = [];
  if (headers.get("Filename") !== assetName) problems.push(`Filename is "${headers.get("Filename")}"`);
  if (headers.get("URL") !== assetName) problems.push(`URL is "${headers.get("URL")}"`);
  const size = fs.statSync(appImagePath).size;
  if (headers.get("Length") !== String(size)) problems.push(`Length is ${headers.get("Length")}, AppImage is ${size} bytes`);
  const sha1 = sha1OfFile(appImagePath);
  if (headers.get("SHA-1")?.toLowerCase() !== sha1) problems.push(`SHA-1 is ${headers.get("SHA-1")}, AppImage is ${sha1}`);
  if (problems.length) {
    throw new Error(`${path.basename(zsyncPath)} doesn't match ${assetName}: ${problems.join("; ")}`);
  }
}

/** Runs zsyncmake; injectable so tests don't need the binary. */
export type ZsyncmakeRunner = (args: string[]) => void;
const runZsyncmake: ZsyncmakeRunner = (args) => {
  execFileSync("zsyncmake", args, { stdio: "inherit" });
};

/**
 * Writes `<dotted AppImage name>.zsync` next to every AppImage in `directory`, replacing
 * any .zsync already there, and verifies each against its AppImage.
 *
 * @returns The .zsync files written.
 * @throws If the directory holds no AppImage, or a generated .zsync doesn't verify.
 */
export function generateAppImageZsyncs(directory: string, run: ZsyncmakeRunner = runZsyncmake): string[] {
  if (!fs.existsSync(directory)) throw new Error(`Directory not found: ${directory}`);
  const appImages = fs.readdirSync(directory).filter((f) => f.endsWith(".AppImage"));
  if (appImages.length === 0) throw new Error(`No .AppImage files found in ${directory}`);

  for (const stale of fs.readdirSync(directory).filter((f) => f.endsWith(".zsync"))) {
    fs.unlinkSync(path.join(directory, stale));
  }

  return appImages.map((appImage) => {
    const appImagePath = path.join(directory, appImage);
    const assetName = toDottedFilename(appImage);
    const zsyncPath = path.join(directory, `${assetName}.zsync`);
    // -f/-u: the Filename and URL headers; a relative URL resolves next to the .zsync
    // in the same release. -o: write beside the AppImage, not into the working directory.
    run(["-f", assetName, "-u", assetName, "-o", zsyncPath, appImagePath]);
    verifyZsync(zsyncPath, appImagePath, assetName);
    return zsyncPath;
  });
}

const isMainModule =
  (typeof (import.meta as { main?: boolean }).main === "boolean" &&
    (import.meta as { main?: boolean }).main) ||
  (process.argv[1] && fileURLToPath(import.meta.url) === path.resolve(process.argv[1]));

if (isMainModule) {
  const targetDir =
    process.argv[2] || path.resolve(process.cwd(), "target/release/bundle/appimage");
  console.log(`Generating .zsync files in ${targetDir}...`);
  for (const zsyncPath of generateAppImageZsyncs(targetDir)) {
    console.log(`Generated and verified: ${zsyncPath}`);
  }
}
