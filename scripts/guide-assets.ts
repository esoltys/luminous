// scripts/guide-assets.ts
// Decides which user-guide assets ship inside the app, and keeps their WebP copies current.
//
// docs/user-guide/assets/ holds the PNG captures. They stay PNG because the Microsoft Store
// listing and the esoltys.dev landing page take them as-is. The app only needs the subset the
// guides actually show, so each guide references a lossy WebP copy (`<name>.webp` next to
// `<name>.png`) that is committed alongside the PNG. Committing the copies means the app build
// (including the offline Flatpak build) never needs an image encoder; only the capture scripts
// do. Usage: bun run guide-webp   (re-encodes any WebP copy that is missing or older than its PNG)
import * as fs from "fs";
import * as path from "path";
import { spawnSync } from "child_process";
import { fileURLToPath } from "url";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
export const USER_GUIDE_DIR = path.resolve(__dirname, "..", "docs", "user-guide");

const GUIDE_FILE = /^luminous-user-guide-[A-Z]{2}\.html$/;
const ASSET_REF = /\b(?:src|href)="(assets\/[^"#?]+)"/g;
// Lossy WebP at 80 keeps UI text legible while cutting a capture to about a fifth of the PNG.
const WEBP_QUALITY = 80;

/**
 * Every asset the guides can display, as paths relative to the user-guide folder. The markup
 * names the dark capture; guide.js swaps in the light one, so both are included when present.
 */
export function referencedAssets(guideDir: string = USER_GUIDE_DIR): string[] {
  const refs = new Set<string>();
  for (const file of fs.readdirSync(guideDir).filter((f) => GUIDE_FILE.test(f))) {
    const html = fs.readFileSync(path.join(guideDir, file), "utf8");
    for (const [, ref] of html.matchAll(ASSET_REF)) {
      refs.add(ref);
      if (ref.includes("/dark/")) refs.add(ref.replace("/dark/", "/light/"));
    }
  }
  // A light twin that was never captured is fine: guide.js falls back to the dark one.
  return [...refs]
    .filter(
      (ref) => fs.existsSync(path.join(guideDir, ref)) || fs.existsSync(pngMaster(guideDir, ref)),
    )
    .sort();
}

function pngMaster(guideDir: string, ref: string): string {
  return path.join(guideDir, ref.replace(/\.webp$/, ".png"));
}

/**
 * Re-encodes the WebP copy of every referenced capture whose copy is missing or older than its
 * PNG. Needs ImageMagick (`magick`) on PATH; without it, warns and leaves the copies alone.
 */
export function encodeGuideWebp(guideDir: string = USER_GUIDE_DIR): {
  encoded: string[];
  failed: string[];
} {
  const encoded: string[] = [];
  const failed: string[] = [];
  for (const ref of referencedAssets(guideDir).filter((r) => r.endsWith(".webp"))) {
    const png = pngMaster(guideDir, ref);
    const webp = path.join(guideDir, ref);
    if (!fs.existsSync(png)) continue;
    if (fs.existsSync(webp) && fs.statSync(webp).mtimeMs >= fs.statSync(png).mtimeMs) continue;

    const result = spawnSync("magick", [
      png,
      "-quality",
      String(WEBP_QUALITY),
      "-define",
      "webp:method=6",
      webp,
    ]);
    if (result.error && (result.error as NodeJS.ErrnoException).code === "ENOENT") {
      console.warn(
        "[guide-webp] ImageMagick (`magick`) is not on PATH, so the guide's WebP copies were not updated. Install it and run `bun run guide-webp`.",
      );
      return { encoded, failed };
    }
    if (result.status === 0) encoded.push(ref);
    else failed.push(ref);
  }
  if (encoded.length)
    console.log(
      `[guide-webp] Encoded ${encoded.length} WebP cop${encoded.length === 1 ? "y" : "ies"}.`,
    );
  for (const ref of failed) console.warn(`[guide-webp] Failed to encode ${ref}`);
  return { encoded, failed };
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const { failed } = encodeGuideWebp();
  if (failed.length) process.exitCode = 1;
}
