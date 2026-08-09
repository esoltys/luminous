// Docs-screenshot harness: boots its own Vite dev server, injects the mocked
// Tauri IPC bridge (see tauri-ipc-mock.ts), captures each view listed in
// mock-config.json via Playwright, then kills the dev server. See
// .claude/CLAUDE.md for the mock-config.json setup trap in a fresh worktree.
// Usage: bun run take-screenshots [--name=<entry>]
import { spawn, execSync } from "child_process";
import * as fs from "fs";
import * as path from "path";
import { fileURLToPath } from "url";
import { compileMockScript } from "./compile-mock-script";
import { loadMockConfig, loadMockLibrary, resolveFeatured, resolveScreenshotSettings } from "./mock-library";
import type { FeaturedSelection } from "./mock-library";
import { en } from "../src/lib/locales/en";
import { fr } from "../src/lib/locales/fr";

// Minimal ANSI coloring (no chalk dependency) so warnings/errors stand out
// against the routine progress logs when scanning a long run's output.
const color = {
  yellow: (s: string) => `\x1b[33m${s}\x1b[0m`,
  red: (s: string) => `\x1b[31m${s}\x1b[0m`,
  green: (s: string) => `\x1b[32m${s}\x1b[0m`,
};
function logWarn(...args: unknown[]) {
  const [first, ...rest] = args;
  console.warn(color.yellow(String(first)), ...rest);
}
function logError(...args: unknown[]) {
  const [first, ...rest] = args;
  console.error(color.red(String(first)), ...rest);
}

// The app renders all button/tooltip text in the active locale, so any UI
// text used to find elements to click must be looked up per-language rather
// than hardcoded in English.
const locales: Record<string, Record<string, unknown>> = { en, fr };
function t(language: string, keyPath: string): string {
  const dict = locales[language] ?? en;
  const value = keyPath.split(".").reduce<unknown>((obj, key) => (obj as Record<string, unknown> | undefined)?.[key], dict);
  return typeof value === "string" ? value : keyPath;
}

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);

function parseNameFilter(argv: string[]): string | undefined {
  const eqArg = argv.find((a) => a.startsWith("--name="));
  if (eqArg) return eqArg.slice("--name=".length);
  const flagIndex = argv.indexOf("--name");
  if (flagIndex !== -1) return argv[flagIndex + 1];
  return undefined;
}

async function main() {
  if (process.env.CI) {
    console.log("Running in CI environment. Skipping screenshot generation.");
    process.exit(0);
  }

  const nameFilter = parseNameFilter(process.argv.slice(2));
  if (nameFilter) {
    console.log(`--name "${nameFilter}" given; only that screenshot will be captured.`);
  }
  console.log("Starting screenshot generation...");

  // 1. Try dynamically importing playwright
  let playwright;
  try {
    playwright = await import("playwright");
  } catch (err) {
    logWarn("\n[WARNING] Playwright is not installed. Skipping screenshot generation.");
    logWarn("To install and run screenshots locally, run:\n");
    logWarn("  bun add -D playwright && bunx playwright install chromium\n");
    process.exit(0);
  }

  // 2. Start Vite server in background
  console.log("Starting Vite dev server on port 1420...");
  // A single command string (rather than a separate args array) avoids
  // Node's DEP0190 warning — passing an args array alongside shell: true is
  // deprecated because the args get concatenated into the shell command
  // unescaped. Not a real risk here (no untrusted input), but this form is
  // the sanctioned way to invoke a shell built-in like `bun run dev` while
  // still using shell: true (needed on Windows to resolve bun's .cmd shim).
  const devServer = spawn("bun run dev", {
    stdio: "pipe",
    shell: true,
  });

  // Keep track of server output for debugging if needed
  devServer.stdout.on("data", (data) => {
    // console.log(`[Vite stdout] ${data}`);
  });
  devServer.stderr.on("data", (data) => {
    // console.error(`[Vite stderr] ${data}`);
  });

  // Ensure devServer is terminated when process exits. It's spawned with
  // shell: true, so on Windows devServer.kill() only kills the cmd.exe
  // wrapper and leaves the actual bun/vite process (and port 1420) orphaned;
  // taskkill /t walks the whole process tree instead.
  const killDevServer = () => {
    if (!devServer.pid) return;
    if (process.platform === "win32") {
      try {
        execSync(`taskkill /pid ${devServer.pid} /t /f`, { stdio: "ignore" });
      } catch {
        // Already exited.
      }
    } else {
      devServer.kill("SIGTERM");
    }
  };

  const cleanup = () => {
    console.log("Cleaning up Vite server process...");
    killDevServer();
  };

  process.on("exit", cleanup);
  process.on("SIGINT", () => { process.exit(0); });
  process.on("SIGTERM", () => { process.exit(0); });

  // 3. Poll server until active
  console.log("Waiting for Vite server on http://localhost:1420...");
  let ready = false;
  for (let i = 0; i < 50; i++) {
    try {
      const res = await fetch("http://localhost:1420");
      if (res.ok) {
        ready = true;
        break;
      }
    } catch (e) {}
    await new Promise((resolve) => setTimeout(resolve, 200));
  }

  if (!ready) {
    logError("[ERROR] Vite server failed to respond on port 1420.");
    killDevServer();
    process.exit(1);
  }

  console.log("Vite server is ready. Launching headless browser...");

  // 4. Run Playwright automation
  const { chromium } = playwright;
  const browser = await chromium.launch({ headless: true });

  const mockConfig = loadMockConfig();
  const mockLibrary = await loadMockLibrary(mockConfig);
  const defaultFeatured = resolveFeatured(mockLibrary, {
    featuredSong: mockConfig.default?.featuredSong,
    featuredArtist: mockConfig.default?.featuredArtist,
    featuredAlbum: mockConfig.default?.featuredAlbum,
  });
  console.log(
    `Mock library: ${mockLibrary.source} (${mockLibrary.songs.length} songs, ${mockLibrary.artists.length} artists). Featured artist: ${defaultFeatured.artist ?? "none"}. Featured album: ${defaultFeatured.album ?? "none"}.`
  );
  // Library data (songs/albums/artists) is the same for every screenshot; only
  // the "featured" selection and UI settings vary per-screenshot.
  const libraryJson = JSON.stringify(mockLibrary);
  const mockCode = compileMockScript();

  interface CaptureOptions {
    tab: string;
    subTab: string;
    theme: string;
    filename: string;
    featured: FeaturedSelection;
    language?: string;
    afterLoad?: (page: import("playwright").Page, featured: FeaturedSelection, language: string) => Promise<void>;
    isImmersive?: boolean;
    sidebarOpen?: boolean;
    rightPanelOpen?: boolean;
    sidebarWidth?: number;
    positionSeconds?: number;
    viewportWidth?: number;
    viewportHeight?: number;
    emptyLibrary?: boolean;
    /** e.g. "[3/20]" — shown when running the full batch (no --name filter); omitted otherwise. */
    progressLabel?: string;
  }

  async function capture({
    tab,
    subTab,
    theme,
    filename,
    featured,
    language = "en",
    afterLoad,
    isImmersive = false,
    sidebarOpen = true,
    rightPanelOpen = false,
    sidebarWidth = 64,
    positionSeconds = 122,
    viewportWidth = 1280,
    viewportHeight = 800,
    emptyLibrary = false,
    progressLabel,
  }: CaptureOptions) {
    console.log(`${progressLabel ? progressLabel + " " : ""}Capturing ${filename}...`);
    const page = await browser.newPage();
    await page.setViewportSize({ width: viewportWidth, height: viewportHeight });
    page.on("console", (msg) => {
      if (msg.type() === "error" || msg.type() === "warning") {
        const text = msg.text();
        if (text.includes("Cannot read properties of undefined (reading 'offsetHeight')")) {
          return;
        }
        logWarn(`[Page ${msg.type()}] ${text}`);
      }
    });
    page.on("pageerror", (err) => {
      const msg = err.stack || err.message || String(err);
      if (msg.includes("Cannot read properties of undefined (reading 'offsetHeight')")) {
        return;
      }
      logError(`[Page error] ${msg}`);
    });

    // Inject the mock library data, then the mock Tauri IPC bridge that reads it.
    // emptyLibrary swaps in a zeroed-out library (used for the no-folders-added
    // welcome/empty-state capture) instead of the real mock data.
    const emptyLibraryJson = JSON.stringify({ songs: [], albums: [], artists: [], playlists: [], playlistTracks: {}, lyrics: "" });
    await page.addInitScript(`
      window.__LUMINOUS_MOCK_LIBRARY__ = ${emptyLibrary ? emptyLibraryJson : libraryJson};
      window.__LUMINOUS_MOCK_FEATURED__ = ${emptyLibrary ? "{}" : JSON.stringify(featured)};
    `);
    await page.addInitScript(mockCode);

    // Pre-configure the mock settings on mount. Spread over whatever mockCode's
    // own default already set (e.g. launched_version, seeded so the first-launch
    // celebration toast never fires during capture) instead of replacing the
    // object outright — a prior version of this script clobbered that default
    // and reintroduced the celebration-toast hang.
    await page.addInitScript(`
      window.mockSettings = {
        ...(window.mockSettings || {}),
        active_theme_id: "${theme}",
        custom_themes: "[]",
        active_tab: "${tab}",
        active_sub_tab: "${subTab}",
        language: "${language}"
      };
      window.mockPlaybackPositionSec = ${positionSeconds};
      window.localStorage.setItem("layout_immersiveMode", "${isImmersive ? 'true' : 'false'}");
      window.localStorage.setItem("layout_sidebarOpen", "${sidebarOpen ? 'true' : 'false'}");
      window.localStorage.setItem("layout_rightPanelOpen", "${rightPanelOpen ? 'true' : 'false'}");
      window.localStorage.setItem("layout_sidebarWidth", "${sidebarWidth}");
      if ("${subTab}" === "artists") {
        window.localStorage.setItem("sort_artist_field", "song_count");
        window.localStorage.setItem("sort_artist_asc", "false");
      } else if ("${subTab}" === "albums") {
        window.localStorage.setItem("sort_album_field", "year");
        window.localStorage.setItem("sort_album_asc", "false");
      }
      if ("${subTab}" === "auto" || "${subTab}" === "custom") {
        window.localStorage.setItem("navigation_playlistsSubTab", "${subTab}");
      }
    `);

    await page.goto("http://localhost:1420");

    // Wait for Svelte app container to mount. Generous timeout: on a cold
    // dev-server start, navigating into a view can make Vite discover
    // previously-unbundled dependencies (icons, Tauri API shims, etc.) that
    // weren't reachable from the initial crawl, triggering a full client
    // reload mid-mount — that reoptimize-and-reload cycle can take well
    // over the default 30s on a first hit.
    await page.waitForSelector(".flex-1", { timeout: 60000 });

    // Wait for rendering & animations to settle (e.g. waveform seek bar, dynamic styles, visualizer FFT frames)
    await page.waitForTimeout(1500);

    // Optional post-load interaction (e.g. clicking into a sub-tab)
    if (afterLoad) {
      await afterLoad(page, featured, language);
    }
    // Let any rendering and async effects fire
    await page.waitForTimeout(600);
    // Wait for all <img> tags to complete loading. CoverArt.svelte sets
    // loading="lazy" on cover art — in a long grid (e.g. Albums), most covers
    // sit below the fold and Chromium never fetches them without a real
    // scroll, so they'd never fire load/error and this would hang forever.
    // Forcing eager loading makes every image actually fetch.
    await page.evaluate(async () => {
      const imgs = Array.from(document.querySelectorAll("img"));
      await Promise.all(
        imgs.map((img) => {
          if (img.loading === "lazy") img.loading = "eager";
          if (img.complete) return;
          return new Promise((resolve) => {
            img.addEventListener("load", resolve);
            img.addEventListener("error", resolve);
          });
        })
      );
    });
    // Settle transitions
    await page.waitForTimeout(400);

    const dir = path.join(__dirname, "../docs/user-guide/screenshots");
    if (!fs.existsSync(dir)) {
      fs.mkdirSync(dir, { recursive: true });
    }

    const screenshotPath = path.join(dir, filename);
    let attempts = 0;
    while (attempts < 3) {
      try {
        if (fs.existsSync(screenshotPath)) {
          fs.unlinkSync(screenshotPath);
        }
        await page.screenshot({ path: screenshotPath });
        break;
      } catch (err) {
        attempts++;
        if (attempts >= 3) throw err;
        logWarn(`Screenshot capture for ${filename} failed (attempt ${attempts}), retrying in 300ms...`, err);
        await page.waitForTimeout(300);
      }
    }
    const relativePath = path.relative(path.join(__dirname, ".."), screenshotPath);
    console.log(color.green(`Saved screenshot to ${relativePath}`));
    await page.close();
  }

  const actionRegistry: Record<string, (page: import("playwright").Page, featured: FeaturedSelection, language: string) => Promise<void>> = {
    "click-artist": async (page, featured) => {
      await page.evaluate((artistName) => {
        const cards = Array.from(document.querySelectorAll(".artist-card"));
        const targetCard = cards.find((c: Element) => {
          const nameSpan = c.querySelector("span");
          return nameSpan && nameSpan.textContent?.trim() === artistName;
        });
        if (targetCard) {
          (targetCard as HTMLElement).click();
        }
      }, featured.artist);
    },
    "click-album": async (page, featured) => {
      await page.evaluate((albumName) => {
        const cards = Array.from(document.querySelectorAll(".bg-brand-sidebar"));
        let targetCard = cards.find((c: Element) => {
          const titleBtn = c.querySelector("button.font-semibold");
          return titleBtn && titleBtn.textContent?.trim() === albumName;
        });
        if (!targetCard && cards.length > 0) {
          targetCard = cards[0];
        }
        if (targetCard) {
          const titleBtn = targetCard.querySelector("button.font-semibold");
          if (titleBtn) {
            (titleBtn as HTMLElement).click();
          }
        }
      }, featured.album ?? featured.song?.album);
    },
    "click-song-tag-editor": async (page, _featured, language) => {
      await page.getByTitle(t(language, "collection.editTagsTooltip")).first().click();
      await page.waitForTimeout(500);
    },
    "click-album-tag-editor": async (page, featured, language) => {
      await page.evaluate((albumName) => {
        const cards = Array.from(document.querySelectorAll(".bg-brand-sidebar"));
        let targetCard = cards.find((c: Element) => {
          const titleBtn = c.querySelector("button.font-semibold");
          return titleBtn && titleBtn.textContent?.trim() === albumName;
        });
        if (!targetCard && cards.length > 0) {
          targetCard = cards[0];
        }
        if (targetCard) {
          const titleBtn = targetCard.querySelector("button.font-semibold");
          if (titleBtn) {
            (titleBtn as HTMLElement).click();
          }
        }
      }, featured.album ?? featured.song?.album);
      await page.waitForTimeout(500);
      await page.getByTitle(t(language, "albumDetail.editInfoTooltip"), { exact: true }).click();
      await page.waitForTimeout(400);
    },
    "click-themes": async (page, _featured, language) => {
      await page.evaluate((label) => {
        const btns = Array.from(document.querySelectorAll("button"));
        const match = btns.find((b: Element) => (b as HTMLElement).textContent?.trim() === label);
        if (match) (match as HTMLElement).click();
      }, t(language, "settings.tabThemes"));
    },
    "click-equalizer": async (page, _featured, language) => {
      // Locator click auto-waits for the button to be actionable — more
      // reliable than a fixed-delay evaluate() when the settings sub-tabs
      // haven't finished rendering yet.
      await page.getByRole("button", { name: t(language, "settings.tabEqualizer"), exact: true }).click();
      await page.waitForTimeout(400);
    },
    "click-equalizer-parametric": async (page, _featured, language) => {
      await page.getByRole("button", { name: t(language, "settings.tabEqualizer"), exact: true }).click();
      await page.waitForTimeout(400);
      await page.getByRole("button", { name: t(language, "equalizer.modeParametric"), exact: true }).click();
      await page.waitForTimeout(400);
    },
    "click-settings-folders": async (page, _featured, language) => {
      await page.getByRole("button", { name: t(language, "settings.tabFolders"), exact: true }).click();
      await page.waitForTimeout(400);
    },
    "click-settings-tools": async (page, _featured, language) => {
      await page.getByRole("button", { name: t(language, "settings.tabTools"), exact: true }).click();
      await page.waitForTimeout(400);
      // Swap in a template that actually changes the mock library's paths
      // (the default template already matches how the mock data is laid
      // out, so every row would show "Unchanged" otherwise).
      const templateInput = page.locator("#template-input");
      await templateInput.fill("%albumartist/{%album/}{Disc %disc/}{%track }%title");
      await page.waitForTimeout(600);
    },
    "click-settings-about": async (page, _featured, language) => {
      await page.getByRole("button", { name: t(language, "settings.tabAbout"), exact: true }).click();
      await page.waitForTimeout(400);
    },
    "click-bands-toggle": async (page, _featured, language) => {
      await page.getByTitle(t(language, "playerBar.seekbarModeWaveform")).click();
      await page.waitForTimeout(400);
    },
    "click-playlist": async (page) => {
      // "2010s" is a mock playlist name (data), not app UI copy — same in every locale.
      await page.getByRole("button", { name: "2010s", exact: true }).click();
      await page.waitForTimeout(400);
    },
    "click-smart-playlist": async (page, _featured, language) => {
      await page.getByRole("button", { name: t(language, "playlists.newSmartPlaylistBtn"), exact: true }).click();
      await page.waitForTimeout(400);
    },
    "click-smart-playlist-edit": async (page, _featured, language) => {
      await page.getByRole("button", { name: t(language, "playlists.newSmartPlaylistBtn"), exact: true }).click();
      await page.waitForTimeout(400);
      const nameInput = page.locator("#smart-playlist-name-input");
      await nameInput.fill("1980s Rock Mix");
      await page.waitForTimeout(200);

      const valuePlaceholder = t(language, "smartPlaylistBuilder.valuePlaceholder");
      const valInputs = page.locator(`input[placeholder="${valuePlaceholder}"]`);
      await valInputs.nth(0).fill("Rock");

      const addRuleLabel = t(language, "smartPlaylistBuilder.addRule");
      await page.getByRole("button", { name: addRuleLabel, exact: true }).click();
      await page.waitForTimeout(200);
      const selects = page.locator("form select");
      await selects.nth(2).selectOption("year");
      await page.waitForTimeout(100);
      await selects.nth(3).selectOption(">=");
      await page.waitForTimeout(100);
      await valInputs.nth(1).fill("1980");

      await page.getByRole("button", { name: addRuleLabel, exact: true }).click();
      await page.waitForTimeout(200);
      await selects.nth(4).selectOption("year");
      await page.waitForTimeout(100);
      await selects.nth(5).selectOption("<=");
      await page.waitForTimeout(100);
      await valInputs.nth(2).fill("1989");
      await page.waitForTimeout(400);
    },
    "type-search": async (page, _featured, language) => {
      const searchPlaceholder = t(language, "topNav.searchPlaceholder");
      const searchInput = page.locator(`input[placeholder="${searchPlaceholder}"]`);
      await searchInput.focus();
      await searchInput.fill("evan");
      await page.waitForTimeout(400);
    },
    "type-search-key": async (page, _featured, language) => {
      // Reveal the Key column so the table behind the search dropdown shows
      // initial_key values matching the "key:d" query.
      await page.getByTitle(t(language, "collection.columnsBtn")).click();
      await page.waitForTimeout(200);
      await page.getByRole("checkbox", { name: t(language, "collection.columnInitialKey"), exact: true }).click();
      await page.waitForTimeout(150);

      const searchPlaceholder = t(language, "topNav.searchPlaceholder");
      const searchInput = page.locator(`input[placeholder="${searchPlaceholder}"]`);
      // A real click (not .focus()) also serves as the outside-click that
      // closes the column selector dropdown opened above.
      await searchInput.click();
      await searchInput.fill("key:d");
      await page.waitForTimeout(400);
    },
    "click-rows-view": async (page, _featured, language) => {
      await page.getByRole("button", { name: t(language, "collection.viewRows"), exact: true }).click();
      await page.waitForTimeout(400);
    },
    "toggle-miniplayer": async (page) => {
      await page.keyboard.press("Control+m");
      await page.waitForTimeout(400);
    },
    "toggle-miniplayer-hover": async (page, _featured, language) => {
      await page.keyboard.press("Control+m");
      await page.waitForTimeout(400);
      const miniplayerLabel = t(language, "miniplayer.title");
      const miniRegion = page.locator(`[role="group"][aria-label="${miniplayerLabel}"]`);
      await miniRegion.hover();
      await page.waitForTimeout(400);
    }
  };

  const withLanguageSuffix = (filename: string, language: string) => {
    const suffix = language.toUpperCase();
    const dotIndex = filename.lastIndexOf(".");
    return dotIndex === -1 ? `${filename}-${suffix}` : `${filename.slice(0, dotIndex)}-${suffix}${filename.slice(dotIndex)}`;
  };

  const cleanThemeId = (theme: string) => {
    return theme.trim().toLowerCase().replace(/\s+/g, "-");
  };

  try {
    if (mockConfig.screenshots && mockConfig.screenshots.length > 0) {
      const screenshotsToRun = nameFilter
        ? mockConfig.screenshots.filter((s) => s.name === nameFilter)
        : mockConfig.screenshots;
      if (nameFilter && screenshotsToRun.length === 0) {
        logWarn(`No screenshot named "${nameFilter}" found in mock-config.json. Available: ${mockConfig.screenshots.map((s) => s.name).join(", ")}`);
      }
      const totalCaptures = screenshotsToRun.length * Object.keys(locales).length;
      let captureIndex = 0;
      for (const s of screenshotsToRun) {
        const settings = resolveScreenshotSettings(mockConfig, s);
        const featured = resolveFeatured(mockLibrary, settings);
        const afterLoad = s.action ? actionRegistry[s.action] : undefined;

        // Every screenshot is captured once per supported locale (see the
        // `locales` map above), so adding a language only requires adding its
        // locale file there — no changes needed here or in mock-config.json.
        for (const language of Object.keys(locales)) {
          captureIndex++;
          await capture({
            tab: s.tab,
            subTab: s.subTab,
            theme: cleanThemeId(settings.theme),
            filename: withLanguageSuffix(s.filename, language),
            featured,
            language,
            afterLoad,
            isImmersive: s.isImmersive ?? false,
            sidebarOpen: settings.sidebarOpen,
            rightPanelOpen: settings.rightPanelOpen,
            sidebarWidth: settings.sidebarWidth,
            positionSeconds: settings.positionSeconds,
            viewportWidth: s.viewportWidth,
            viewportHeight: s.viewportHeight,
            emptyLibrary: s.emptyLibrary,
            progressLabel: nameFilter ? undefined : `[${captureIndex}/${totalCaptures}]`,
          });
        }
      }
    } else {
      // Predefined default captures fallback
      const featured = defaultFeatured;
      const fallbackCaptures: Array<{ name: string; opts: CaptureOptions }> = [
        { name: "home", opts: { tab: "home", subTab: "", theme: "nordic-blue", filename: "home.png", featured, sidebarWidth: 64, positionSeconds: 68 } },
        { name: "albums", opts: { tab: "collection", subTab: "albums", theme: "nordic-blue", filename: "albums.png", featured, sidebarWidth: 64, positionSeconds: 102 } },
        { name: "artists", opts: { tab: "collection", subTab: "artists", theme: "nordic-blue", filename: "artists.png", featured, sidebarWidth: 64, positionSeconds: 38 } },
        { name: "artist-detail", opts: { tab: "collection", subTab: "artists", theme: "nordic-blue", filename: "artist-detail.png", featured, afterLoad: actionRegistry["click-artist"], sidebarWidth: 64, positionSeconds: 38 } },
        { name: "album-detail", opts: { tab: "collection", subTab: "albums", theme: "nordic-blue", filename: "album-detail.png", featured, afterLoad: actionRegistry["click-album"], sidebarWidth: 64, positionSeconds: 38 } },
        { name: "themes", opts: { tab: "settings", subTab: "", theme: "nordic-blue", filename: "themes.png", featured, afterLoad: actionRegistry["click-themes"], sidebarWidth: 64, positionSeconds: 156 } },
        { name: "equalizer", opts: { tab: "settings", subTab: "", theme: "nordic-blue", filename: "equalizer.png", featured, afterLoad: actionRegistry["click-equalizer"], sidebarWidth: 64, positionSeconds: 92 } },
        { name: "equalizer-parametric", opts: { tab: "settings", subTab: "", theme: "nordic-blue", filename: "equalizer-parametric.png", featured, afterLoad: actionRegistry["click-equalizer-parametric"], sidebarWidth: 64, positionSeconds: 92 } },
        { name: "now-playing", opts: { tab: "collection", subTab: "songs", theme: "nordic-blue", filename: "now-playing.png", featured, isImmersive: true, sidebarOpen: false, rightPanelOpen: false, sidebarWidth: 64, positionSeconds: 82 } },
      ];
      const toRun = nameFilter ? fallbackCaptures.filter((c) => c.name === nameFilter) : fallbackCaptures;
      if (nameFilter && toRun.length === 0) {
        logWarn(`No screenshot named "${nameFilter}". Available: ${fallbackCaptures.map((c) => c.name).join(", ")}`);
      }
      for (const [i, c] of toRun.entries()) {
        await capture({
          ...c.opts,
          progressLabel: nameFilter ? undefined : `[${i + 1}/${toRun.length}]`,
        });
      }
    }
  } catch (err) {
    logError("Error capturing screenshots:", err);
  } finally {
    await browser.close();
    killDevServer();
    console.log("Done.");
    process.exit(0);
  }
}

main().catch((err) => {
  logError("Fatal error in script runner:", err);
  process.exit(1);
});
