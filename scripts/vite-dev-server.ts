// Boots `bun run dev` (Vite on port 1420) for the Playwright-driven mock
// harnesses (take-screenshots.ts, style-diff.ts) and tears it down again.
import { spawn, execSync } from "child_process";

export const DEV_SERVER_URL = "http://localhost:1420";

/**
 * Starts the dev server and resolves once it is serving and its initial
 * dependency optimisation has finished. The returned function kills it; it
 * is also registered on process exit, so a crash doesn't orphan port 1420.
 */
export async function startViteDevServer(): Promise<() => void> {
  // A single command string (rather than a separate args array) avoids
  // Node's DEP0190 warning — passing an args array alongside shell: true is
  // deprecated because the args get concatenated into the shell command
  // unescaped. Not a real risk here (no untrusted input), but this form is
  // the sanctioned way to invoke a shell built-in like `bun run dev` while
  // still using shell: true (needed on Windows to resolve bun's .cmd shim).
  const devServer = spawn("bun run dev", { stdio: "pipe", shell: true });
  // Drain the pipes so a chatty server can't block on a full buffer.
  devServer.stdout.on("data", () => {});
  devServer.stderr.on("data", () => {});

  // It's spawned with shell: true, so on Windows devServer.kill() only kills
  // the cmd.exe wrapper and leaves the actual bun/vite process (and port
  // 1420) orphaned; taskkill /t walks the whole process tree instead.
  let killed = false;
  const kill = () => {
    if (killed || !devServer.pid) return;
    killed = true;
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
  process.on("exit", kill);
  process.on("SIGINT", () => process.exit(0));
  process.on("SIGTERM", () => process.exit(0));

  for (let i = 0; i < 100; i++) {
    try {
      const res = await fetch(DEV_SERVER_URL);
      if (res.ok) {
        // Probe the root layout component so Vite completes its initial optimizeDeps
        // pass before launching the browser. During cold startup, Vite optimizes
        // dependencies and returns 504 on in-flight requests until the bundle is written.
        const layoutRes = await fetch(`${DEV_SERVER_URL}/src/routes/+layout.svelte`);
        if (layoutRes.ok) return kill;
      }
    } catch {
      // Not listening yet.
    }
    await new Promise((resolve) => setTimeout(resolve, 250));
  }
  kill();
  throw new Error(`Vite dev server failed to respond on ${DEV_SERVER_URL}.`);
}
