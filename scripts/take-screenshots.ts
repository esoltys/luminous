// scripts/take-screenshots.ts
import { spawn } from "child_process";
import * as fs from "fs";
import * as path from "path";
import { fileURLToPath } from "url";

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

  const mockCode = fs.readFileSync(path.join(__dirname, "tauri-ipc-mock.js"), "utf8");

  async function capture(
    tab: string,
    subTab: string,
    theme: string,
    filename: string,
    afterLoad?: (page: import("playwright").Page) => Promise<void>,
    isImmersive = false
  ) {
    console.log(`[Screenshot Automation] Capturing ${filename} (Tab: ${tab}, SubTab: ${subTab}, Theme: ${theme}, Immersive: ${isImmersive})...`);
    const page = await browser.newPage();
    await page.setViewportSize({ width: 1280, height: 800 });

    // Inject the mock Tauri IPC bridge
    await page.addInitScript(mockCode);

    // Pre-configure the mock settings on mount
    await page.addInitScript(`
      window.mockSettings = {
        active_theme_id: "${theme}",
        custom_themes: "[]",
        active_tab: "${tab}",
        active_sub_tab: "${subTab}",
        excluded_formats: "[]"
      };
      window.localStorage.setItem("layout_immersiveMode", "${isImmersive ? 'true' : 'false'}");
    `);

    await page.goto("http://localhost:1420");

    // Wait for Svelte app container to mount
    await page.waitForSelector(".flex-1");

    // Wait for rendering & animations to settle (e.g. waveform seek bar, dynamic styles, visualizer FFT frames)
    await page.waitForTimeout(1500);

    // Optional post-load interaction (e.g. clicking into a sub-tab)
    if (afterLoad) {
      await afterLoad(page);
      await page.waitForTimeout(500);
    }

    const dir = path.join(__dirname, "../docs/screenshots");
    if (!fs.existsSync(dir)) {
      fs.mkdirSync(dir, { recursive: true });
    }

    const screenshotPath = path.join(dir, filename);
    await page.screenshot({ path: screenshotPath });
    console.log(`[Screenshot Automation] Saved screenshot to ${screenshotPath}`);
    await page.close();
  }

  try {
    // Take screenshots of key views in their chosen themes
    await capture("home", "", "nordic-blue", "home.png");
    await capture("collection", "albums", "nordic-blue", "albums.png");
    await capture("collection", "artists", "nordic-blue", "artists.png");
    await capture("settings", "", "nordic-blue", "themes.png", async (page) => {
      // Click the "UI Themes" sub-tab inside the Settings view
      await page.evaluate(() => {
        const btns = Array.from(document.querySelectorAll("button"));
        const t = btns.find((b: Element) => (b as HTMLElement).textContent?.trim() === "UI Themes");
        if (t) (t as HTMLElement).click();
      });
    });
    await capture("collection", "songs", "nordic-blue", "now-playing.png", undefined, true);
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
