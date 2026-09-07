// Windows-only e2e smoke test: launches the real Tauri app (real Rust
// backend, real IPC, real SQLite) through tauri-driver + msedgedriver and
// verifies the main window renders. See issue #779.
import { existsSync, mkdtempSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { getSource, getTitle, startSession, stopSession, type DriverSession } from './webdriver-client';

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

async function runSmokeTest(): Promise<void> {
  if (!existsSync(APP_EXE)) {
    throw new Error(`App binary not found at ${APP_EXE}.\nBuild it first: bunx tauri build --debug --no-bundle`);
  }

  // Isolated per-run data dir so this never reads or writes a real user's
  // library/database (see src-tauri/src/paths.rs).
  const dataDir = mkdtempSync(path.join(tmpdir(), 'luminous-e2e-'));
  console.log(`Starting tauri-driver (app data dir: ${dataDir})...`);

  let session: DriverSession | undefined;
  try {
    session = await startSession(APP_EXE, dataDir);

    console.log('Verifying app window loaded...');
    const deadline = Date.now() + 15_000;
    let navFound = false;
    let lastSource = '';
    while (Date.now() < deadline) {
      lastSource = await getSource(session);
      if (/<nav[\s>]/.test(lastSource)) {
        navFound = true;
        break;
      }
      await new Promise((r) => setTimeout(r, 500));
    }

    if (!navFound) {
      const title = await getTitle(session).catch(() => '<unavailable>');
      throw new Error(
        `Sidebar <nav> element did not render within 15s — app did not load correctly.\n\n` +
          `Document title: ${title}\n\nPage source (first 2000 chars):\n${lastSource.slice(0, 2000)}`,
      );
    }

    console.log('✅ Smoke test passed: app launched and sidebar rendered.');
  } finally {
    if (session) await stopSession(session);
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
