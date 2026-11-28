// Fails when CSS reads a custom property with no fallback — `var(--x)` or
// Tailwind's `bg-(--x)` shorthand — and nothing in the codebase defines --x.
//
// Why a lint and not a code review habit: a var() naming an undefined
// property doesn't lose the cascade, it *wins* it and then becomes invalid at
// computed-value time, which silently computes to `unset`. The element just
// turns transparent or loses its border — often in only one theme, with no
// console error. (So when an element is transparent even though a rule
// clearly gives it a colour, suspect a broken var() before the cascade.)
//
// Writers: `--x:` declarations (CSS, <style> blocks, style attributes, CSS
// built in TS template strings), `@property --x`, `setProperty("--x", …)`,
// Tailwind's default theme (node_modules/tailwindcss/theme.css), and
// HOST_PROVIDED below.
// Readers: `var(--x…)` anywhere in src (tests excluded) and Tailwind
// `…-(--x)` class shorthand. A `var(--prefix-${…})` template read passes if
// any writer starts with that prefix.
//
// Theme-colour classes: `bg-brand-main` reads --color-brand-main too, and an
// unknown name fails the same silent way, except Tailwind emits no rule at all
// (#2442). Those reads must name a token in an @theme block: a --color-brand-*
// written only at runtime (setProperty) defines no utility, so the general
// writer set doesn't count for them.
//
// Usage: bun run scripts/lint-css-vars.ts [--unused]
//   --unused also lists (without failing) properties written but never read,
//   and fallback-guarded reads whose name is written nowhere — a typo hidden
//   behind a fallback.
import { readdirSync, readFileSync } from "node:fs";
import { createRequire } from "node:module";
import path from "node:path";
import { fileURLToPath } from "node:url";

const ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const SRC = path.join(ROOT, "src");
// Resolve through module lookup, not a hard-coded path: a worktree has no
// node_modules of its own and finds packages in the main checkout's (#1282).
const TAILWIND_THEME = createRequire(import.meta.url).resolve("tailwindcss/theme.css");

/** Properties provided by something other than our source, each with its reason. */
const HOST_PROVIDED: { pattern: RegExp; reason: string }[] = [
  {
    pattern: /^--tw-/,
    reason: "Tailwind utilities declare their own --tw-* internals per element",
  },
];

interface Read {
  name: string;
  file: string;
  line: number;
  hasFallback: boolean;
  /** Name ends in a template interpolation: `var(--color-artwork-${key})`. */
  isPrefix: boolean;
}

function walk(dir: string): string[] {
  return readdirSync(dir, { withFileTypes: true }).flatMap((entry) => {
    const full = path.join(dir, entry.name);
    if (entry.isDirectory()) return walk(full);
    return /\.(css|svelte|ts)$/.test(entry.name) && !/\.(test|spec)\.ts$/.test(entry.name)
      ? [full]
      : [];
  });
}

/**
 * Blanks out comments (keeping newlines, so line numbers survive) so a
 * commented-out var() or declaration doesn't count. `//` only counts at the
 * start of a line or after whitespace, so URLs like https:// survive.
 */
function stripComments(text: string, file: string): string {
  const blank = (m: string) => m.replace(/[^\n]/g, " ");
  let out = text.replace(/\/\*[\s\S]*?\*\//g, blank).replace(/<!--[\s\S]*?-->/g, blank);
  if (!file.endsWith(".css")) out = out.replace(/(^|[ \t])\/\/[^\n]*/gm, blank);
  return out;
}

function lineOf(text: string, index: number): number {
  let line = 1;
  for (let i = 0; i < index; i++) if (text.charCodeAt(i) === 10) line++;
  return line;
}

function collectWriters(text: string, into: Set<string>) {
  for (const m of text.matchAll(/(?<![\w-])(--[\w-]+)["'`]?\s*:/g)) into.add(m[1]);
  for (const m of text.matchAll(/@property\s+(--[\w-]+)/g)) into.add(m[1]);
  for (const m of text.matchAll(/setProperty\(\s*["'`](--[\w-]+)["'`]/g)) into.add(m[1]);
}

/**
 * A colour utility naming a `brand-*` theme colour, with any variant prefix
 * (`hover:`, `group-hover:`) and opacity suffix (`/40`) around it. The name
 * is matched whole, so `accent-text-hover` is never read as `accent`.
 */
const BRAND_COLOUR_CLASS =
  /(?<![\w-])(?:bg|text|border(?:-[trblxyse])?|ring(?:-offset)?|outline|fill|stroke|shadow|from|via|to|divide|decoration|accent|caret|placeholder)-(brand-[a-z](?:[a-z-]*[a-z])?)(?![\w-])/g;

interface ThemeColourRead {
  /** The colour as the class spells it (`brand-main`); its token is `--color-${colour}`. */
  colour: string;
  file: string;
  line: number;
}

function collectThemeColourReads(text: string, file: string, into: ThemeColourRead[]) {
  const rel = path.relative(ROOT, file).replaceAll("\\", "/");
  for (const m of text.matchAll(BRAND_COLOUR_CLASS)) {
    into.push({ colour: m[1], file: rel, line: lineOf(text, m.index) });
  }
}

function collectThemeTokens(text: string, into: Set<string>) {
  for (const block of text.matchAll(/@theme[^{]*\{([^}]*)\}/g)) collectWriters(block[1], into);
}

function collectReads(text: string, file: string, into: Read[]) {
  const rel = path.relative(ROOT, file).replaceAll("\\", "/");
  for (const m of text.matchAll(/var\(\s*(--[\w-]+)(\$\{)?\s*(,)?/g)) {
    into.push({
      name: m[1],
      file: rel,
      line: lineOf(text, m.index),
      isPrefix: !!m[2],
      hasFallback: !!m[3],
    });
  }
  // Tailwind v4 shorthand: bg-(--glass-bg), shadow-(--glass-shadow)
  for (const m of text.matchAll(/[\w\]]-\((--[\w-]+)\)/g)) {
    into.push({
      name: m[1],
      file: rel,
      line: lineOf(text, m.index),
      isPrefix: false,
      hasFallback: false,
    });
  }
}

function main() {
  const reportUnused = process.argv.includes("--unused");
  const writers = new Set<string>();
  const reads: Read[] = [];
  const themeTokens = new Set<string>();
  const themeColourReads: ThemeColourRead[] = [];

  collectWriters(stripComments(readFileSync(TAILWIND_THEME, "utf8"), TAILWIND_THEME), writers);
  const files = walk(SRC);
  for (const file of files) {
    const text = stripComments(readFileSync(file, "utf8"), file);
    collectWriters(text, writers);
    collectReads(text, file, reads);
    collectThemeTokens(text, themeTokens);
    collectThemeColourReads(text, file, themeColourReads);
  }

  const isWritten = (read: Read) =>
    HOST_PROVIDED.some(({ pattern }) => pattern.test(read.name)) ||
    (read.isPrefix ? [...writers].some((w) => w.startsWith(read.name)) : writers.has(read.name));

  const broken = reads.filter((r) => !r.hasFallback && !isWritten(r));
  const brokenClasses = themeColourReads.filter((r) => !themeTokens.has(`--color-${r.colour}`));

  if (reportUnused) {
    // @theme tokens are read by the utilities Tailwind generates from them
    // (--color-brand-main → bg-brand-main), not by var(), so they don't count.
    const ownWriters = new Set<string>();
    for (const file of files)
      collectWriters(stripComments(readFileSync(file, "utf8"), file), ownWriters);
    const unread = [...ownWriters].filter(
      (w) =>
        !themeTokens.has(w) &&
        !reads.some((r) => (r.isPrefix ? w.startsWith(r.name) : r.name === w)),
    );
    const hiddenTypos = reads.filter((r) => r.hasFallback && !isWritten(r));
    console.log(
      `Written in src but never read via var(), excluding @theme tokens (${unread.length}) — check for JS readers before deleting:`,
    );
    for (const name of unread.sort()) console.log(`  ${name}`);
    console.log(`\nFallback-guarded reads of a property written nowhere (${hiddenTypos.length}):`);
    for (const r of hiddenTypos) console.log(`  ${r.file}:${r.line}  ${r.name}`);
    console.log();
  }

  if (brokenClasses.length > 0) {
    console.error(
      `✖ ${brokenClasses.length} theme-colour class${brokenClasses.length === 1 ? " names" : "es name"} a colour no @theme block defines:`,
    );
    for (const r of brokenClasses) console.error(`  ${r.file}:${r.line}  ${r.colour}`);
    console.error(
      "\nTailwind emits no rule for an unknown theme colour, so the element is left transparent. Use a --color-brand-* token from src/app.css.\n",
    );
  }
  if (broken.length > 0) {
    console.error(
      `✖ ${broken.length} custom propert${broken.length === 1 ? "y is" : "ies are"} read with no fallback and never defined:`,
    );
    for (const r of broken)
      console.error(`  ${r.file}:${r.line}  ${r.name}${r.isPrefix ? "*" : ""}`);
    console.error(
      "\nAn undefined var() computes to `unset` (transparent/missing) with no error. Define it, add a fallback, or fix the name.",
    );
  }
  if (broken.length > 0 || brokenClasses.length > 0) process.exit(1);
  console.log(
    `✔ CSS custom properties: ${reads.length} reads and ${themeColourReads.length} theme-colour classes across ${files.length} files, all defined or guarded by a fallback.`,
  );
}

main();
