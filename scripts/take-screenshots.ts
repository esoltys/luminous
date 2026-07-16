// scripts/take-screenshots.ts
import { spawn } from "child_process";
import * as fs from "fs";
import * as path from "path";
import { fileURLToPath } from "url";
import { compileMockScript } from "./compile-mock-script";
import { loadMockConfig, loadMockLibrary, resolveFeatured } from "./mock-library";

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);

async function main() {
  if (process.env.CI) {
    console.log("[Screenshot Automation] Running in CI environment. Skipping screenshot generation.");
    process.exit(0);
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

  // Ensure devServer is terminated when process exits
  const cleanup = () => {
    console.log("[Screenshot Automation] Cleaning up Vite server process...");
    devServer.kill("SIGTERM");
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
    devServer.kill("SIGTERM");
    process.exit(1);
  }

  console.log("[Screenshot Automation] Vite server is ready. Launching headless browser...");

  // 4. Run Playwright automation
  const { chromium } = playwright;
  const browser = await chromium.launch({ headless: true });

  const mockConfig = loadMockConfig();
  const mockLibrary = await loadMockLibrary(mockConfig);
  const featured = resolveFeatured(mockLibrary, mockConfig);
  console.log(
    `[Screenshot Automation] Mock library: ${mockLibrary.source} (${mockLibrary.songs.length} songs, ${mockLibrary.artists.length} artists). Featured artist: ${featured.artist ?? "none"}.`
  );
  const libraryInitScript = `
    window.__LUMINOUS_MOCK_LIBRARY__ = ${JSON.stringify(mockLibrary)};
    window.__LUMINOUS_MOCK_FEATURED__ = ${JSON.stringify(featured)};
    window.__LUMINOUS_MOCK_CONFIG__ = ${JSON.stringify(mockConfig)};
  `;
  const mockCode = compileMockScript();

  async function capture(
    tab: string,
    subTab: string,
    theme: string,
    filename: string,
    afterLoad?: (page: import("playwright").Page) => Promise<void>,
    isImmersive = false,
    sidebarOpen = true,
    rightPanelOpen = false,
    sidebarWidth = 64,
    positionSeconds = 122
  ) {
    console.log(`[Screenshot Automation] Capturing ${filename} (Tab: ${tab}, SubTab: ${subTab}, Theme: ${theme}, Immersive: ${isImmersive})...`);
    const page = await browser.newPage();
    await page.setViewportSize({ width: 1280, height: 800 });

    // Inject the mock library data, then the mock Tauri IPC bridge that reads it
    await page.addInitScript(libraryInitScript);
    await page.addInitScript(mockCode);

    const customThemes = [
      {
        id: "custom-rodrigo",
        name: "Rodrigo",
        colors: {
          "bg-main": "#1c120c",
          "bg-sidebar": "#150e09",
          "bg-playerbar": "#160d09",
          "color-accent": "#c97f4c",
          "color-accent-hover": "#dca27a",
          "color-text-primary": "#ffffff",
          "color-text-secondary": "#baa7a1",
          "color-border": "#3d261a"
        },
        isCustom: true
      },
      {
        id: "custom-tom-petty",
        name: "Tom Petty",
        colors: {
          "bg-main": "#1a0f0c",
          "bg-sidebar": "#120a08",
          "bg-playerbar": "#140c0a",
          "color-accent": "#b83e20",
          "color-accent-hover": "#d4583b",
          "color-text-primary": "#ffffff",
          "color-text-secondary": "#b3a19c",
          "color-border": "#2d1814"
        },
        isCustom: true
      }
    ];

    // Pre-configure the mock settings on mount
    await page.addInitScript(`
      window.mockSettings = {
        active_theme_id: "${theme}",
        custom_themes: \`${JSON.stringify(customThemes)}\`,
        active_tab: "${tab}",
        active_sub_tab: "${subTab}",
        excluded_formats: "[]"
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
      await afterLoad(page);
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

  const actionRegistry: Record<string, (page: import("playwright").Page) => Promise<void>> = {
    "click-artist": async (page) => {
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
    "click-album": async (page) => {
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
      const defaultTheme = mockConfig.theme ? cleanThemeId(mockConfig.theme) : "nordic-blue";
      for (const s of mockConfig.screenshots) {
        const afterLoad = s.action ? actionRegistry[s.action] : undefined;
        const theme = s.theme || mockConfig.theme || "nordic-blue";
        const isImmersive = s.isImmersive ?? false;
        const sidebarOpen = s.sidebarOpen ?? mockConfig.sidebarOpen ?? true;
        const rightPanelOpen = s.rightPanelOpen ?? mockConfig.rightPanelOpen ?? false;
        const sidebarWidth = s.sidebarWidth ?? mockConfig.sidebarWidth ?? 64;
        const positionSeconds = s.positionSeconds ?? mockConfig.positionSeconds ?? 122;

        await capture(
          s.tab,
          s.subTab,
          cleanThemeId(theme),
          s.filename,
          afterLoad,
          isImmersive,
          sidebarOpen,
          rightPanelOpen,
          sidebarWidth,
          positionSeconds
        );
      }
    } else {
      // Predefined default captures fallback
      await capture("home", "", "custom-tom-petty", "home.png", undefined, false, true, false, 64, 68);
      await capture("collection", "albums", "custom-tom-petty", "albums.png", undefined, false, true, false, 64, 102);
      await capture("collection", "artists", "custom-tom-petty", "artists.png", undefined, false, true, false, 64, 38);
      await capture("collection", "artists", "custom-tom-petty", "artist-detail.png", actionRegistry["click-artist"], false, true, false, 64, 38);
      await capture("collection", "albums", "custom-tom-petty", "album-detail.png", actionRegistry["click-album"], false, true, false, 64, 38);
      await capture("settings", "", "custom-tom-petty", "themes.png", actionRegistry["click-themes"], false, true, false, 64, 156);
      await capture("collection", "songs", "custom-tom-petty", "now-playing.png", undefined, true, false, false, 64, 82);
    }
  } catch (err) {
    console.error("[Screenshot Automation] Error capturing screenshots:", err);
  } finally {
    await browser.close();
    devServer.kill("SIGTERM");
    console.log("[Screenshot Automation] All screenshots captured successfully.");
    process.exit(0);
  }
}

main().catch((err) => {
  console.error("[Screenshot Automation] Fatal error in script runner:", err);
  process.exit(1);
});
