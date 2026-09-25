#!/usr/bin/env bun
/**
 * Runs the three memory scenarios from docs/PERFORMANCE.md (idle, after a
 * forced full scan, playback with EQ + analyzer) against a release build,
 * identically every time, and appends one median row per scenario to
 * docs/performance-history.csv. Windows only — it drives the app through
 * WebView2's Chrome DevTools Protocol port, which WebKitGTK doesn't speak;
 * on Linux, follow the manual steps in docs/PERFORMANCE.md instead.
 *
 * What it holds constant between runs, so two versions' rows are comparable:
 *   - a release exe built from the current app source (refuses a stale one)
 *   - a fresh launch into Collection → Songs with nothing selected
 *   - a fixed window size (--window, default 1400x900), on screen
 *   - settle time before sampling, and the median of --samples readings
 *   - the same IPC calls the UI's buttons make (Force Full Scan, Play)
 *
 * It runs against your real library and settings. Everything it changes —
 * the restored view, window placement, EQ enabled state, play/pause state and
 * position — is put back before the app closes. If the run dies partway,
 * the saved view/window state is in the backup file it prints at startup.
 * Playback is audible at your current volume for about a minute, from 0:00 of
 * whatever track is loaded, and stops before the track's halfway mark so it
 * never records a play or scrobble (it refuses a track too short for that).
 *
 * Usage (build first: `bun run tauri build --no-bundle`):
 *   bun run scripts/perf-memory-scenarios.ts --app-version 2.5.0
 *
 * Options:
 *   --app-version <ver>  version recorded in the CSV (default: package.json's — pass the upcoming
 *                        release's version when measuring before the version bump)
 *   --csv <path>         default docs/performance-history.csv
 *   --window <WxH>       pinned outer window size in physical pixels (default 1400x900)
 *   --samples <n>        readings per scenario, median recorded (default 5)
 *   --interval <sec>     seconds between readings (default 5)
 *   --exe <path>         default target/release/LuminousMusicPlayer.exe
 *   --dry-run            run everything but print the rows instead of appending them
 */

import { execFileSync, spawn } from "node:child_process";
import { existsSync, statSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";
import {
  APP_SOURCE_PATHS,
  appCommit,
  appendCsvRow,
  csvLine,
  formatSnapshot,
  osLabel,
  packageVersion,
  sampleMedian,
  type CsvRow,
  type Snapshot,
} from "./measure-memory";
import { CdpClient } from "./monitor-cdp";

const REPO_ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const CDP_PORT = 9222;
const BACKUP_FILE = path.join(tmpdir(), "luminous-perf-restore.json");

// Seconds to let each scenario settle before sampling (startup/scan/playback
// allocations churn for a while before memory flattens out).
const SETTLE_IDLE_SEC = 60;
const SETTLE_AFTER_SCAN_SEC = 20;
const SETTLE_PLAYBACK_SEC = 30;

function parseArgs() {
  const args = process.argv.slice(2);
  const get = (flag: string) => {
    const i = args.indexOf(flag);
    return i !== -1 ? args[i + 1] : undefined;
  };
  const window = get("--window") ?? "1400x900";
  const [width, height] = window.split("x").map(Number);
  if (!width || !height) throw new Error(`--window must look like 1400x900, got "${window}"`);
  return {
    appVersion: get("--app-version") ?? packageVersion(),
    csv: get("--csv") ?? path.join(REPO_ROOT, "docs", "performance-history.csv"),
    window,
    width,
    height,
    samples: Number(get("--samples") ?? "5"),
    intervalSec: Number(get("--interval") ?? "5"),
    exe: get("--exe") ?? path.join(REPO_ROOT, "target", "release", "LuminousMusicPlayer.exe"),
    dryRun: args.includes("--dry-run"),
  };
}

const sleep = (sec: number) => new Promise((r) => setTimeout(r, sec * 1000));
const log = (msg: string) => console.log(`[perf] ${msg}`);

// ── Win32 window helpers (via PowerShell) ─────────────────────────────────

const WIN32 = `
Add-Type -Namespace LumPerf -Name Win -MemberDefinition @'
[StructLayout(LayoutKind.Sequential)] public struct PT { public int X, Y; }
[StructLayout(LayoutKind.Sequential)] public struct RC { public int L, T, R, B; }
[StructLayout(LayoutKind.Sequential)] public struct WP { public int length, flags, showCmd; public PT min, max; public RC normal; }
[DllImport("user32.dll")] static extern bool GetWindowPlacement(IntPtr h, ref WP p);
[DllImport("user32.dll")] static extern bool SetWindowPlacement(IntPtr h, ref WP p);
[DllImport("user32.dll")] static extern bool ShowWindow(IntPtr h, int c);
[DllImport("user32.dll")] static extern bool SetWindowPos(IntPtr h, IntPtr after, int x, int y, int w, int hh, uint f);
public static string Save(IntPtr h) {
  var p = new WP(); p.length = Marshal.SizeOf(p); GetWindowPlacement(h, ref p);
  return string.Join(",", p.flags, p.showCmd, p.min.X, p.min.Y, p.max.X, p.max.Y, p.normal.L, p.normal.T, p.normal.R, p.normal.B);
}
public static void Restore(IntPtr h, string s) {
  var v = Array.ConvertAll(s.Split(','), int.Parse);
  var p = new WP(); p.length = Marshal.SizeOf(p);
  p.flags = v[0]; p.showCmd = v[1]; p.min.X = v[2]; p.min.Y = v[3]; p.max.X = v[4]; p.max.Y = v[5];
  p.normal.L = v[6]; p.normal.T = v[7]; p.normal.R = v[8]; p.normal.B = v[9];
  SetWindowPlacement(h, ref p);
}
public static void Pin(IntPtr h, int w, int hh) { ShowWindow(h, 9); SetWindowPos(h, IntPtr.Zero, 100, 100, w, hh, 0x14); }
'@
$main = Get-Process -Name LuminousMusicPlayer -ErrorAction SilentlyContinue | Where-Object { $_.MainWindowHandle -ne 0 } | Select-Object -First 1
if (-not $main) { throw 'Luminous main window not found' }
$h = $main.MainWindowHandle
`;

function ps(script: string): string {
  return execFileSync("powershell", ["-NoProfile", "-Command", script], { encoding: "utf8" }).trim();
}

const saveWindowPlacement = () => ps(`${WIN32}[LumPerf.Win]::Save($h)`);
function restoreWindowPlacement(placement: string) {
  // Spliced into a PowerShell command, so accept only what Save() produces: ten integers.
  if (!/^-?\d+(,-?\d+){9}$/.test(placement)) throw new Error(`Unexpected window placement "${placement}"`);
  ps(`${WIN32}[LumPerf.Win]::Restore($h, '${placement}')`);
}
const pinWindow = (w: number, h: number) => ps(`${WIN32}[LumPerf.Win]::Pin($h, ${w}, ${h})`);

function isRunning(): boolean {
  return ps("@(Get-Process -Name LuminousMusicPlayer -ErrorAction SilentlyContinue).Count") !== "0";
}

/** Closes via WM_CLOSE (not a kill) so the app saves its state on the way out. */
async function closeGracefully() {
  ps("Get-Process -Name LuminousMusicPlayer -ErrorAction SilentlyContinue | ForEach-Object { $_.CloseMainWindow() | Out-Null }");
  for (let i = 0; i < 30; i++) {
    if (!isRunning()) return;
    await sleep(1);
  }
  throw new Error("Luminous didn't exit within 30s of being asked to close; close it by hand.");
}

// ── App launch + IPC ──────────────────────────────────────────────────────

async function launch(exe: string): Promise<CdpClient> {
  const existing = process.env.WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS ?? "";
  spawn(exe, [], {
    detached: true,
    stdio: "ignore",
    env: {
      ...process.env,
      // Release builds don't open the devtools port themselves (see
      // remote_devtools_enabled() in src-tauri/src/lib.rs), but they append
      // to whatever is already in this variable, so we can pass it in.
      WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: `${existing} --remote-debugging-port=${CDP_PORT}`.trim(),
    },
  }).unref();

  for (let i = 0; i < 60; i++) {
    await sleep(1);
    const cdp = new CdpClient();
    try {
      await cdp.connect(CDP_PORT);
      const ready = await cdp.eval("document.readyState === 'complete' && !!window.__TAURI_INTERNALS__");
      if (ready.value === true) return cdp;
      cdp.close();
    } catch {
      cdp.close();
    }
  }
  throw new Error(`Luminous didn't expose a ready page on CDP port ${CDP_PORT} within 60s.`);
}

/**
 * Calls a fixed in-page function with `args` passed as CDP call arguments
 * (Runtime.callFunctionOn), never spliced into source text, so values like
 * the saved localStorage entries can't change what code runs in the page.
 */
async function callInPage<T>(cdp: CdpClient, fn: string, ...args: unknown[]): Promise<T> {
  const global = await cdp.send("Runtime.evaluate", { expression: "globalThis" });
  const res = await cdp.send("Runtime.callFunctionOn", {
    objectId: global.result.objectId,
    functionDeclaration: fn,
    arguments: args.map((value) => ({ value })),
    awaitPromise: true,
    returnByValue: true,
  });
  if (res.exceptionDetails) {
    throw new Error(res.exceptionDetails.exception?.description ?? res.exceptionDetails.text);
  }
  return res.result?.value as T;
}

async function invoke<T = unknown>(cdp: CdpClient, cmd: string, args: Record<string, unknown> = {}): Promise<T> {
  try {
    return await callInPage<T>(cdp, "function (cmd, args) { return this.__TAURI_INTERNALS__.invoke(cmd, args); }", cmd, args);
  } catch (e) {
    throw new Error(`invoke('${cmd}') failed: ${e instanceof Error ? e.message : e}`);
  }
}

/** Runs `fn` with a short-lived CDP connection, so an attached debugger isn't inflating the renderer while sampling. */
async function withCdp<T>(fn: (cdp: CdpClient) => Promise<T>): Promise<T> {
  const cdp = new CdpClient();
  await cdp.connect(CDP_PORT);
  try {
    return await fn(cdp);
  } finally {
    cdp.close();
  }
}

const NAV_PREFIX = "navigation_";

function readNavigationKeys(cdp: CdpClient): Promise<Record<string, string>> {
  return callInPage(
    cdp,
    `function (prefix) {
      return Object.fromEntries(Object.keys(localStorage).filter((k) => k.startsWith(prefix)).map((k) => [k, localStorage.getItem(k)]));
    }`,
    NAV_PREFIX,
  );
}

async function writeNavigationKeys(cdp: CdpClient, keys: Record<string, string>) {
  await callInPage(
    cdp,
    `function (prefix, keys) {
      for (const k of Object.keys(localStorage)) if (k.startsWith(prefix)) localStorage.removeItem(k);
      for (const [k, v] of Object.entries(keys)) localStorage.setItem(k, v);
    }`,
    NAV_PREFIX,
    keys,
  );
}

// The fixed view every run measures: Collection → Songs, nothing selected.
const CANONICAL_VIEW = { navigation_activeTab: "collection", navigation_activeSubTab: "songs" };

// ── Run ───────────────────────────────────────────────────────────────────

interface PlaybackState {
  state: "stopped" | "playing" | "paused";
  current_song: { title: string; length_nanosec: number | null } | null;
  position_nanosec: number;
}
interface EqualizerConfig {
  enabled: boolean;
  [key: string]: unknown;
}

function checkPreconditions(opts: ReturnType<typeof parseArgs>) {
  if (process.platform !== "win32") {
    throw new Error("This script is Windows-only (it needs WebView2's CDP port). See docs/PERFORMANCE.md for the Linux steps.");
  }
  if (isRunning()) {
    throw new Error("Luminous is already running. Close it first — launching a second instance would hand off to yours.");
  }
  if (!existsSync(opts.exe)) {
    throw new Error(`No release build at ${opts.exe}. Build one first: bun run tauri build --no-bundle`);
  }
  const lastSourceChange = Number(
    execFileSync("git", ["log", "-1", "--format=%ct", "--", ...APP_SOURCE_PATHS], { cwd: REPO_ROOT, encoding: "utf8" }).trim(),
  );
  if (statSync(opts.exe).mtimeMs / 1000 < lastSourceChange) {
    throw new Error(`${opts.exe} is older than the last app-source commit — rebuild it (bun run tauri build --no-bundle).`);
  }
  if (appCommit().endsWith("-dirty")) {
    log("warning: app source has uncommitted changes; the commit column will be marked -dirty.");
  }
}

async function main() {
  const opts = parseArgs();
  checkPreconditions(opts);

  const rows: CsvRow[] = [];
  const base = {
    app_version: opts.appVersion,
    commit: appCommit(),
    os: osLabel(),
    window: opts.window,
  };
  const row = (label: string, s: Snapshot, extra: Partial<CsvRow> = {}): CsvRow => ({
    timestamp: new Date().toISOString(),
    label,
    ...base,
    library_tracks: "",
    process_count: s.processCount,
    working_set_mb: s.workingSetMb.toFixed(1),
    private_bytes_mb: s.privateBytesMb.toFixed(1),
    samples: opts.samples,
    scan_seconds: "",
    ...extra,
  });

  // 1. Launch once to save the user's view + window placement and switch to
  //    the canonical view and window size, then close so the measured launch
  //    starts fresh straight into them.
  log(`launching ${opts.exe}`);
  let cdp = await launch(opts.exe);
  const savedNav = await readNavigationKeys(cdp);
  const savedPlacement = saveWindowPlacement();
  writeFileSync(BACKUP_FILE, JSON.stringify({ navigation: savedNav, windowPlacement: savedPlacement }, null, 2));
  log(`saved your view/window state (backup: ${BACKUP_FILE})`);
  await writeNavigationKeys(cdp, CANONICAL_VIEW);
  pinWindow(opts.width, opts.height);
  cdp.close();
  await closeGracefully();

  let savedEq: EqualizerConfig | null = null;
  let savedPlayback: PlaybackState | null = null;
  try {
    // 2. Idle
    log("relaunching into the canonical view");
    cdp = await launch(opts.exe);
    pinWindow(opts.width, opts.height);
    const visualizerMounted = await cdp.eval("document.querySelectorAll('canvas').length > 0");
    const tracks = (await invoke<{ total_songs: number }>(cdp, "get_library_stats")).total_songs;
    cdp.close();
    if (visualizerMounted.value !== true) {
      log("warning: no visualizer canvas found — the playback scenario won't exercise the analyzer's rendering.");
    }
    log(`idle: settling ${SETTLE_IDLE_SEC}s, library has ${tracks} tracks`);
    await sleep(SETTLE_IDLE_SEC);
    const onSample = (s: Snapshot) => console.log(`  ${formatSnapshot("sample", s)}`);
    const idle = await sampleMedian(opts.samples, opts.intervalSec, { onSample });
    rows.push(row("idle", idle, { library_tracks: tracks }));

    // 3. Forced full scan — same call as Settings → Force Full Scan.
    log("starting forced full scan");
    const scanResult = await withCdp((c) =>
      c.eval(
        `(async () => { const t = performance.now(); await window.__TAURI_INTERNALS__.invoke('scan_directories', { force: true }); return (performance.now() - t) / 1000; })()`,
      ),
    );
    if (scanResult.error) throw new Error(`Scan failed: ${scanResult.error}`);
    const scanSeconds = Number(scanResult.value).toFixed(1);
    log(`scan took ${scanSeconds}s; settling ${SETTLE_AFTER_SCAN_SEC}s`);
    await sleep(SETTLE_AFTER_SCAN_SEC);
    const afterScan = await sampleMedian(opts.samples, opts.intervalSec, { onSample });
    rows.push(row("after-full-scan", afterScan, { library_tracks: tracks, scan_seconds: scanSeconds }));

    // 4. Playback with EQ + analyzer on. Plays the loaded track from 0:00
    //    and stops well short of its halfway mark, which is where a listen
    //    gets recorded to play stats (and scrobbled) — see
    //    Player::on_position_update. Seeking while paused doesn't count.
    const playSeconds = SETTLE_PLAYBACK_SEC + opts.samples * (opts.intervalSec + 2) + 10;
    await withCdp(async (c) => {
      savedPlayback = await invoke<PlaybackState>(c, "get_playback_state");
      const song = savedPlayback.current_song;
      if (!song) {
        throw new Error("Nothing is loaded in the player — load a track in Luminous (and pause it), then re-run.");
      }
      const halfway = (song.length_nanosec ?? 0) / 2e9;
      if (halfway <= playSeconds) {
        throw new Error(
          `"${song.title}" is too short: this scenario plays ~${playSeconds}s, which would pass its halfway mark ` +
            `(${halfway.toFixed(0)}s) and record a play. Load a track longer than ${Math.ceil((playSeconds * 2) / 60)} minutes and re-run.`,
        );
      }
      savedEq = await invoke<EqualizerConfig>(c, "get_equalizer_state");
      if (!savedEq.enabled) await invoke(c, "apply_equalizer_config", { config: { ...savedEq, enabled: true } });
      await invoke(c, "pause");
      await invoke(c, "seek_to", { positionNanosec: 0 });
      await invoke(c, "resume");
      log(`playing "${song.title}" from 0:00; settling ${SETTLE_PLAYBACK_SEC}s`);
    });
    await sleep(SETTLE_PLAYBACK_SEC);
    const playback = await sampleMedian(opts.samples, opts.intervalSec, { onSample });
    await withCdp((c) => invoke(c, "pause"));
    rows.push(row("playback-eq-analyzer", playback, { library_tracks: tracks }));
  } finally {
    // 5. Put back everything the run changed, then close so the app saves it.
    //    (Relaunch first if a failure left it closed — step 1 already
    //    overwrote the saved view and window size.)
    if (!isRunning()) (await launch(opts.exe)).close();
    {
      await withCdp(async (c) => {
        // The app always launches paused, so "restore" means paused at the
        // saved position (seeking while paused never records a play).
        const pb = savedPlayback as PlaybackState | null;
        if (pb) {
          await invoke(c, "pause");
          await invoke(c, "seek_to", { positionNanosec: pb.position_nanosec });
        }
        const eq = savedEq as EqualizerConfig | null;
        if (eq && !eq.enabled) await invoke(c, "apply_equalizer_config", { config: eq });
        await writeNavigationKeys(c, savedNav);
      });
      restoreWindowPlacement(savedPlacement);
      await closeGracefully();
      log("restored your view, window, EQ and playback state");
    }
  }

  for (const r of rows) {
    if (opts.dryRun) console.log(csvLine(r));
    else appendCsvRow(opts.csv, r);
  }
  log(opts.dryRun ? "dry run — nothing written" : `appended ${rows.length} rows to ${opts.csv}`);
  log("compare with: python scripts/perf-chart.py --baseline <ver> --candidate " + opts.appVersion);
}

main().catch((err) => {
  console.error(`[perf] ${err instanceof Error ? err.message : err}`);
  process.exit(1);
});
