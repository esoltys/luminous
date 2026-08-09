// scripts/sync-docs.ts
// Syncs the locale-specific user guides (and their shared assets) from docs/user-guide/ into
// static/ so the Help view can load them directly (see .gitignore for the generated paths).
import * as fs from "fs";
import * as path from "path";
import { fileURLToPath } from "url";

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);
const rootDir = path.resolve(__dirname, "..");
const userGuideDir = path.join(rootDir, "docs", "user-guide");
const staticDir = path.join(rootDir, "static");

function copyDir(src: string, dest: string) {
  fs.mkdirSync(dest, { recursive: true });
  for (const entry of fs.readdirSync(src, { withFileTypes: true })) {
    const srcPath = path.join(src, entry.name);
    const destPath = path.join(dest, entry.name);
    if (entry.isDirectory()) {
      copyDir(srcPath, destPath);
    } else {
      fs.copyFileSync(srcPath, destPath);
    }
  }
}

const HTML_COPIES: Array<[string, string]> = [
  ["luminous-user-guide-EN.dc.html", "luminous-user-guide-EN.html"],
  ["luminous-user-guide-FR.dc.html", "luminous-user-guide-FR.html"],
];

const ASSET_COPIES = ["support.js", "image-slot.js", "luminous-mark.svg", "expose-700.woff2"];

for (const [src, dest] of HTML_COPIES) {
  fs.copyFileSync(path.join(userGuideDir, src), path.join(staticDir, dest));
}

for (const asset of ASSET_COPIES) {
  fs.copyFileSync(path.join(userGuideDir, asset), path.join(staticDir, asset));
}

copyDir(path.join(userGuideDir, "screenshots"), path.join(staticDir, "screenshots"));
copyDir(path.join(userGuideDir, "assets"), path.join(staticDir, "assets"));

console.log("[sync-docs] Synced user guides and assets into static/");
