#!/usr/bin/env bun
// Captures the docs and Store screenshots from the REAL app and your real library,
// in a throwaway profile (see throwaway-profile.ts) so your own settings, library
// database and theme are never touched. Scenes live in scripts/scenes/.
//
// The app is driven through window.__LUMINOUS_SCRIPT__, which only dev builds
// install. Run `bun run dev` in this checkout, build a debug exe once with
// `cd src-tauri && cargo build`, then run this script. The debug exe loads whatever
// Vite serves on :1420, so make sure that is this checkout's.
//
// Usage: bun scripts/take-scene-screenshots.ts --library <dir> [--library <dir>...]
//          [--exe <path>] [--name=<scene>[,<scene>...|prefix*]] [--locale=<tag>] [--scheme=light|dark]
//          [--stage=fresh|library] [--keep-profiles]
//        bun scripts/take-scene-screenshots.ts --clone-profile[=<luminous.db>] [...same filters]
//
// --clone-profile starts from a copy of your real database instead of an empty profile,
// so Home shows your real stats, pins and playlists and there is no library scan to wait
// for. Plays and setting changes land in the copy and are discarded; scrobbling and
// Discord presence are switched off in it. The first-run scenes need an empty library,
// so they are skipped. Close your own Luminous first.
import * as fs from "node:fs";
import * as path from "node:path";
import { fileURLToPath } from "node:url";
import { defaultDbPath } from "./mock-library";
import { AppProfile } from "./throwaway-profile";
import { createRemoteApi } from "./scenes/remote-api";
import { scenes } from "./scenes";
import { ALL_SCHEMES, runScenes, summarize } from "./scene-runner";
import type { ColorScheme } from "./scenes/types";
import { LOCALES, catalogChain } from "../src/lib/locales";

const REPO_ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const OUT_ROOT = path.join(REPO_ROOT, "docs/user-guide/assets");

function parseArgs(argv: string[]) {
  const value = (flag: string) => {
    const eq = argv.find((a) => a.startsWith(`--${flag}=`));
    if (eq) return eq.slice(flag.length + 3);
    const i = argv.indexOf(`--${flag}`);
    return i !== -1 ? argv[i + 1] : undefined;
  };
  const libraries = argv.flatMap((a, i) => (a === "--library" && argv[i + 1] ? [argv[i + 1]] : []));
  const scheme = value("scheme");
  if (scheme && !ALL_SCHEMES.includes(scheme as ColorScheme)) throw new Error(`--scheme must be light or dark, got "${scheme}".`);
  const locale = value("locale");
  const allLocales: string[] = LOCALES.map((l) => l.tag);
  if (locale && !allLocales.includes(locale)) throw new Error(`Unknown --locale "${locale}". Valid: ${allLocales.join(", ")}`);
  const stage = value("stage");
  if (stage && stage !== "fresh" && stage !== "library") throw new Error(`--stage must be fresh or library, got "${stage}".`);
  const cloneArg = argv.find((a) => a === "--clone-profile" || a.startsWith("--clone-profile="));
  const cloneFrom = cloneArg ? (cloneArg.split("=")[1] ?? defaultDbPath()) : undefined;
  if (cloneArg && !cloneFrom) throw new Error("Can't find your luminous.db; pass --clone-profile=<path to luminous.db>.");
  if (cloneArg && stage === "fresh") throw new Error("--stage=fresh needs an empty library, so it can't run with --clone-profile.");
  return {
    cloneFrom,
    libraries,
    exe: value("exe") ?? path.join(REPO_ROOT, "target", "debug", "LuminousMusicPlayer.exe"),
    name: value("name"),
    locales: locale ? [locale] : allLocales,
    schemes: scheme ? [scheme as ColorScheme] : undefined,
    stage: (cloneArg ? "library" : stage) as "fresh" | "library" | undefined,
    keepProfiles: argv.includes("--keep-profiles"),
  };
}

function translate(locale: string, keyPath: string): string {
  for (const catalog of catalogChain(locale)) {
    const found = keyPath.split(".").reduce<unknown>((obj, key) => (obj as Record<string, unknown> | undefined)?.[key], catalog);
    if (typeof found === "string") return found;
  }
  return keyPath;
}

async function main() {
  if (process.env.CI) {
    console.log("Running in CI environment. Skipping screenshot generation.");
    return;
  }
  const opts = parseArgs(process.argv.slice(2));
  const needsLibrary = scenes.some((s) => (s.stage ?? "library") === "library") && opts.stage !== "fresh";
  if (needsLibrary && !opts.cloneFrom && opts.libraries.length === 0) {
    throw new Error("Pass at least one --library <dir> holding the music the scenes feature (the albums named in scripts/scenes/).");
  }

  const version = JSON.parse(fs.readFileSync(path.join(REPO_ROOT, "package.json"), "utf8")).version as string;
  const profile = new AppProfile({
    exe: opts.exe,
    cloneFrom: opts.cloneFrom,
    // launched_version stops the first-launch celebration toast from landing in frames.
    appState: { launched_version: version, active_theme_id: "system" },
  });

  try {
    if (opts.cloneFrom) console.log(`Cloned ${opts.cloneFrom}: runs use a private copy; your profile is not written.`);
    await profile.launch();
    await using driver = await profile.connectDriver();
    const api = createRemoteApi(driver);

    // Fail before any capture if this isn't a dev build. The layout installs the API
    // from a dynamic import, so it can land just after load.
    await driver
      .waitForCondition(() => driver.evaluate(() => !!window.__LUMINOUS_SCRIPT__), { timeoutMs: 10000 })
      .catch(() => {
        throw new Error(
          `window.__LUMINOUS_SCRIPT__ is missing: ${opts.exe} is not a dev-mode build. ` +
            "Run `bun run dev`, build a debug exe (`cd src-tauri && cargo build`) and pass it with --exe."
        );
      });

    const report = await runScenes({
      driver,
      sceneDriver: driver,
      api,
      outRoot: OUT_ROOT,
      scenes,
      translate,
      filters: { locales: opts.locales, schemes: opts.schemes, name: opts.name, stage: opts.stage },
      async prepareLibrary() {
        // A clone already has its library scanned.
        if (!opts.cloneFrom) await profile.scanLibrary(opts.libraries);
        // Startup-only layout preferences, read once at load: match the current guide
        // (Album Info collapsed so the track list gets the room), then reload.
        await driver.evaluate(() => {
          localStorage.setItem("layout_isOverviewExpanded", "false");
        });
        await driver.reload();
        await driver.waitForCondition(() => driver.evaluate(() => !!window.__LUMINOUS_SCRIPT__), { timeoutMs: 10000 });
        // Silent while it plays: the scenes only need the picture.
        await driver.invoke("set_volume", { volume: 0 });
      },
    });

    console.log(summarize(report));
    if (report.failures.length) process.exitCode = 1;
  } finally {
    await profile.dispose(opts.keepProfiles);
  }
}

main().catch((err) => {
  console.error(err);
  process.exit(1);
});
