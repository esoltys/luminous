// Fails when a component colours something with a fixed red/amber/green-family
// Tailwind palette utility (`text-red-400`, `bg-amber-500/10`, `border-emerald-500`…).
//
// Why: state must never be carried by colour alone. A theme can match the
// palette (a red theme swallows red text), and palette values ignore the theme's
// background contrast (amber on a light theme). Use `text-brand-text-primary` /
// `-secondary` and pair the message with an icon, wording or a shape change
// (docs/UI_CONVENTIONS.md "State is never colour-only", #1878).
//
// Gradients (`from-`/`to-`/`via-`) are decorative avatar palettes, not state,
// and are deliberately not matched. There is no allowlist: destructive actions
// and toasts follow the same rule.
//
// Usage: bun run scripts/lint-state-colours.ts
import { readdirSync, readFileSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const SRC = path.join(ROOT, "src");

const STATE_COLOUR =
  /\b(?:text|bg|border|fill|stroke|ring|outline|divide|shadow|decoration)-(?:red|amber|green|emerald|yellow|orange|rose|lime)-\d{2,3}\b/g;

function walk(dir: string): string[] {
  return readdirSync(dir, { withFileTypes: true }).flatMap((entry) => {
    const full = path.join(dir, entry.name);
    if (entry.isDirectory()) return walk(full);
    return /\.(css|svelte|ts)$/.test(entry.name) && !/\.(test|spec)\.ts$/.test(entry.name)
      ? [full]
      : [];
  });
}

const failures: string[] = [];
for (const file of walk(SRC)) {
  readFileSync(file, "utf8")
    .split("\n")
    .forEach((line, i) => {
      for (const match of line.matchAll(STATE_COLOUR)) {
        failures.push(`${path.relative(ROOT, file)}:${i + 1}  ${match[0]}`);
      }
    });
}

if (failures.length > 0) {
  console.error(
    "State-colour utilities found (use theme text colours + an icon/wording, #1878):\n",
  );
  console.error(failures.join("\n"));
  process.exit(1);
}
console.log("lint-state-colours: ok");
