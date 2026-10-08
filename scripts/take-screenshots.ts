// Docs-screenshot harness: boots its own Vite dev server, injects the mocked
// Tauri IPC bridge (see tauri-ipc-mock.ts), captures each view listed in
// mock-config.json via Playwright (once per color scheme, into
// docs/user-guide/assets/{locale}/screenshots/{light,dark}/), then kills the dev server. See
// .claude/CLAUDE.md for the mock-config.json setup trap in a fresh worktree.
// Usage: bun run take-screenshots [--name=<entry>] [--locale=<tag>]
import * as fs from "fs";
import * as path from "path";
import { fileURLToPath } from "url";
import { compileMockScript } from "./compile-mock-script";
import { DEV_SERVER_URL, startViteDevServer } from "./vite-dev-server";
import { DEFAULT_VIEWPORT, loadMockConfig, loadMockLibrary, resolveFeatured, resolveScreenshotSettings } from "./mock-library";
import type { FeaturedSelection } from "./mock-library";
import { BASE_LOCALE, LOCALES, catalogChain } from "../src/lib/locales";

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
function t(language: string, keyPath: string): string {
  for (const catalog of catalogChain(language)) {
    const value = keyPath.split(".").reduce<unknown>((obj, key) => (obj as Record<string, unknown> | undefined)?.[key], catalog);
    if (typeof value === "string") return value;
  }
  return keyPath;
}

// Every screenshot is captured in every shipped locale (regional variants
// included) so each can feed its own Store listing; an entry may narrow that
// with `"locales": [...]`, and `--locale=<tag>` narrows a whole run.
const ALL_LOCALES = LOCALES.map((l) => l.tag);
function localesFor(spec: string[] | undefined, localeFilter: string | undefined): string[] {
  const tags = spec ?? ALL_LOCALES;
  return localeFilter ? tags.filter((t) => t === localeFilter) : tags;
}

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);

function parseFlag(argv: string[], flag: string): string | undefined {
  const eqArg = argv.find((a) => a.startsWith(`--${flag}=`));
  if (eqArg) return eqArg.slice(`--${flag}=`.length);
  const flagIndex = argv.indexOf(`--${flag}`);
  if (flagIndex !== -1) return argv[flagIndex + 1];
  return undefined;
}

async function main() {
  if (process.env.CI) {
    console.log("Running in CI environment. Skipping screenshot generation.");
    process.exit(0);
  }

  const nameFilter = parseFlag(process.argv.slice(2), "name");
  const localeFilter = parseFlag(process.argv.slice(2), "locale");
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

  // 2. Start Vite server and wait until it is serving
  console.log("Starting Vite dev server on port 1420...");
  let killDevServer: () => void;
  try {
    killDevServer = await startViteDevServer();
  } catch (err) {
    logError(`[ERROR] ${(err as Error).message}`);
    process.exit(1);
  }

  console.log("Vite server is ready. Launching headless browser...");

  // 3. Run Playwright automation
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

  // "dynamic" is for dynamic-artwork captures: the theme comes from the album
  // art rather than the OS color scheme, so no scheme is emulated.
  const colorSchemes = ["light", "dark"] as const;
  type ColorScheme = (typeof colorSchemes)[number] | "dynamic";

  interface CaptureOptions {
    tab: string;
    subTab?: string;
    theme: string;
    filename: string;
    featured: FeaturedSelection;
    language?: string;
    /** Color scheme to render and the output subfolder to write into. */
    scheme?: ColorScheme;
    openAlbum?: boolean;
    online?: boolean;
    afterLoad?: (page: import("playwright").Page, featured: FeaturedSelection, language: string) => Promise<void>;
    isImmersive?: boolean;
    sidebarOpen?: boolean;
    rightPanelOpen?: boolean;
    sidebarWidth?: number;
    positionSeconds?: number;
    viewportWidth?: number;
    viewportHeight?: number;
    emptyLibrary?: boolean;
    selector?: string;
    walkthroughCompleted?: boolean;
    /** e.g. "[3/20]" — shown when running the full batch (no --name filter); omitted otherwise. */
    progressLabel?: string;
  }

  async function capture({
    tab,
    subTab = "",
    theme,
    filename,
    featured,
    language = BASE_LOCALE,
    scheme = "dark",
    openAlbum = false,
    online = true,
    afterLoad,
    isImmersive = false,
    sidebarOpen = true,
    rightPanelOpen = false,
    sidebarWidth = 64,
    positionSeconds = 122,
    viewportWidth = DEFAULT_VIEWPORT.width,
    viewportHeight = DEFAULT_VIEWPORT.height,
    emptyLibrary = false,
    selector,
    walkthroughCompleted = true,
    progressLabel,
  }: CaptureOptions) {
    console.log(`${progressLabel ? progressLabel + " " : ""}Capturing ${scheme}/${filename}...`);
    const page = await browser.newPage();
    await page.setViewportSize({ width: viewportWidth, height: viewportHeight });
    // The System theme resolves light/dark from the OS color-scheme media
    // query (Chromium defaults to light), so emulate the requested scheme.
    // A legacy `-light` theme suffix is ignored: both schemes are always captured.
    const themeId = theme.endsWith("-light") ? theme.slice(0, -"-light".length) : theme;
    if (scheme !== "dynamic") await page.emulateMedia({ colorScheme: scheme });
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
    // welcome/empty-state capture) instead of the real mock data. The backend
    // always bootstraps the built-in Queue playlist at startup regardless of
    // library state (see PlaylistManager::queue() in AGENTS.md's Architecture
    // Invariants), so it must still be present here — without it,
    // PlaylistsStore.init()'s requireQueue() throws "built-in Queue playlist
    // missing from backend" and the page never finishes loading.
    const emptyLibraryQueuePlaylist = {
      id: 1,
      name: "Queue",
      dynamic_enabled: false,
      created: 0,
      updated: 0,
      track_count: 0,
      is_queue: true,
    };
    const emptyLibraryJson = JSON.stringify({
      songs: [],
      albums: [],
      artists: [],
      artistProfiles: [],
      playlists: [emptyLibraryQueuePlaylist],
      playlistTracks: {},
      lyrics: "",
    });
    await page.addInitScript(`
      window.__LUMINOUS_MOCK_LIBRARY__ = ${emptyLibrary ? emptyLibraryJson : libraryJson};
      window.__LUMINOUS_MOCK_FEATURED__ = ${emptyLibrary ? "{}" : JSON.stringify(featured)};
      window.__LUMINOUS_MOCK_ONLINE__ = ${online};
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
        active_theme_id: "${themeId}",
        custom_themes: "[]",
        active_tab: "${tab}",
        active_sub_tab: "${subTab}",
        language: "${language}",
        // Marks \`language\` as a BCP 47 tag so it isn't treated as a legacy "en"/"fr".
        language_tags: "1",
        // Otherwise +layout.svelte auto-starts the first-launch Walkthrough
        // tour, whose popover would cover every capture (see #897).
        walkthrough_completed: "${walkthroughCompleted ? 'true' : 'false'}",
        // Otherwise the first-run Welcome screen covers every capture behind
        // its full-screen overlay before the Walkthrough tour even starts.
        welcome_seen: "true"
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
      if (${openAlbum}) {
        window.localStorage.setItem("navigation_selectedAlbumName", ${JSON.stringify(featured.album ?? featured.song?.album ?? "")});
      }
      if ("${subTab}" === "auto" || "${subTab}" === "custom") {
        window.localStorage.setItem("navigation_playlistsSubTab", "${subTab}");
      }
    `);

    await page.goto(DEV_SERVER_URL);

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
    const ensureImagesLoaded = async () => {
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
    };
    try {
      await ensureImagesLoaded();
    } catch (err: unknown) {
      if (String(err).includes("Execution context was destroyed")) {
        await page.waitForSelector(".flex-1", { timeout: 30000 });
        if (afterLoad) {
          await afterLoad(page, featured, language);
        }
        await ensureImagesLoaded();
      } else {
        throw err;
      }
    }
    // Settle transitions
    await page.waitForTimeout(400);

    // Matches the Microsoft Store listing layout: assets/{locale}/screenshots/{theme}.
    const dir = path.join(__dirname, "../docs/user-guide/assets", language, "screenshots", scheme);
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
        if (selector) {
          const locator = page.locator(selector).first();
          await locator.screenshot({ path: screenshotPath });
        } else {
          await page.screenshot({ path: screenshotPath });
        }
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
      // "Edit Album Details" lives behind the "More actions" overflow menu (#97) —
      // it's a role="menuitem" button with no title attribute of its own, so
      // it has to be opened first and found by name rather than getByTitle.
      await page.getByTitle(t(language, "playlists.moreActionsTooltip"), { exact: true }).click();
      await page.waitForTimeout(300);
      await page.getByRole("menuitem", { name: t(language, "albumDetail.editAlbumDetails"), exact: true }).click();
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
      await page.getByRole("tab", { name: t(language, "settings.tabEqualizer"), exact: true }).click();
      await page.waitForTimeout(400);
    },
    "click-equalizer-parametric": async (page, _featured, language) => {
      await page.getByRole("tab", { name: t(language, "settings.tabEqualizer"), exact: true }).click();
      await page.waitForTimeout(400);
      await page.getByRole("button", { name: t(language, "equalizer.modeParametric"), exact: true }).click();
      await page.waitForTimeout(400);
      // Open the preset actions menu so the guide shows where Import, Export
      // and the user-preset actions live — it's easy to miss when closed.
      await page.getByRole("button", { name: t(language, "equalizer.presetActions"), exact: true }).click();
      await page.getByRole("menu").waitFor();
    },
    "click-settings-system": async (page, _featured, language) => {
      await page.getByRole("tab", { name: t(language, "settings.tabSystem"), exact: true }).click();
      await page.waitForTimeout(400);
    },
    "click-settings-sources": async (page, _featured, language) => {
      await page.getByRole("tab", { name: t(language, "settings.tabSources"), exact: true }).click();
      await page.waitForTimeout(400);
    },
    "click-settings-folders": async (page, _featured, language) => {
      await page.getByRole("tab", { name: t(language, "settings.tabSources"), exact: true }).click();
      await page.waitForTimeout(400);
    },
    "click-settings-integrations": async (page, _featured, language) => {
      await page.getByRole("tab", { name: t(language, "settings.tabIntegrations"), exact: true }).click();
      await page.waitForTimeout(400);
    },
    "click-organize-custom-template": async (page, _featured, language) => {
      // Switch the Template Pattern section to "Custom" to reveal the
      // free-text field, then swap in a template that actually changes the
      // mock library's paths (the default preset already matches how the
      // mock data is laid out, so every row would show "Unchanged" otherwise).
      await page.getByRole("button", { name: t(language, "organizer.presetCustom"), exact: true }).click();
      await page.waitForTimeout(300);
      const templateInput = page.locator("#template-input");
      await templateInput.fill("%albumartist/{%album/}{Disc %disc/}{%track }%title");
      await page.waitForTimeout(600);
    },
    "click-settings-about": async (page, _featured, language) => {
      await page.getByRole("tab", { name: t(language, "settings.tabAbout"), exact: true }).click();
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
      // initial_key values matching the "key:d" query. The column menu has no
      // visible trigger button anymore (see #0f22bafb) — it opens via
      // right-click on the table header row.
      await page.locator('[role="row"][tabindex="-1"]').first().click({ button: "right" });
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
    },
    "right-click-genre-chip": async (page) => {
      // Right-clicks whichever sub-genre chip happens to exist first, rather
      // than a hardcoded name — works against both the bundled fixture (see
      // mock-data.ts's Evanescence "Symphonic Metal" chip) and a real
      // library's own curated hierarchy, whatever names that happens to
      // contain. Opens GenreContextMenu with the hierarchy-curation actions
      // (Rename/Promote/Delete). A library with no sub-genre chips assigned
      // yet has nothing to right-click — skip rather than time out the run.
      const chip = page.locator("[data-chip-key]").first();
      if ((await chip.count()) === 0) {
        logWarn("right-click-genre-chip: no sub-genre chip found in this library; skipping the right-click.");
        return;
      }
      await chip.click({ button: "right" });
      await page.waitForTimeout(400);
    },
    "click-restart-walkthrough": async (page, _featured, language) => {
      const restartBtn = page.getByRole("button", { name: t(language, "walkthrough.restartTour"), exact: true });
      await restartBtn.waitFor({ state: "visible", timeout: 10000 });
      await restartBtn.click();
      await page.waitForSelector('[role="dialog"][aria-labelledby="walkthrough-step-title"]', { timeout: 10000 });
      await page.waitForTimeout(600);
    },
  };

  const cleanThemeId = (theme: string) => {
    return theme.trim().toLowerCase().replace(/\s+/g, "-");
  };

  try {
    if (mockConfig.screenshots && mockConfig.screenshots.length > 0) {
      const headless = mockConfig.screenshots.filter((s) => !s.liveApp);
      const screenshotsToRun = nameFilter ? headless.filter((s) => s.name === nameFilter) : headless;
      if (nameFilter && screenshotsToRun.length === 0) {
        logWarn(`No screenshot named "${nameFilter}" found in mock-config.json. Available: ${mockConfig.screenshots.map((s) => s.name).join(", ")}`);
      }
      const schemesFor = (s: { schemes?: ColorScheme[] }) => s.schemes ?? colorSchemes;
      const totalCaptures = screenshotsToRun.reduce((n, s) => n + localesFor(s.locales, localeFilter).length * schemesFor(s).length, 0);
      let captureIndex = 0;
      for (const s of screenshotsToRun) {
        const settings = resolveScreenshotSettings(mockConfig, s);
        const featured = resolveFeatured(mockLibrary, settings);
        const afterLoad = s.action ? actionRegistry[s.action] : undefined;

        for (const language of localesFor(s.locales, localeFilter)) {
        for (const scheme of schemesFor(s)) {
          captureIndex++;
          await capture({
            tab: s.tab,
            subTab: s.subTab,
            theme: cleanThemeId(settings.theme),
            filename: s.filename,
            featured,
            language,
            scheme,
            openAlbum: s.openAlbum,
            online: s.online,
            afterLoad,
            isImmersive: s.isImmersive ?? false,
            sidebarOpen: settings.sidebarOpen,
            rightPanelOpen: settings.rightPanelOpen,
            sidebarWidth: settings.sidebarWidth,
            positionSeconds: settings.positionSeconds,
            viewportWidth: s.viewportWidth,
            viewportHeight: s.viewportHeight,
            emptyLibrary: s.emptyLibrary,
            selector: s.selector,
            walkthroughCompleted: s.walkthroughCompleted ?? true,
            progressLabel: nameFilter ? undefined : `[${captureIndex}/${totalCaptures}]`,
          });
         }
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
      const runs = toRun.flatMap((c) => colorSchemes.map((scheme) => ({ ...c.opts, scheme })));
      for (const [i, opts] of runs.entries()) {
        await capture({
          ...opts,
          progressLabel: nameFilter ? undefined : `[${i + 1}/${runs.length}]`,
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
