// scripts/take-screenshots.ts
import { spawn, execSync } from "child_process";
import * as fs from "fs";
import * as path from "path";
import { fileURLToPath } from "url";
import { compileMockScript } from "./compile-mock-script";
import { loadMockConfig, loadMockLibrary, resolveFeatured, resolveScreenshotSettings } from "./mock-library";
import type { FeaturedSelection } from "./mock-library";

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);

function parseNameFilter(argv: string[]): string | undefined {
  const eqArg = argv.find((a) => a.startsWith("--name="));
  if (eqArg) return eqArg.slice("--name=".length);
  const flagIndex = argv.indexOf("--name");
  if (flagIndex !== -1) return argv[flagIndex + 1];
  return undefined;
}

/**
 * Resolves a `luminous-art://...` (or its Windows rewrite,
 * `http://luminous-art.localhost/...`) request to a file under coversDir,
 * mirroring the custom protocol handler in src-tauri/src/lib.rs exactly —
 * there's no real Tauri backend in the browser to answer these requests, so
 * without this every cover art image 404s.
 */
function resolveCoverArtPath(requestUrl: string, coversDir: string): string {
  let trimmed = requestUrl;
  if (requestUrl.startsWith("http://luminous-art.localhost/")) {
    trimmed = requestUrl.slice("http://luminous-art.localhost/".length);
  } else if (requestUrl.startsWith("luminous-art://")) {
    trimmed = requestUrl.slice("luminous-art://".length);
  }
  if (trimmed.startsWith("localhost/")) {
    trimmed = trimmed.slice("localhost/".length);
  }
  trimmed = trimmed.replace(/\/+$/, "");

  if (trimmed.startsWith("local/")) {
    return decodeURIComponent(trimmed.slice("local/".length));
  }
  return path.join(coversDir, decodeURIComponent(trimmed));
}

async function registerCoverArtRoute(page: import("playwright").Page, coversDir: string | undefined) {
  if (!coversDir) return;
  const handler = async (route: import("playwright").Route) => {
    const filePath = resolveCoverArtPath(route.request().url(), coversDir);
    if (fs.existsSync(filePath) && fs.statSync(filePath).isFile()) {
      await route.fulfill({
        status: 200,
        contentType: filePath.toLowerCase().endsWith(".png") ? "image/png" : "image/jpeg",
        body: fs.readFileSync(filePath),
      });
    } else {
      await route.fulfill({ status: 404, body: Buffer.alloc(0) });
    }
  };
  await page.route("http://luminous-art.localhost/**", handler);
  await page.route("luminous-art://**", handler);
}

async function main() {
  if (process.env.CI) {
    console.log("[Screenshot Automation] Running in CI environment. Skipping screenshot generation.");
    process.exit(0);
  }

  const nameFilter = parseNameFilter(process.argv.slice(2));
  if (nameFilter) {
    console.log(`[Screenshot Automation] --name "${nameFilter}" given; only that screenshot will be captured.`);
  }
  console.log("[Screenshot Automation] Starting screenshot generation...");

  // 1. Try dynamically importing playwright
  let playwright;
  try {
    playwright = await import("playwright");
  } catch (err) {
    console.warn("\n[WARNING] Playwright is not installed. Skipping screenshot generation.");
    console.warn("To install and run screenshots locally, run:\n");
    console.warn("  bun add -D playwright && bunx playwright install chromium\n");
    process.exit(0);
  }

  // 2. Start Vite server in background
  console.log("[Screenshot Automation] Starting Vite dev server on port 1420...");
  const devServer = spawn("bun", ["run", "dev"], {
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
    console.log("[Screenshot Automation] Cleaning up Vite server process...");
    killDevServer();
  };

  process.on("exit", cleanup);
  process.on("SIGINT", () => { process.exit(0); });
  process.on("SIGTERM", () => { process.exit(0); });

  // 3. Poll server until active
  console.log("[Screenshot Automation] Waiting for Vite server on http://localhost:1420...");
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
    console.error("[ERROR] Vite server failed to respond on port 1420.");
    killDevServer();
    process.exit(1);
  }

  console.log("[Screenshot Automation] Vite server is ready. Launching headless browser...");

  // 4. Run Playwright automation
  const { chromium } = playwright;
  const browser = await chromium.launch({ headless: true });

  const mockConfig = loadMockConfig();
  const mockLibrary = await loadMockLibrary(mockConfig);
  const defaultFeatured = resolveFeatured(mockLibrary, {
    featuredSong: mockConfig.default?.featuredSong,
    featuredArtist: mockConfig.default?.featuredArtist,
  });
  console.log(
    `[Screenshot Automation] Mock library: ${mockLibrary.source} (${mockLibrary.songs.length} songs, ${mockLibrary.artists.length} artists). Featured artist: ${defaultFeatured.artist ?? "none"}.`
  );
  // Library data (songs/albums/artists) is the same for every screenshot; only
  // the "featured" selection and UI settings vary per-screenshot.
  const libraryJson = JSON.stringify(mockLibrary);
  const mockCode = compileMockScript();
  const coversDir = mockLibrary.dbPath ? path.join(path.dirname(mockLibrary.dbPath), "covers") : undefined;

  interface CaptureOptions {
    tab: string;
    subTab: string;
    theme: string;
    filename: string;
    featured: FeaturedSelection;
    language?: string;
    afterLoad?: (page: import("playwright").Page, featured: FeaturedSelection) => Promise<void>;
    isImmersive?: boolean;
    sidebarOpen?: boolean;
    rightPanelOpen?: boolean;
    sidebarWidth?: number;
    positionSeconds?: number;
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
  }: CaptureOptions) {
    console.log(`[Screenshot Automation] Capturing ${filename} (Tab: ${tab}, SubTab: ${subTab}, Theme: ${theme}, Language: ${language}, Immersive: ${isImmersive})...`);
    const page = await browser.newPage();
    await page.setViewportSize({ width: 1280, height: 800 });
    await registerCoverArtRoute(page, coversDir);
    page.on("console", (msg) => {
      if (msg.type() === "error" || msg.type() === "warning") {
        console.warn(`[Page ${msg.type()}] ${msg.text()}`);
      }
    });
    page.on("pageerror", (err) => console.error(`[Page error] ${err.stack || err.message}`));

    // Inject the mock library data, then the mock Tauri IPC bridge that reads it
    await page.addInitScript(`
      window.__LUMINOUS_MOCK_LIBRARY__ = ${libraryJson};
      window.__LUMINOUS_MOCK_FEATURED__ = ${JSON.stringify(featured)};
    `);
    await page.addInitScript(mockCode);

    // Pre-configure the mock settings on mount
    await page.addInitScript(`
      window.mockSettings = {
        active_theme_id: "${theme}",
        custom_themes: "[]",
        active_tab: "${tab}",
        active_sub_tab: "${subTab}",
        excluded_formats: "[]",
        language: "${language}"
      };
      window.mockPlaybackPositionSec = ${positionSeconds};
      window.localStorage.setItem("layout_immersiveMode", "${isImmersive ? 'true' : 'false'}");
      window.localStorage.setItem("layout_sidebarOpen", "${sidebarOpen ? 'true' : 'false'}");
      window.localStorage.setItem("layout_rightPanelOpen", "${rightPanelOpen ? 'true' : 'false'}");
      window.localStorage.setItem("layout_sidebarWidth", "${sidebarWidth}");
      if ("${subTab}" === "artists") {
        window.localStorage.setItem("sort_artist_field", "album_count");
        window.localStorage.setItem("sort_artist_asc", "false");
      } else if ("${subTab}" === "albums") {
        window.localStorage.setItem("sort_album_field", "year");
        window.localStorage.setItem("sort_album_asc", "false");
      }
    `);

    await page.goto("http://localhost:1420");

    // Wait for Svelte app container to mount
    await page.waitForSelector(".flex-1");

    // Wait for rendering & animations to settle (e.g. waveform seek bar, dynamic styles, visualizer FFT frames)
    await page.waitForTimeout(1500);

    // Optional post-load interaction (e.g. clicking into a sub-tab)
    if (afterLoad) {
      await afterLoad(page, featured);
    }
    // Let any rendering and async effects fire
    await page.waitForTimeout(600);
    // Wait for all <img> tags to complete loading
    await page.evaluate(async () => {
      const imgs = Array.from(document.querySelectorAll("img"));
      await Promise.all(
        imgs.map((img) => {
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

    const dir = path.join(__dirname, "../docs/screenshots");
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
        console.warn(`[Screenshot Automation] Screenshot capture for ${filename} failed (attempt ${attempts}), retrying in 300ms...`, err);
        await page.waitForTimeout(300);
      }
    }
    console.log(`[Screenshot Automation] Saved screenshot to ${screenshotPath}`);
    await page.close();
  }

  const actionRegistry: Record<string, (page: import("playwright").Page, featured: FeaturedSelection) => Promise<void>> = {
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
      }, featured.song?.album);
    },
    "click-themes": async (page) => {
      await page.evaluate(() => {
        const btns = Array.from(document.querySelectorAll("button"));
        const t = btns.find((b: Element) => (b as HTMLElement).textContent?.trim() === "UI Themes");
        if (t) (t as HTMLElement).click();
      });
    }
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
        console.warn(`[Screenshot Automation] No screenshot named "${nameFilter}" found in mock-config.json. Available: ${mockConfig.screenshots.map((s) => s.name).join(", ")}`);
      }
      for (const s of screenshotsToRun) {
        const settings = resolveScreenshotSettings(mockConfig, s);
        const featured = resolveFeatured(mockLibrary, settings);
        const afterLoad = s.action ? actionRegistry[s.action] : undefined;

        await capture({
          tab: s.tab,
          subTab: s.subTab,
          theme: cleanThemeId(settings.theme),
          filename: s.filename,
          featured,
          language: settings.language,
          afterLoad,
          isImmersive: s.isImmersive ?? false,
          sidebarOpen: settings.sidebarOpen,
          rightPanelOpen: settings.rightPanelOpen,
          sidebarWidth: settings.sidebarWidth,
          positionSeconds: settings.positionSeconds,
        });
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
        { name: "now-playing", opts: { tab: "collection", subTab: "songs", theme: "nordic-blue", filename: "now-playing.png", featured, isImmersive: true, sidebarOpen: false, rightPanelOpen: false, sidebarWidth: 64, positionSeconds: 82 } },
      ];
      const toRun = nameFilter ? fallbackCaptures.filter((c) => c.name === nameFilter) : fallbackCaptures;
      if (nameFilter && toRun.length === 0) {
        console.warn(`[Screenshot Automation] No screenshot named "${nameFilter}". Available: ${fallbackCaptures.map((c) => c.name).join(", ")}`);
      }
      for (const c of toRun) {
        await capture(c.opts);
      }
    }
  } catch (err) {
    console.error("[Screenshot Automation] Error capturing screenshots:", err);
  } finally {
    await browser.close();
    killDevServer();
    console.log("[Screenshot Automation] All screenshots captured successfully.");
    process.exit(0);
  }
}

main().catch((err) => {
  console.error("[Screenshot Automation] Fatal error in script runner:", err);
  process.exit(1);
});
