// scripts/sync-docs.ts
// Syncs the locale-specific user guides (and their shared assets) from docs/user-guide/ into
// static/ so the Help view can load them directly (see .gitignore for the generated paths).
import * as fs from "fs";
import * as path from "path";
import { fileURLToPath } from "url";
import { referencedAssets } from "./guide-assets";
import { validateGuideAssets } from "./guide-manifest";

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);
const rootDir = path.resolve(__dirname, "..");
const userGuideDir = path.join(rootDir, "docs", "user-guide");
const staticDir = path.join(rootDir, "static");

// Validate guide assets, manifest, and dimensions before syncing. Fails fast on any mismatch.
const validation = validateGuideAssets({ guideDir: userGuideDir });
if (!validation.valid) {
  for (const issue of validation.issues) {
    console.error(`[sync-docs] Validation error [${issue.code}]: ${issue.message}`);
  }
  process.exit(1);
}

// The guide reuses the app's own fonts rather than keeping a copy in docs/user-guide/.
const FONT_DIRS = ["expose", "fira-sans"];

const FILE_COPIES = [
  "guide.css",
  "guide.js",
  "luminous-mark.svg",
  "signature.svg",
];

const guides = fs.readdirSync(userGuideDir).filter((f) => /^luminous-user-guide-[A-Z]{2}\.html$/.test(f));
for (const file of [...guides, ...FILE_COPIES]) {
  fs.copyFileSync(path.join(userGuideDir, file), path.join(staticDir, file));
}

const fontsOut = path.join(staticDir, "fonts");
fs.mkdirSync(fontsOut, { recursive: true });
for (const dir of FONT_DIRS) {
  const fontsSrc = path.join(rootDir, "src", "lib", "fonts", dir);
  for (const file of fs.readdirSync(fontsSrc).filter((f) => f.endsWith(".woff2"))) {
    fs.copyFileSync(path.join(fontsSrc, file), path.join(fontsOut, file));
  }
}

// Only what the guides display ships: the WebP copies they reference, never the PNG captures
// (those are kept for the Store and website; see guide-assets.ts). Start from an empty folder so
// files left over from an older layout don't get embedded.
const assetsOut = path.join(staticDir, "assets");
fs.rmSync(assetsOut, { recursive: true, force: true });
for (const ref of referencedAssets(userGuideDir)) {
  const src = path.join(userGuideDir, ref);
  if (!fs.existsSync(src)) {
    console.error(`[sync-docs] Missing ${ref}; run \`bun run guide-webp\` to encode it.`);
    process.exit(1);
  }
  const dest = path.join(staticDir, ref);
  fs.mkdirSync(path.dirname(dest), { recursive: true });
  fs.copyFileSync(src, dest);
}

console.log("[sync-docs] Synced user guides and assets into static/");
