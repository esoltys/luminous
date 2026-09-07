// Windows-only e2e smoke test: launches the real Tauri app (real Rust
// backend, real IPC, real SQLite) through tauri-driver + msedgedriver and
// verifies the main window renders. See issue #779.
//
// Talks to tauri-driver directly over the W3C WebDriver HTTP protocol
// (rather than through a client library like webdriverio) — that library's
// session negotiation was unreliable against tauri-driver's WebView2 backend
// in testing (the session would come back attached to a webview stuck on
// about:blank), while plain HTTP requests against the same endpoints were
// consistently reliable.
import { spawn, type ChildProcess } from 'node:child_process';
import { createConnection } from 'node:net';
import { existsSync, mkdtempSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const REPO_ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
// Must be built via `tauri build --debug` (not plain `cargo build`, and not
// `tauri build` in release mode):
//   - Only the tauri CLI's build pipeline embeds `frontendDist` into the
//     binary. A raw `cargo build` still loads `devUrl` (localhost:1420) and
//     shows a blank/network-error page, since no dev server runs for this test.
//   - A release-profile build enables tauri-plugin-prevent-default's full
//     flag set (see lib.rs's build_prevent_default_plugin), which disables
//     WebView2 devtools/accelerator keys — blocking the CDP channel
//     msedgedriver needs to control the page, so it hangs at about:blank.
//     `--debug` keeps `cfg!(debug_assertions)` true, which carves out
//     DEV_TOOLS from that flag set.
const APP_EXE = path.join(REPO_ROOT, 'target', 'debug', 'LuminousMusicPlayer.exe');
const TAURI_DRIVER_PORT = 4444;
const DRIVER_BASE = `http://127.0.0.1:${TAURI_DRIVER_PORT}`;

function waitForPort(port: number, timeoutMs: number): Promise<void> {
  const deadline = Date.now() + timeoutMs;
  return new Promise((resolve, reject) => {
    const attempt = () => {
      const socket = createConnection({ port, host: '127.0.0.1' });
      socket.once('connect', () => {
        socket.destroy();
        resolve();
      });
      socket.once('error', () => {
        socket.destroy();
        if (Date.now() > deadline) {
          reject(new Error(`tauri-driver did not start listening on port ${port} within ${timeoutMs}ms`));
        } else {
          setTimeout(attempt, 200);
        }
      });
    };
    attempt();
  });
}

async function webdriverRequest(method: string, urlPath: string, body?: unknown): Promise<any> {
  const res = await fetch(`${DRIVER_BASE}${urlPath}`, {
    method,
    headers: { 'Content-Type': 'application/json' },
    body: body === undefined ? undefined : JSON.stringify(body),
  });
  const json = await res.json();
  if (!res.ok) {
    throw new Error(`WebDriver ${method} ${urlPath} failed (HTTP ${res.status}): ${JSON.stringify(json)}`);
  }
  return json.value;
}

async function runSmokeTest(): Promise<void> {
  if (!existsSync(APP_EXE)) {
    throw new Error(`App binary not found at ${APP_EXE}.\nBuild it first: bunx tauri build --debug --no-bundle`);
  }

  // Isolated per-run data dir so this never reads or writes a real user's
  // library/database (see src-tauri/src/paths.rs).
  const dataDir = mkdtempSync(path.join(tmpdir(), 'luminous-e2e-'));

  console.log(`Starting tauri-driver (app data dir: ${dataDir})...`);
  const driver: ChildProcess = spawn('tauri-driver', ['--port', String(TAURI_DRIVER_PORT)], {
    stdio: ['ignore', 'pipe', 'pipe'],
    env: { ...process.env, LUMINOUS_DATA_DIR: dataDir },
  });

  let driverOutput = '';
  driver.stdout?.on('data', (d) => (driverOutput += d.toString()));
  driver.stderr?.on('data', (d) => (driverOutput += d.toString()));

  const driverExitedEarly = new Promise<never>((_, reject) => {
    driver.once('error', (err: NodeJS.ErrnoException) => {
      if (err.code === 'ENOENT') {
        reject(new Error('tauri-driver not found on PATH. Install it with: cargo install tauri-driver --locked'));
      } else {
        reject(err);
      }
    });
    driver.once('exit', (code) => {
      if (code !== 0) {
        reject(new Error(`tauri-driver exited early (code ${code}):\n${driverOutput}`));
      }
    });
  });

  let sessionId: string | undefined;
  try {
    await Promise.race([waitForPort(TAURI_DRIVER_PORT, 10_000), driverExitedEarly]);

    console.log('Creating WebDriver session...');
    const session = await webdriverRequest('POST', '/session', {
      capabilities: {
        alwaysMatch: {
          browserName: 'wry',
          'tauri:options': { application: APP_EXE },
        },
      },
    });
    sessionId = session.sessionId;

    console.log('Verifying app window loaded...');
    const deadline = Date.now() + 15_000;
    let navFound = false;
    let lastSource = '';
    while (Date.now() < deadline) {
      lastSource = await webdriverRequest('GET', `/session/${sessionId}/source`);
      if (/<nav[\s>]/.test(lastSource)) {
        navFound = true;
        break;
      }
      await new Promise((r) => setTimeout(r, 500));
    }

    if (!navFound) {
      const title = await webdriverRequest('GET', `/session/${sessionId}/title`).catch(() => '<unavailable>');
      throw new Error(
        `Sidebar <nav> element did not render within 15s — app did not load correctly.\n\n` +
          `Document title: ${title}\n\nPage source (first 2000 chars):\n${lastSource.slice(0, 2000)}`,
      );
    }

    console.log('✅ Smoke test passed: app launched and sidebar rendered.');
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    throw new Error(`${message}${driverOutput ? `\n\ntauri-driver output:\n${driverOutput}` : ''}`);
  } finally {
    if (sessionId) {
      await webdriverRequest('DELETE', `/session/${sessionId}`).catch(() => {});
    }
    driver.kill();
    rmSync(dataDir, { recursive: true, force: true });
  }
}

async function main() {
  if (process.platform !== 'win32') {
    console.log('Skipping: this e2e smoke test is Windows-only (see issue #779).');
    return;
  }

  try {
    await runSmokeTest();
  } catch (err) {
    console.error(`\n❌ ${err instanceof Error ? err.message : String(err)}\n`);
    process.exitCode = 1;
  }
}

main().finally(() => process.exit(process.exitCode ?? 0));
