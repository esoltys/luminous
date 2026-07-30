// Animation preview harness: opens the dev-only micro-animation gallery
// (src/routes/dev/animations, see issue #182) in the system default browser.
// Unlike take-screenshots.ts this needs no Tauri IPC mocking — the gallery
// only touches local component state and the toast store — so it's just a
// Vite dev server plus a browser tab, left running so you can click Replay
// live. If a dev server is already running on port 1420 (e.g. `bun run dev`
// or `bun run tauri dev`), reuses it instead of spawning a second one.
// Usage: bun run preview-animations
import { spawn, execSync } from "child_process";

const PORT = 1420;
const URL = `http://localhost:${PORT}/dev/animations`;

function openBrowser(url: string) {
  const platform = process.platform;
  if (platform === "win32") {
    execSync(`start "" "${url}"`, { shell: "cmd.exe" });
  } else if (platform === "darwin") {
    execSync(`open "${url}"`);
  } else {
    execSync(`xdg-open "${url}"`);
  }
}

async function isServerUp(): Promise<boolean> {
  try {
    const res = await fetch(`http://localhost:${PORT}`);
    return res.ok;
  } catch {
    return false;
  }
}

async function main() {
  let devServer: ReturnType<typeof spawn> | undefined;

  const killDevServer = () => {
    if (!devServer?.pid) return;
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
  process.on("exit", killDevServer);
  process.on("SIGINT", () => process.exit(0));
  process.on("SIGTERM", () => process.exit(0));

  if (await isServerUp()) {
    console.log(`[Animation Preview] Reusing dev server already running on port ${PORT}.`);
  } else {
    console.log(`[Animation Preview] Starting Vite dev server on port ${PORT}...`);
    devServer = spawn("bun", ["run", "dev"], { stdio: "inherit", shell: true });

    console.log(`[Animation Preview] Waiting for Vite server on http://localhost:${PORT}...`);
    let ready = false;
    for (let i = 0; i < 50; i++) {
      if (await isServerUp()) {
        ready = true;
        break;
      }
      await new Promise((resolve) => setTimeout(resolve, 200));
    }
    if (!ready) {
      console.error("[ERROR] Vite server failed to respond.");
      killDevServer();
      process.exit(1);
    }
  }

  console.log(`[Animation Preview] Opening ${URL} ...`);
  try {
    openBrowser(URL);
  } catch (err) {
    console.warn(`[Animation Preview] Could not auto-open a browser — visit ${URL} manually.`, err);
  }

  if (devServer) {
    console.log("[Animation Preview] Dev server running — press Ctrl+C to stop.");
  } else {
    console.log("[Animation Preview] Done — the gallery tab is open against your existing dev server.");
    process.exit(0);
  }
}

main().catch((err) => {
  console.error("[Animation Preview] Fatal error:", err);
  process.exit(1);
});
