// Dev-time tool for visually inspecting the real running app (real Rust
// backend, real IPC, real SQLite) — not part of the CI test suite. Built for
// #779's follow-up: gives an agent a way to launch the app, click around,
// and take real screenshots while working on an issue, instead of only
// being able to run non-visual checks.
//
// Windows only, and needs the same one-time setup as `bun run
// test:e2e:windows` (see docs/TROUBLESHOOTING.md): `cargo install
// tauri-driver --locked` plus a matching `msedgedriver.exe` on PATH.
//
// Each subcommand is a separate process — session state (driver PID +
// WebDriver session id) is persisted to .inspect-session.json (gitignored)
// between calls so `start` can run once and later commands reuse it.
//
// Usage:
//   bunx tsx scripts/inspect-app.ts start [--real]
//     --real points the app at your actual library/database instead of an
//     isolated temp directory — use only for read-only visual comparison,
//     never for exercising clicks that mutate state (ratings, deletes,
//     playlist edits, etc.).
//   bunx tsx scripts/inspect-app.ts screenshot <output.png>
//   bunx tsx scripts/inspect-app.ts screenshot <output.png> --css "<selector>"
//     Crops to just that element (via the driver, no manual DPI math) —
//     use this over a full-page screenshot when checking pixel-level detail
//     like a hover outline/corner treatment.
//   bunx tsx scripts/inspect-app.ts click --css "<selector>"
//   bunx tsx scripts/inspect-app.ts click --text "<visible text>"
//   bunx tsx scripts/inspect-app.ts hover --css "<selector>"
//   bunx tsx scripts/inspect-app.ts type --css "<selector>" "<text>"
//   bunx tsx scripts/inspect-app.ts source
//   bunx tsx scripts/inspect-app.ts url
//   bunx tsx scripts/inspect-app.ts stop
import { existsSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import {
  clickElement,
  elementScreenshot,
  getSource,
  getUrl,
  hoverElement,
  screenshot,
  startSession,
  stopSession,
  typeIntoElement,
  webdriverRequest,
  type DriverSession,
} from '../e2e/webdriver-client';

const REPO_ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const APP_EXE = path.join(REPO_ROOT, 'target', 'debug', 'LuminousMusicPlayer.exe');
const STATE_FILE = path.join(REPO_ROOT, '.inspect-session.json');

interface PersistedState {
  driverPid: number;
  sessionId: string;
  dataDir?: string; // absent when running against the real data dir (--real)
}

function loadState(): PersistedState {
  if (!existsSync(STATE_FILE)) {
    throw new Error('No running session. Start one first: bunx tsx scripts/inspect-app.ts start');
  }
  return JSON.parse(readFileSync(STATE_FILE, 'utf8'));
}

function asSession(state: PersistedState): DriverSession {
  // Reconstruct just enough of a DriverSession for the read-only helpers —
  // stopSession/webdriverRequest only need sessionId; `driver.kill()` is
  // replaced with a direct pid kill in cmdStop since we don't hold the
  // original ChildProcess across process invocations.
  return {
    sessionId: state.sessionId,
    driver: { kill: () => {} } as unknown as DriverSession['driver'],
    driverOutput: { value: '' },
  };
}

async function cmdStart(args: string[]) {
  if (existsSync(STATE_FILE)) {
    throw new Error('A session is already running. Stop it first: bunx tsx scripts/inspect-app.ts stop');
  }
  if (!existsSync(APP_EXE)) {
    throw new Error(`App binary not found at ${APP_EXE}.\nBuild it first: bunx tauri build --debug --no-bundle`);
  }

  const real = args.includes('--real');
  const dataDir = real ? undefined : mkdtempSync(path.join(tmpdir(), 'luminous-inspect-'));

  console.log(real ? 'Starting app against your REAL library (read-only exploration only)...' : `Starting app with isolated data dir: ${dataDir}`);
  const session = await startSession(APP_EXE, dataDir);

  const state: PersistedState = { driverPid: session.driver.pid!, sessionId: session.sessionId, dataDir };
  writeFileSync(STATE_FILE, JSON.stringify(state, null, 2));
  console.log(`Session started (pid ${state.driverPid}). Use 'screenshot', 'click', 'source', 'stop'.`);
}

async function cmdStop() {
  const state = loadState();
  await webdriverRequest('DELETE', `/session/${state.sessionId}`).catch(() => {});
  try {
    process.kill(state.driverPid);
  } catch {
    // already gone
  }
  if (state.dataDir) rmSync(state.dataDir, { recursive: true, force: true });
  rmSync(STATE_FILE, { force: true });
  console.log('Session stopped.');
}

async function cmdScreenshot(args: string[]) {
  const outPath = args[0];
  if (!outPath) throw new Error('Usage: screenshot <output.png> [--css/--text <sel>]');
  const session = asSession(loadState());
  const hasSelector = args.includes('--css') || args.includes('--text');
  const base64 = hasSelector ? await elementScreenshot(session, ...selectorFor(args.slice(1))) : await screenshot(session);
  writeFileSync(path.resolve(outPath), Buffer.from(base64, 'base64'));
  console.log(`Saved screenshot to ${path.resolve(outPath)}`);
}

function selectorFor(args: string[]): [string, string] {
  const { using, value } = parseSelectorArgs(args);
  return [using, value];
}

function parseSelectorArgs(args: string[]): { using: string; value: string; rest: string[] } {
  const cssIdx = args.indexOf('--css');
  if (cssIdx !== -1) return { using: 'css selector', value: args[cssIdx + 1], rest: args.filter((_, i) => i !== cssIdx && i !== cssIdx + 1) };
  const textIdx = args.indexOf('--text');
  if (textIdx !== -1) {
    const value = args[textIdx + 1];
    return { using: 'xpath', value: `//*[contains(text(),'${value}')]`, rest: args.filter((_, i) => i !== textIdx && i !== textIdx + 1) };
  }
  throw new Error('Provide --css "<selector>" or --text "<visible text>"');
}

async function cmdClick(args: string[]) {
  const { using, value } = parseSelectorArgs(args);
  const session = asSession(loadState());
  await clickElement(session, using, value);
  console.log(`Clicked ${using}=${value}`);
}

async function cmdHover(args: string[]) {
  const { using, value } = parseSelectorArgs(args);
  const session = asSession(loadState());
  await hoverElement(session, using, value);
  console.log(`Hovering ${using}=${value}`);
}

async function cmdType(args: string[]) {
  const { using, value, rest } = parseSelectorArgs(args);
  const text = rest[0];
  if (!text) throw new Error('Usage: type --css "<selector>" "<text>"');
  const session = asSession(loadState());
  await typeIntoElement(session, using, value, text);
  console.log(`Typed into ${using}=${value}`);
}

async function cmdSource() {
  const session = asSession(loadState());
  console.log(await getSource(session));
}

async function cmdUrl() {
  const session = asSession(loadState());
  console.log(await getUrl(session));
}

async function main() {
  if (process.platform !== 'win32') {
    console.error('This tool is Windows-only (see issue #779).');
    process.exitCode = 1;
    return;
  }

  const [command, ...args] = process.argv.slice(2);
  switch (command) {
    case 'start':
      return cmdStart(args);
    case 'stop':
      return cmdStop();
    case 'screenshot':
      return cmdScreenshot(args);
    case 'click':
      return cmdClick(args);
    case 'hover':
      return cmdHover(args);
    case 'type':
      return cmdType(args);
    case 'source':
      return cmdSource();
    case 'url':
      return cmdUrl();
    default:
      console.error('Usage: inspect-app.ts <start [--real]|stop|screenshot <path>|click --css/--text <sel>|hover --css/--text <sel>|type --css/--text <sel> <text>|source|url>');
      process.exitCode = 1;
  }
}

main()
  .catch((err) => {
    console.error(`\n❌ ${err instanceof Error ? err.message : String(err)}\n`);
    process.exitCode = 1;
  })
  .finally(() => process.exit(process.exitCode ?? 0));
