// Fails when a component dims readable text with an alpha modifier on a theme text colour
// (`text-brand-text-secondary/70`, `hover:text-brand-text-primary/60`…).
//
// Why: the built-in themes guarantee WCAG AA (4.5:1) for `--color-text-primary` and
// `--color-text-secondary` at full strength (colorAccessibility.test.ts), but every alpha
// step below that quietly gives the guarantee back — `/70` of secondary is 2.9:1 on the
// Luminous light theme, `/50` is 2.1:1 (#2344). Express hierarchy with size, weight, or the
// primary-vs-secondary choice instead (docs/UI_CONVENTIONS.md "Readable text has no alpha").
//
// Exemptions: the `disabled:` variant (disabled controls are outside WCAG 1.4.3) and
// `src/lib/utils/lyricsStyle.ts`, where the synced-lyrics fade is the point of the feature.
//
// Usage: bun run scripts/lint-text-alpha.ts
import { readdirSync, readFileSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const SRC = path.join(ROOT, "src");
const EXEMPT_FILES = new Set([path.join(SRC, "lib", "utils", "lyricsStyle.ts")]);

const TEXT_ALPHA = /(?:[\w[\]/-]+:)*text-brand-text-(?:primary|secondary)\/\d+/g;

function walk(dir: string): string[] {
  return readdirSync(dir, { withFileTypes: true }).flatMap((entry) => {
    const full = path.join(dir, entry.name);
    if (entry.isDirectory()) return walk(full);
    return /\.(css|svelte|ts)$/.test(entry.name) &&
      !/\.(test|spec)\.ts$/.test(entry.name) &&
      !EXEMPT_FILES.has(full)
      ? [full]
      : [];
  });
}

const failures: string[] = [];
for (const file of walk(SRC)) {
  readFileSync(file, "utf8")
    .split("\n")
    .forEach((line, i) => {
      for (const match of line.matchAll(TEXT_ALPHA)) {
        if (/(^|:)disabled:/.test(match[0]) || match[0].startsWith("disabled:")) continue;
        failures.push(`${path.relative(ROOT, file)}:${i + 1}  ${match[0]}`);
      }
    });
}

if (failures.length > 0) {
  console.error("Alpha on readable text found (use the full-strength theme text colour, #2344):\n");
  console.error(failures.join("\n"));
  process.exit(1);
}
console.log("lint-text-alpha: ok");
