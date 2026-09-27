// Proves a CSS/component refactor is visually neutral: captures the computed
// style and box of every visible element (plus a screenshot) across a matrix
// of light/dark and layout states, then diffs two captures element by element.
//
// Why not just screenshots: a pixel diff misses sub-pixel moves and colour
// changes too small to see, and when it does fire it can't say *which*
// element and property moved. The computed-style diff can.
//
// Runs against the mocked Tauri IPC harness (same as take-screenshots.ts), so
// the library, playback position and settings are identical run to run.
// Whatever still varies (a clock, an animation frame) is found empirically:
// capture the unchanged app twice and pass the second run as --noise, and
// any element, property or pixel that differs between those two runs is
// masked out of the comparison.
//
// Usage:
//   bun run style-diff capture <name> [--states=a,b]
//   bun run style-diff compare <before> <after> [--noise=<before2>]
//   bun run style-diff states
// Captures land in .style-diff/<name>/ (gitignored). compare exits 1 on any
// unmasked difference and writes <after>/<state>.diff.png for pixel changes.
import * as fs from "fs";
import * as path from "path";
import { fileURLToPath } from "url";
import type { Browser, Page } from "playwright";
import { compileMockScript } from "./compile-mock-script";
import { DEV_SERVER_URL, startViteDevServer } from "./vite-dev-server";
import { loadMockConfig, loadMockLibrary, resolveFeatured } from "./mock-library";

const ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const OUT = path.join(ROOT, ".style-diff");

// tsx compiles with esbuild's keepNames, which wraps named inner functions in
// a __name() helper that doesn't exist in the page functions are serialised to.
const NAME_SHIM = "globalThis.__name = (fn) => fn;";

interface State {
  name: string;
  colorScheme: "light" | "dark";
  width: number;
  height: number;
  miniplayer?: boolean;
}

// All on the "system" theme, which follows the OS light/dark preference; the
// named themes only swap palette tokens. Glass is always on, so no glass axis.
const STATES: State[] = [
  { name: "system-dark", colorScheme: "dark", width: 1280, height: 800 },
  { name: "system-light", colorScheme: "light", width: 1280, height: 800 },
  // Below PLAYBAR_ONLY_HEIGHT_BREAKPOINT_PX (src/lib/constants.ts).
  { name: "playbar-only", colorScheme: "dark", width: 1280, height: 120 },
  { name: "miniplayer", colorScheme: "dark", width: 300, height: 360, miniplayer: true },
];

// Everything that affects how an element looks or where it sits. Longhands
// only: shorthands are derived from these and would double-report.
const PROPS = [
  "display", "position", "top", "right", "bottom", "left", "z-index", "float",
  "box-sizing", "width", "height", "min-width", "min-height", "max-width", "max-height",
  "margin-top", "margin-right", "margin-bottom", "margin-left",
  "padding-top", "padding-right", "padding-bottom", "padding-left",
  "border-top-width", "border-right-width", "border-bottom-width", "border-left-width",
  "border-top-style", "border-right-style", "border-bottom-style", "border-left-style",
  "border-top-color", "border-right-color", "border-bottom-color", "border-left-color",
  "border-top-left-radius", "border-top-right-radius", "border-bottom-right-radius", "border-bottom-left-radius",
  "outline-width", "outline-style", "outline-color", "outline-offset",
  "flex-direction", "flex-wrap", "flex-grow", "flex-shrink", "flex-basis", "order",
  "justify-content", "align-items", "align-self", "align-content", "gap", "row-gap", "column-gap",
  "grid-template-columns", "grid-template-rows", "grid-column-start", "grid-column-end",
  "grid-row-start", "grid-row-end",
  "color", "background-color", "background-image", "background-size", "background-position", "background-clip",
  "opacity", "visibility", "box-shadow", "text-shadow", "filter", "backdrop-filter", "mix-blend-mode",
  "transform", "transform-origin", "clip-path", "mask-image", "overflow-x", "overflow-y",
  "font-family", "font-size", "font-weight", "font-style", "line-height", "letter-spacing",
  "text-align", "text-transform", "text-decoration-line", "text-decoration-color", "text-overflow",
  "white-space", "word-break", "vertical-align",
  "cursor", "pointer-events", "user-select", "fill", "stroke", "stroke-width",
  "transition-property", "transition-duration", "transition-timing-function", "transition-delay",
  "animation-name", "animation-duration",
];

/** One element's snapshot: its box plus every property in PROPS (and its pseudo-elements'). */
type ElementRecord = Record<string, string>;

interface StateCapture {
  state: string;
  /** Custom properties set on <html> and <body>, keyed like "html --glass-bg". */
  customProperties: Record<string, string>;
  /**
   * PROPS as computed on a bare <div> in <body>. Element records omit any
   * property equal to this (most of them), which keeps a capture ~10× smaller.
   */
  baseline: Record<string, string>;
  /** Keyed by structural path. */
  elements: Record<string, ElementRecord>;
}

function parseFlag(argv: string[], flag: string): string | undefined {
  const arg = argv.find((a) => a.startsWith(`--${flag}=`));
  return arg?.slice(flag.length + 3);
}

async function capture(name: string, stateFilter?: string[]) {
  const states = stateFilter ? STATES.filter((s) => stateFilter.includes(s.name)) : STATES;
  const unknown = stateFilter?.filter((f) => !STATES.some((s) => s.name === f)) ?? [];
  if (unknown.length > 0) throw new Error(`Unknown state(s): ${unknown.join(", ")}. Run "states" to list them.`);

  const dir = path.join(OUT, name);
  fs.mkdirSync(dir, { recursive: true });

  const { chromium } = await import("playwright");
  console.log("Starting Vite dev server...");
  const killDevServer = await startViteDevServer();
  const browser = await chromium.launch({ headless: true });
  try {
    const mockConfig = loadMockConfig();
    const library = await loadMockLibrary(mockConfig);
    const featured = resolveFeatured(library, {
      featuredSong: mockConfig.default?.featuredSong,
      featuredArtist: mockConfig.default?.featuredArtist,
      featuredAlbum: mockConfig.default?.featuredAlbum,
    });
    const setup = {
      library: JSON.stringify(library),
      featured: JSON.stringify(featured),
      mockCode: compileMockScript(),
      positionSeconds: mockConfig.default?.positionSeconds ?? 122,
    };
    for (const [i, state] of states.entries()) {
      console.log(`[${i + 1}/${states.length}] ${state.name}`);
      const result = await captureState(browser, state, setup, path.join(dir, `${state.name}.png`));
      fs.writeFileSync(path.join(dir, `${state.name}.json`), JSON.stringify(result));
    }
  } finally {
    await browser.close();
    killDevServer();
  }
  console.log(`Captured ${states.length} state(s) to ${path.relative(ROOT, dir)}`);
}

async function captureState(
  browser: Browser,
  state: State,
  setup: { library: string; featured: string; mockCode: string; positionSeconds: number },
  pngPath: string,
): Promise<StateCapture> {
  const page = await browser.newPage({ viewport: { width: state.width, height: state.height } });
  await page.emulateMedia({ colorScheme: state.colorScheme });
  page.on("pageerror", (err) => console.warn(`  [page error] ${err.message}`));

  await page.addInitScript(NAME_SHIM);
  await page.addInitScript(`
    window.__LUMINOUS_MOCK_LIBRARY__ = ${setup.library};
    window.__LUMINOUS_MOCK_FEATURED__ = ${setup.featured};
  `);
  await page.addInitScript(setup.mockCode);
  await page.addInitScript(`
    window.mockSettings = {
      ...(window.mockSettings || {}),
      active_theme_id: "system",
      custom_themes: "[]",
      active_tab: "collection",
      active_sub_tab: "albums",
      language: "en",
      walkthrough_completed: "true",
      welcome_seen: "true"
    };
    window.mockPlaybackPositionSec = ${setup.positionSeconds};
    // Paused, so the seek bar and anything keyed to playback stand still.
    window.mockPlayState = "paused";
    localStorage.setItem("layout_immersiveMode", "false");
    localStorage.setItem("layout_sidebarOpen", "true");
    localStorage.setItem("layout_rightPanelOpen", "false");
    localStorage.setItem("layout_isMiniplayer", ${JSON.stringify(String(!!state.miniplayer))});
  `);

  await page.goto(DEV_SERVER_URL);
  // Generous: a cold dev server can re-optimise deps and reload mid-mount.
  await page.waitForSelector(".flex-1", { timeout: 60000 });
  await page.waitForTimeout(1500);
  try {
    await loadAllImages(page);
  } catch (err) {
    // Vite's dependency re-optimisation reloaded the page mid-wait.
    if (!String(err).includes("Execution context was destroyed")) throw err;
    await page.waitForSelector(".flex-1", { timeout: 30000 });
    await loadAllImages(page);
  }
  await settle(page);

  await page.screenshot({ path: pngPath, animations: "disabled" });
  const result = await page.evaluate(snapshotPage, PROPS);
  await page.close();
  return { state: state.name, ...result };
}

async function loadAllImages(page: Page) {
  // Cover art is loading="lazy"; below-the-fold images never load without a
  // real scroll, so force them eager or their boxes vary with timing.
  await page.evaluate(async () => {
    await Promise.all(
      Array.from(document.querySelectorAll("img")).map((img) => {
        if (img.loading === "lazy") img.loading = "eager";
        if (img.complete) return;
        return new Promise((resolve) => {
          img.addEventListener("load", resolve);
          img.addEventListener("error", resolve);
        });
      }),
    );
  });
}

/** Jumps every animation to a fixed frame and hides visualisers (signal, not chrome). */
async function settle(page: Page) {
  await page.waitForTimeout(400);
  await page.evaluate(() => {
    for (const animation of document.getAnimations()) {
      const timing = animation.effect?.getComputedTiming();
      if (timing && Number.isFinite(timing.endTime as number)) {
        animation.finish();
      } else {
        animation.pause();
        animation.currentTime = 0;
      }
    }
    for (const canvas of document.querySelectorAll("canvas")) canvas.style.visibility = "hidden";
  });
}

// Runs in the page. Must be self-contained (Playwright serialises it).
function snapshotPage(props: string[]) {
  const customProperties: Record<string, string> = {};
  for (const [label, el] of [["html", document.documentElement], ["body", document.body]] as const) {
    const cs = getComputedStyle(el);
    for (let i = 0; i < cs.length; i++) {
      const name = cs[i];
      if (name.startsWith("--")) customProperties[`${label} ${name}`] = cs.getPropertyValue(name).trim();
    }
  }

  const probe = document.body.appendChild(document.createElement("div"));
  const probeStyle = getComputedStyle(probe);
  const baseline: Record<string, string> = {};
  for (const p of props) baseline[p] = probeStyle.getPropertyValue(p);
  probe.remove();
  const record = (into: Record<string, string>, prefix: string, cs: CSSStyleDeclaration) => {
    for (const p of props) {
      const value = cs.getPropertyValue(p);
      if (value !== baseline[p]) into[prefix + p] = value;
    }
  };

  const elements: Record<string, Record<string, string>> = {};
  const round = (n: number) => String(Math.round(n * 100) / 100);

  // Key: tag plus index among same-tag siblings, with the id where there is
  // one, so an unrelated sibling appearing doesn't re-key the whole subtree.
  const walk = (el: Element, key: string) => {
    if (el.tagName === "CANVAS" || el.tagName === "SCRIPT" || el.tagName === "STYLE") return;
    const cs = getComputedStyle(el);
    if (cs.display === "none") return;
    const rect = el.getBoundingClientRect();
    const visible = rect.width > 0 && rect.height > 0 && cs.visibility !== "hidden";
    if (visible) {
      const entry: Record<string, string> = {
        "box.x": round(rect.x),
        "box.y": round(rect.y),
        "box.width": round(rect.width),
        "box.height": round(rect.height),
      };
      record(entry, "", cs);
      for (const pseudo of ["::before", "::after"]) {
        const ps = getComputedStyle(el, pseudo);
        if (ps.content === "none" || ps.content === "normal") continue;
        entry[`${pseudo} content`] = ps.content;
        record(entry, `${pseudo} `, ps);
      }
      const ownText = Array.from(el.childNodes)
        .filter((n) => n.nodeType === Node.TEXT_NODE)
        .map((n) => n.textContent?.trim())
        .join(" ")
        .trim();
      if (ownText) entry.text = ownText.slice(0, 80);
      // Labels the element in reports, and makes a class swap show up as the cause.
      const className = el.getAttribute("class")?.trim();
      if (className) entry.class = className;
      elements[key] = entry;
    }
    const counts: Record<string, number> = {};
    for (const child of el.children) {
      const tag = child.tagName.toLowerCase();
      const n = (counts[tag] = (counts[tag] ?? 0) + 1) - 1;
      walk(child, `${key} > ${tag}${child.id ? `#${child.id}` : ""}[${n}]`);
    }
  };
  walk(document.body, "body");
  return { customProperties, baseline, elements };
}

interface Report {
  appeared: string[];
  disappeared: string[];
  changed: { key: string; label?: string; props: { prop: string; before: string; after: string }[] }[];
  customProperties: { prop: string; before?: string; after?: string }[];
  pixels: { changed: number; masked: number; bounds?: { x: number; y: number; width: number; height: number } };
}

function diffRecords(before: Record<string, string>, after: Record<string, string>, noise?: Record<string, string>) {
  const out: { prop: string; before: string; after: string }[] = [];
  for (const prop of new Set([...Object.keys(before), ...Object.keys(after)])) {
    if (before[prop] === after[prop]) continue;
    if (noise && noise[prop] !== before[prop]) continue;
    out.push({ prop, before: before[prop] ?? "(unset)", after: after[prop] ?? "(unset)" });
  }
  return out;
}

function diffState(before: StateCapture, after: StateCapture, noise?: StateCapture): Omit<Report, "pixels"> {
  // An element that comes and goes between two unchanged runs is noise.
  const flaky = (key: string) => noise !== undefined && !(key in noise.elements) !== !(key in before.elements);
  const appeared = Object.keys(after.elements).filter((k) => !(k in before.elements) && !flaky(k));
  const disappeared = Object.keys(before.elements).filter((k) => !(k in after.elements) && !flaky(k));
  const changed = Object.keys(before.elements)
    .filter((k) => k in after.elements && !flaky(k))
    .map((key) => ({
      key,
      label: after.elements[key].class,
      props: diffRecords(before.elements[key], after.elements[key], noise?.elements[key]),
    }))
    .filter((c) => c.props.length > 0);
  const customProperties = diffRecords(before.customProperties, after.customProperties, noise?.customProperties);
  return { appeared, disappeared, changed, customProperties };
}

/** Pixel diff in a headless page (canvas getImageData), so no PNG library is needed. */
async function diffPixels(page: Page, before: string, after: string, noise: string | undefined, diffPath: string) {
  const toDataUrl = (file: string) => `data:image/png;base64,${fs.readFileSync(file).toString("base64")}`;
  const result = await page.evaluate(
    async ([b, a, n]) => {
      const load = (src: string) =>
        new Promise<HTMLImageElement>((resolve, reject) => {
          const img = new Image();
          img.onload = () => resolve(img);
          img.onerror = reject;
          img.src = src;
        });
      const pixels = async (src: string, w: number, h: number) => {
        const canvas = document.createElement("canvas");
        canvas.width = w;
        canvas.height = h;
        const ctx = canvas.getContext("2d")!;
        ctx.drawImage(await load(src), 0, 0);
        return ctx.getImageData(0, 0, w, h);
      };
      const first = await load(b);
      const w = first.naturalWidth;
      const h = first.naturalHeight;
      const [pb, pa, pn] = await Promise.all([pixels(b, w, h), pixels(a, w, h), n ? pixels(n, w, h) : undefined]);
      const out = new ImageData(w, h);
      let changed = 0;
      let masked = 0;
      let minX = w, minY = h, maxX = -1, maxY = -1;
      for (let i = 0; i < pb.data.length; i += 4) {
        const differs = (x: ImageData, y: ImageData) =>
          x.data[i] !== y.data[i] || x.data[i + 1] !== y.data[i + 1] || x.data[i + 2] !== y.data[i + 2];
        // Faint grey copy of the "after" frame, with real changes in red.
        const grey = (pa.data[i] + pa.data[i + 1] + pa.data[i + 2]) / 3;
        out.data.set([grey, grey, grey, 60], i);
        if (!differs(pb, pa)) continue;
        if (pn && differs(pb, pn)) {
          masked++;
          continue;
        }
        changed++;
        out.data.set([255, 0, 0, 255], i);
        const x = (i / 4) % w;
        const y = Math.floor(i / 4 / w);
        minX = Math.min(minX, x);
        minY = Math.min(minY, y);
        maxX = Math.max(maxX, x);
        maxY = Math.max(maxY, y);
      }
      let png: string | undefined;
      if (changed > 0) {
        const canvas = document.createElement("canvas");
        canvas.width = w;
        canvas.height = h;
        canvas.getContext("2d")!.putImageData(out, 0, 0);
        png = canvas.toDataURL("image/png");
      }
      const bounds = changed > 0 ? { x: minX, y: minY, width: maxX - minX + 1, height: maxY - minY + 1 } : undefined;
      return { changed, masked, bounds, png };
    },
    [toDataUrl(before), toDataUrl(after), noise ? toDataUrl(noise) : undefined] as const,
  );
  if (result.png) fs.writeFileSync(diffPath, Buffer.from(result.png.split(",")[1], "base64"));
  else if (fs.existsSync(diffPath)) fs.rmSync(diffPath);
  return { changed: result.changed, masked: result.masked, bounds: result.bounds };
}

async function compare(beforeName: string, afterName: string, noiseName?: string) {
  const dirOf = (name: string) => {
    const dir = path.join(OUT, name);
    if (!fs.existsSync(dir)) throw new Error(`No capture named "${name}" in ${path.relative(ROOT, OUT)}.`);
    return dir;
  };
  const [beforeDir, afterDir] = [dirOf(beforeName), dirOf(afterName)];
  const noiseDir = noiseName ? dirOf(noiseName) : undefined;
  if (!noiseDir) console.warn("No --noise baseline: anything that varies between runs will be reported as a change.\n");

  const read = (dir: string, state: string): StateCapture => {
    const capture: StateCapture = JSON.parse(fs.readFileSync(path.join(dir, `${state}.json`), "utf8"));
    // Re-inflate the properties the capture omitted for matching the baseline,
    // so a baseline change still shows up on every element it reaches.
    const withBaseline = (prefix: string) =>
      Object.fromEntries(Object.entries(capture.baseline).map(([p, v]) => [prefix + p, v]));
    const base = withBaseline("");
    const pseudoBase = { "::before": withBaseline("::before "), "::after": withBaseline("::after ") };
    for (const [key, entry] of Object.entries(capture.elements)) {
      capture.elements[key] = {
        ...base,
        ...("::before content" in entry ? pseudoBase["::before"] : {}),
        ...("::after content" in entry ? pseudoBase["::after"] : {}),
        ...entry,
      };
    }
    return capture;
  };
  const statesIn = (dir: string) =>
    fs.readdirSync(dir).filter((f) => f.endsWith(".json")).map((f) => f.slice(0, -".json".length));
  const states = statesIn(beforeDir).filter((s) => statesIn(afterDir).includes(s));
  const skipped = [...new Set([...statesIn(beforeDir), ...statesIn(afterDir)])].filter((s) => !states.includes(s));
  if (skipped.length > 0) console.warn(`Only in one capture, skipped: ${skipped.join(", ")}\n`);

  const { chromium } = await import("playwright");
  const browser = await chromium.launch({ headless: true });
  const page = await browser.newPage();
  await page.evaluate(NAME_SHIM);
  let dirty = 0;
  try {
    for (const state of states) {
      const noiseHas = noiseDir && fs.existsSync(path.join(noiseDir, `${state}.json`));
      const report: Report = {
        ...diffState(read(beforeDir, state), read(afterDir, state), noiseHas ? read(noiseDir, state) : undefined),
        pixels: await diffPixels(
          page,
          path.join(beforeDir, `${state}.png`),
          path.join(afterDir, `${state}.png`),
          noiseHas ? path.join(noiseDir, `${state}.png`) : undefined,
          path.join(afterDir, `${state}.diff.png`),
        ),
      };
      const clean =
        report.appeared.length + report.disappeared.length + report.changed.length +
          report.customProperties.length + report.pixels.changed === 0;
      const masked = report.pixels.masked > 0 ? ` (${report.pixels.masked} noisy px masked)` : "";
      if (clean) {
        console.log(`✔ ${state}${masked}`);
        continue;
      }
      dirty++;
      console.log(`✖ ${state}${masked}`);
      printReport(report, path.relative(ROOT, path.join(afterDir, `${state}.diff.png`)));
    }
  } finally {
    await browser.close();
  }
  if (dirty > 0) {
    console.error(`\n${dirty} of ${states.length} state(s) differ.`);
    process.exit(1);
  }
  console.log(`\nNo differences across ${states.length} state(s).`);
}

const LIST_LIMIT = 15;
const PROPS_PER_ELEMENT = 6;

// Properties that move whenever layout reflows, so one authored change (a
// padding) can touch thousands of elements through them. Elements whose only
// changes are these are counted rather than listed.
const DERIVED = new Set([
  "box.x", "box.y", "box.width", "box.height", "width", "height",
  "transform-origin", "perspective-origin", "grid-template-rows", "grid-template-columns",
]);

const clip = (value: string) => (value.length > 60 ? `${value.slice(0, 57)}…` : value);

function printReport(report: Report, diffPng: string) {
  const list = (title: string, items: string[], lines = (item: string) => [item]) => {
    if (items.length === 0) return;
    console.log(`  ${title} (${items.length}):`);
    for (const item of items.slice(0, LIST_LIMIT)) for (const line of lines(item)) console.log(`    ${line}`);
    if (items.length > LIST_LIMIT) console.log(`    … ${items.length - LIST_LIMIT} more`);
  };
  list(
    "Custom properties changed",
    report.customProperties.map((c) => `${c.prop}: ${clip(c.before ?? "(unset)")} → ${clip(c.after ?? "(unset)")}`),
  );
  list("Elements appeared", report.appeared.map(shortKey));
  list("Elements disappeared", report.disappeared.map(shortKey));

  const byProp = new Map<string, number>();
  for (const { props } of report.changed) for (const { prop } of props) byProp.set(prop, (byProp.get(prop) ?? 0) + 1);
  const authored = report.changed.filter(({ props }) => props.some((p) => !DERIVED.has(p.prop)));
  const byKey = new Map(authored.map((c) => [c.key, c]));
  list("Elements changed", authored.map((c) => c.key), (key) => {
    const { props, label } = byKey.get(key)!;
    // Authored properties first: they're the likely cause.
    const sorted = [...props].sort((a, b) => Number(DERIVED.has(a.prop)) - Number(DERIVED.has(b.prop)));
    return [
      `${shortKey(key)}${label ? `  .${clip(label).replace(/\s+/g, ".")}` : ""}`,
      ...sorted.slice(0, PROPS_PER_ELEMENT).map((p) => `    ${p.prop}: ${clip(p.before)} → ${clip(p.after)}`),
      ...(sorted.length > PROPS_PER_ELEMENT ? [`    … ${sorted.length - PROPS_PER_ELEMENT} more properties`] : []),
    ];
  });
  const reflowed = report.changed.length - authored.length;
  if (reflowed > 0) console.log(`  Elements only moved or resized by reflow: ${reflowed}`);
  if (byProp.size > 0) {
    const top = [...byProp].sort((a, b) => b[1] - a[1]).slice(0, 10);
    console.log(`  By property: ${top.map(([prop, n]) => `${prop} ×${n}`).join(", ")}`);
  }
  if (report.pixels.changed > 0) {
    const b = report.pixels.bounds!;
    console.log(`  Pixels changed: ${report.pixels.changed} within ${b.width}×${b.height} at (${b.x}, ${b.y}) — ${diffPng}`);
  }
}

/** The last few path segments: enough to find the element, short enough to read. */
function shortKey(key: string) {
  const parts = key.split(" > ");
  return parts.length > 4 ? `… > ${parts.slice(-4).join(" > ")}` : key;
}

async function main() {
  const [command, ...rest] = process.argv.slice(2);
  const positional = rest.filter((a) => !a.startsWith("--"));
  if (command === "states") {
    for (const s of STATES) console.log(`${s.name.padEnd(16)} ${s.colorScheme}, ${s.width}×${s.height}${s.miniplayer ? ", miniplayer" : ""}`);
  } else if (command === "capture" && positional.length === 1) {
    await capture(positional[0], parseFlag(rest, "states")?.split(","));
  } else if (command === "compare" && positional.length === 2) {
    await compare(positional[0], positional[1], parseFlag(rest, "noise"));
  } else {
    console.error("Usage:\n  style-diff capture <name> [--states=a,b]\n  style-diff compare <before> <after> [--noise=<before2>]\n  style-diff states");
    process.exit(2);
  }
}

main().catch((err) => {
  console.error(err instanceof Error ? err.message : err);
  process.exit(1);
});
