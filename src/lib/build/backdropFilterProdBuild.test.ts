// @vitest-environment node
//
// Regression test for the backdrop-filter production build bug (reopens
// #370, fixed in 0ad3c8a): every glass-panel rule declared unprefixed
// `backdrop-filter` before its `-webkit-backdrop-filter` fallback. Tailwind
// v4's Lightning CSS minifier — the actual minifier this project's Vite
// build uses for production CSS — treats the two as "equivalent"
// declarations and keeps only whichever was declared *last*, silently
// dropping the other. Dev serves unminified CSS with both properties
// intact, so this class of bug only reproduces in a real minified
// production build — a plain source-file grep or a component-level test
// can't catch it, since jsdom never minifies anything.
//
// This test runs `src/app.css` through the actual `@tailwindcss/vite` +
// Vite CSS-minify pipeline (the same one `bun run build` uses), scoped to
// just the CSS entry so it stays fast (~1s) without needing a full
// SvelteKit build.
import { describe, it, expect, afterAll } from "vitest";
import { build } from "vite";
import tailwindcss from "@tailwindcss/vite";
import path from "path";
import fs from "fs";
import os from "os";
import { fileURLToPath } from "url";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const projectRoot = path.resolve(__dirname, "../../..");

let builtCss: string;
let tmpDir: string;

async function buildProdCss(): Promise<string> {
  tmpDir = fs.mkdtempSync(path.join(os.tmpdir(), "backdrop-filter-prod-css-"));
  const entry = path.join(tmpDir, "entry.js");
  fs.writeFileSync(entry, `import ${JSON.stringify(path.join(projectRoot, "src/app.css"))};\n`);
  const outDir = path.join(tmpDir, "out");

  // configFile: false — deliberately skip the project's own vite.config.js,
  // which is wired for the full SvelteKit app (fixed dev port, SvelteKit
  // plugin, etc.) and isn't meant to build a bare entry file. We still use
  // the real `@tailwindcss/vite` plugin and Vite's default production CSS
  // minifier, which is what actually determines whether this bug
  // reproduces.
  await build({
    root: projectRoot,
    configFile: false,
    logLevel: "warn",
    plugins: [tailwindcss()],
    build: {
      outDir,
      emptyOutDir: true,
      write: true,
      minify: false,
      cssMinify: true,
      rollupOptions: { input: entry },
    },
  });

  const assetsDir = path.join(outDir, "assets");
  const cssFile = fs.readdirSync(assetsDir).find(f => f.endsWith(".css"));
  if (!cssFile) {
    throw new Error(`No CSS output found in ${assetsDir}`);
  }
  return fs.readFileSync(path.join(assetsDir, cssFile), "utf-8");
}

afterAll(() => {
  if (tmpDir) fs.rmSync(tmpDir, { recursive: true, force: true });
});

describe("production CSS build: backdrop-filter survives minification", () => {
  it("builds app.css through the real Tailwind v4 + Lightning CSS pipeline", async () => {
    builtCss = await buildProdCss();
    expect(builtCss.length).toBeGreaterThan(0);
  }, 30_000);

  for (const selector of [".glass", ".glass-premium", ".glass-surface"]) {
    it(`keeps both -webkit-backdrop-filter and backdrop-filter on ${selector} after minification`, () => {
      // Match every minified rule block for this selector — Tailwind/Vite may
      // emit more than one block per selector (e.g. an override rule later in
      // the cascade), and each is a separate place the bug could reappear.
      const ruleRegex = new RegExp(
        selector.replace(/[.*+?^${}()|[\]\\]/g, "\\$&") + "\\{[^}]*\\}",
        "g"
      );
      const rules = builtCss.match(ruleRegex);
      expect(rules, `expected at least one minified rule for ${selector}`).toBeTruthy();

      const rulesWithBackdropFilter = (rules ?? []).filter(r => r.includes("backdrop-filter"));
      expect(
        rulesWithBackdropFilter.length,
        `expected at least one ${selector} rule declaring backdrop-filter`
      ).toBeGreaterThan(0);

      for (const rule of rulesWithBackdropFilter) {
        expect(rule, `${selector} rule dropped -webkit-backdrop-filter: ${rule}`).toContain(
          "-webkit-backdrop-filter"
        );
        // A bare (non-prefixed) backdrop-filter declaration — i.e. not
        // immediately preceded by "-webkit-".
        expect(
          /(?<!-webkit-)backdrop-filter:/.test(rule),
          `${selector} rule dropped the standard backdrop-filter: ${rule}`
        ).toBe(true);
      }
    });
  }
});
