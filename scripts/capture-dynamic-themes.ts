#!/usr/bin/env bun
// Captures the Dynamic Artwork theme slideshow from the REAL app, not the mocked
// IPC bridge: each song plays for real (real waveform, real cover-art colors from
// the track-changed path) and its album detail is screenshotted. It runs in a
// throwaway profile (see throwaway-profile.ts), so your real library, settings and
// theme are never touched, and nothing needs restoring afterwards.
//
// The app is driven through window.__LUMINOUS_SCRIPT__, which only dev builds
// install (import.meta.env.DEV). Release builds don't have it, so use a debug exe
// that loads the Vite dev server: run `bun run dev` in one terminal, build once
// with `cd src-tauri && cargo build`, then run this script.
//
// Entries are the `liveApp` rows in screenshot-scenes.json. A borderline cover (e.g. The
// Warning) is captured as the app paints it; the resolved palette is logged per
// shot so a light/dark flip between runs is visible.
//
// Usage: bun scripts/capture-dynamic-themes.ts --library <dir> [--library <dir>...]
//          [--exe <path>] [--name=theme-dynamic-<artist>] [--keep-profiles]
import * as fs from "fs";
import * as path from "path";
import { fileURLToPath } from "url";
import { AppProfile, DevtoolsDriver } from "./throwaway-profile";
import { DEFAULT_VIEWPORT, loadSceneConfig } from "./mock-library";

const REPO_ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const OUT_DIR = path.join(REPO_ROOT, "docs/user-guide/assets/en-CA/screenshots/dynamic");

const sleep = (ms: number) => new Promise((resolve) => setTimeout(resolve, ms));

function parseArgs() {
  const args = process.argv.slice(2);
  const get = (flag: string) => {
    const i = args.indexOf(flag);
    return i !== -1 ? args[i + 1] : undefined;
  };
  const libraries = args.flatMap((a, i) => (a === "--library" && args[i + 1] ? [args[i + 1]] : []));
  if (libraries.length === 0) {
    throw new Error("Pass at least one --library <dir> holding the albums named in screenshot-scenes.json.");
  }
  return {
    libraries,
    exe: get("--exe") ?? path.join(REPO_ROOT, "target", "debug", "LuminousMusicPlayer.exe"),
    name: args.find((a) => a.startsWith("--name="))?.slice("--name=".length),
    keepProfiles: args.includes("--keep-profiles"),
  };
}

async function main() {
  const opts = parseArgs();
  const all = (loadSceneConfig().screenshots ?? []).filter((s) => s.liveApp);
  const entries = all.filter((s) => !opts.name || s.name === opts.name);
  if (entries.length === 0) {
    throw new Error(`No liveApp entry named "${opts.name}". Valid names: ${all.map((s) => s.name).join(", ")}`);
  }

  const profile = new AppProfile({
    exe: opts.exe,
    appState: {
      active_theme_id: "dynamic-artwork",
      active_tab: "collection",
      active_sub_tab: "albums",
    },
  });

  try {
    await profile.launch();
    await profile.scanLibrary(opts.libraries);

    await using driver = await profile.connectDriver();

    // Album Info starts collapsed so the track list gets the room. It's a
    // localStorage layout preference, read at startup, so it needs a reload.
    await driver.evaluate(() => localStorage.setItem("layout_isOverviewExpanded", "false"));
    await driver.reload();

    const hasScripting = await driver.evaluate(() => !!window.__LUMINOUS_SCRIPT__);
    if (!hasScripting) {
      throw new Error(
        `window.__LUMINOUS_SCRIPT__ is missing: ${opts.exe} is not a dev-mode build. ` +
          "Run `bun run dev`, build a debug exe (`cd src-tauri && cargo build`) and pass it with --exe."
      );
    }

    // Online services off keeps enrichment toasts and fetched panels out of the frame.
    await driver.invoke("set_online_enabled", { enabled: false });

    fs.mkdirSync(OUT_DIR, { recursive: true });

    try {
      for (const entry of entries) {
        console.log(`Capturing ${entry.name}...`);
        await driver.setWindowSize(
          entry.viewportWidth ?? DEFAULT_VIEWPORT.width,
          entry.viewportHeight ?? DEFAULT_VIEWPORT.height
        );

        // Play first: the theme re-extracts from the playing song's own cover on
        // track-changed, which is what a user sees after pressing Play.
        const song = await driver.evaluate(
          async (title, artist, position, albumName) => {
            const script = window.__LUMINOUS_SCRIPT__!;
            const played = await script.playback.play({ title, artist });
            await script.playback.seek(position);
            await script.navigate.album(albumName ?? played.album);
            return { title: played.title, album: played.album };
          },
          entry.featuredSong,
          entry.featuredArtist,
          entry.positionSeconds ?? 60,
          entry.featuredAlbum
        );

        await driver.evaluate(() => {
          for (const b of document.querySelectorAll<HTMLElement>("button[aria-label='Dismiss notification']")) b.click();
          return window.__LUMINOUS_SCRIPT__!.wait.settled();
        });
        await sleep(1500); // let the waveform and theme crossfade settle

        const palette = await driver.evaluate(() =>
          document.documentElement.style.getPropertyValue("--color-artwork-primary")
        );
        const file = path.join(OUT_DIR, entry.filename);
        await driver.screenshot({ path: file });
        console.log(`Saved ${path.relative(REPO_ROOT, file)} ("${song.title}", palette ${palette})`);
      }
    } finally {
      await driver.clearWindowSize();
    }
  } finally {
    await profile.dispose(opts.keepProfiles);
  }
}

main().catch((err) => {
  console.error(err);
  process.exit(1);
});
