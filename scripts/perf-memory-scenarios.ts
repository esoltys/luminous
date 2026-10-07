#!/usr/bin/env bun
/**
 * Runs the memory scenarios from docs/PERFORMANCE.md against a release build,
 * identically every time, and appends one row per scenario to
 * docs/performance-history.csv. Windows only — it drives the app through
 * WebView2's Chrome DevTools Protocol port, which WebKitGTK doesn't speak;
 * on Linux, follow the manual steps in docs/PERFORMANCE.md instead.
 *
 * Every source is measured in its own throwaway profile: LUMINOUS_DATA_DIR
 * and WEBVIEW2_USER_DATA_FOLDER both point into a fresh temp folder, so the
 * build under test creates its own database (any past release can be
 * measured — nothing has been migrated under it), and nothing it plays or
 * changes reaches your real profile. Your real database is only ever read, to
 * find your watched folders and the baseline track. (Builds before #1197 also
 * save window placement in the real profile; that file is restored afterwards.) The music files are your
 * real library, read in place. Before adding any folder the profile records
 * "no default library", so the genre hierarchy sidecar is never written into
 * a music folder (sidecar cover art is already off in a fresh profile).
 *
 * Sources and the scenarios each one runs (CSV label in brackets):
 *   local     first scan of the watched folders, embedded/folder art included [initial-scan],
 *             idle after a relaunch [idle], a forced full rescan [after-full-scan], the
 *             baseline track with EQ + analyzer on [playback-eq-analyzer], and scrolling
 *             the whole Albums grid, which generates every cover thumbnail [album-grid]
 *   webdav    first sync [webdav-sync], idle after a relaunch [webdav-idle], the
 *             baseline track streamed with EQ + analyzer on [webdav-playback]
 *   subsonic  the same three against an OpenSubsonic server (e.g. Navidrome)
 *             [subsonic-sync, subsonic-idle, subsonic-playback]
 *
 * Each row has the settled median of --samples readings, plus the peak seen by
 * a background poll across the whole scenario (the work, the settle and the
 * samples). A remote source's baseline track is found by title, album and
 * length, so the server must serve the same file the local library has.
 *
 * What it holds constant between runs, so two versions' rows are comparable:
 *   - a release exe built from the current app source (refuses a stale one)
 *   - a fresh profile per source, launched into Collection → Songs with nothing selected
 *   - a fixed window size (--window, default 1400x900), on screen
 *   - settle time before sampling, and the median of --samples readings
 *   - the same IPC calls the UI's buttons make (Add Folder, Force Full Scan, Sync Now, Play)
 *   - the same baseline track (--track), played from 0:00 — decode cost
 *     differs by format, so an MP3 run and a FLAC run aren't comparable
 * Your library itself still grows; each row records its track count, so
 * compare versions measured back to back on the same day.
 *
 * Playback is audible at the fresh profile's default volume for about a minute
 * per source, and stops before the track's halfway mark, so it never records a
 * play or scrobble (it refuses a track too short for that). Subsonic play
 * reporting is turned off for the measured server either way.
 *
 * Usage (build first: `bun run tauri build --no-bundle`):
 *   bun run scripts/perf-memory-scenarios.ts --app-version 2.5.0 --track "D:/Music/Artist/Album/01 Song.flac"
 *   bun run scripts/perf-memory-scenarios.ts --track "<path>" --sources local,webdav,subsonic
 *
 * Options:
 *   --track <path>       the baseline track, as stored in your real library (required —
 *                        docs/PERFORMANCE.md names the one the recorded history uses)
 *   --sources <list>     comma-separated: local, webdav, subsonic (default: local)
 *   --library <dir>      a folder for the local source; repeat for more (default: the
 *                        watched folders of your real profile)
 *   --app-version <ver>  version recorded in the CSV (default: package.json's — pass the upcoming
 *                        release's version when measuring before the version bump)
 *   --csv <path>         default docs/performance-history.csv
 *   --window <WxH>       pinned outer window size in physical pixels (default 1400x900)
 *   --samples <n>        readings per scenario, median recorded (default 5)
 *   --interval <sec>     seconds between readings (default 5)
 *   --exe <path>         default target/release/LuminousMusicPlayer.exe; a build from another checkout
 *                        (e.g. a baseline worktree) is checked and labelled against that checkout
 *   --keep-profiles      leave the scratch profiles in the temp folder for inspection (they hold
 *                        the remote servers' passwords, so they're deleted by default)
 *   --dry-run            run everything but print the rows instead of appending them
 *
 * Remote servers come from the environment, so credentials stay out of the
 * command line and the CSV:
 *   LUMINOUS_PERF_WEBDAV_URL, LUMINOUS_PERF_WEBDAV_USER, LUMINOUS_PERF_WEBDAV_PASSWORD,
 *   LUMINOUS_PERF_WEBDAV_PATH (optional remote folder)
 *   LUMINOUS_PERF_SUBSONIC_URL, LUMINOUS_PERF_SUBSONIC_USER, LUMINOUS_PERF_SUBSONIC_PASSWORD
 */

import { Database } from "bun:sqlite";
import { execFileSync, spawn } from "node:child_process";
import { existsSync, mkdtempSync, readFileSync, rmSync, statSync, writeFileSync } from "node:fs";
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
  PeakTracker,
  sampleMedian,
  type CsvRow,
  type Snapshot,
} from "./measure-memory";
import { defaultDbPath } from "./mock-library";
import { CdpClient } from "./monitor-cdp";

const REPO_ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const CDP_PORT = 9222;

// Seconds to let each scenario settle before sampling (startup/scan/playback
// allocations churn for a while before memory flattens out).
const SETTLE_IDLE_SEC = 60;
const SETTLE_AFTER_SCAN_SEC = 20;
const SETTLE_PLAYBACK_SEC = 30;

const SOURCES = ["local", "webdav", "subsonic"] as const;
type Source = (typeof SOURCES)[number];

function parseArgs() {
  const args = process.argv.slice(2);
  const get = (flag: string) => {
    const i = args.indexOf(flag);
    return i !== -1 ? args[i + 1] : undefined;
  };
  const getAll = (flag: string) => args.flatMap((a, i) => (a === flag && args[i + 1] ? [args[i + 1]] : []));
  const window = get("--window") ?? "1400x900";
  const [width, height] = window.split("x").map(Number);
  if (!width || !height) throw new Error(`--window must look like 1400x900, got "${window}"`);
  const track = get("--track");
  if (!track) throw new Error("--track <path> is required — docs/PERFORMANCE.md names the baseline track.");
  const sources = (get("--sources") ?? "local").split(",").map((s) => s.trim());
  for (const s of sources) {
    if (!SOURCES.includes(s as Source)) throw new Error(`Unknown source "${s}" — use ${SOURCES.join(", ")}.`);
  }
  const exe = get("--exe") ?? path.join(REPO_ROOT, "target", "release", "LuminousMusicPlayer.exe");
  return {
    track,
    sources: sources as Source[],
    libraries: getAll("--library"),
    appVersion: get("--app-version") ?? packageVersion(),
    csv: get("--csv") ?? path.join(REPO_ROOT, "docs", "performance-history.csv"),
    window,
    width,
    height,
    samples: Number(get("--samples") ?? "5"),
    intervalSec: Number(get("--interval") ?? "5"),
    exe,
    // The checkout the exe was built in (e.g. a baseline worktree), for the staleness check and commit column.
    exeRepo: exeRepoRoot(exe),
    keepProfiles: args.includes("--keep-profiles"),
    dryRun: args.includes("--dry-run"),
  };
}
type Options = ReturnType<typeof parseArgs>;

function exeRepoRoot(exe: string): string {
  try {
    return execFileSync("git", ["rev-parse", "--show-toplevel"], { cwd: path.dirname(exe), encoding: "utf8" }).trim();
  } catch {
    return REPO_ROOT;
  }
}

const sleep = (sec: number) => new Promise((r) => setTimeout(r, sec * 1000));
const log = (msg: string) => console.log(`[perf] ${msg}`);

// ── Win32 window helpers (via PowerShell) ─────────────────────────────────

const PIN_WINDOW = `
Add-Type -Namespace LumPerf -Name Win -MemberDefinition @'
[DllImport("user32.dll")] public static extern bool ShowWindow(IntPtr h, int c);
[DllImport("user32.dll")] public static extern bool SetWindowPos(IntPtr h, IntPtr after, int x, int y, int w, int hh, uint f);
'@
$main = Get-Process -Name LuminousMusicPlayer -ErrorAction SilentlyContinue | Where-Object { $_.MainWindowHandle -ne 0 } | Select-Object -First 1
if (-not $main) { throw 'Luminous main window not found' }
[LumPerf.Win]::ShowWindow($main.MainWindowHandle, 9) | Out-Null
`;

function ps(script: string): string {
  return execFileSync("powershell", ["-NoProfile", "-Command", script], { encoding: "utf8" }).trim();
}

const pinWindow = (w: number, h: number) =>
  ps(`${PIN_WINDOW}[LumPerf.Win]::SetWindowPos($main.MainWindowHandle, [IntPtr]::Zero, 100, 100, ${w}, ${h}, 0x14) | Out-Null`);

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

// ── IPC over CDP ──────────────────────────────────────────────────────────

/**
 * Calls a fixed in-page function with `args` passed as CDP call arguments
 * (Runtime.callFunctionOn), never spliced into source text, so values like
 * server passwords can't change what code runs in the page.
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

/** Times an awaited IPC call, in seconds. */
async function timed<T>(work: () => Promise<T>): Promise<{ result: T; seconds: string }> {
  const t = performance.now();
  const result = await work();
  return { result, seconds: ((performance.now() - t) / 1000).toFixed(1) };
}

// ── Baseline track ────────────────────────────────────────────────────────

// The keys Player::new restores the loaded track and position from.
const PLAYER_KEYS = ["last_song_id", "last_playlist_id", "last_item_uuid", "last_position_nanosec", "last_adhoc_song_ids"];
// models.rs FileType
const FILE_TYPES: Record<number, string> = {
  1: "MP3",
  2: "FLAC",
  3: "Ogg FLAC",
  4: "Ogg Vorbis",
  5: "Opus",
  6: "Speex",
  7: "AAC",
  8: "ALAC",
  9: "AIFF",
  10: "WAV",
};

interface BaselineTrack {
  id: number;
  title: string;
  album: string;
  filetype: number;
  samplerate: number;
  bitdepth: number | null;
  bitrate: number;
  length_nanosec: number;
}

const TRACK_COLUMNS = "id, title, album, filetype, samplerate, bitdepth, bitrate, length_nanosec";

const describeTrack = (t: BaselineTrack) =>
  `"${t.title}" (${FILE_TYPES[t.filetype] ?? `filetype ${t.filetype}`}, ${t.samplerate} Hz` +
  `${t.bitdepth ? `/${t.bitdepth}-bit` : ""}, ${t.bitrate} kbps, ${Math.round(t.length_nanosec / 1e9)}s)`;

function openRealDbReadOnly(): Database {
  const dbPath = defaultDbPath();
  if (!dbPath || !existsSync(dbPath)) throw new Error(`Your Luminous database wasn't found at ${dbPath}.`);
  return new Database(dbPath, { readonly: true });
}

/** Reads the baseline track and your watched folders from the real profile, without writing to it. */
function readRealProfile(trackPath: string): { track: BaselineTrack; folders: string[] } {
  const db = openRealDbReadOnly();
  try {
    const track = db
      .query(`SELECT ${TRACK_COLUMNS} FROM songs WHERE path = ?1 COLLATE NOCASE AND unavailable = 0 AND cue_path IS NULL`)
      .get(path.normalize(trackPath)) as BaselineTrack | null;
    if (!track) throw new Error(`--track "${trackPath}" isn't an available, non-CUE song in your library.`);
    const folders = (db.query("SELECT path FROM directories ORDER BY id").all() as { path: string }[]).map((d) => d.path);
    return { track, folders };
  } finally {
    db.close();
  }
}

/**
 * Builds older than #1197 save window placement in the real profile's config
 * folder even under LUMINOUS_DATA_DIR (newer ones keep it in the scratch
 * profile), so the real file is snapshotted up front and put back after.
 * Returns the restore function.
 */
function protectRealWindowState(): () => void {
  const file = path.join(path.dirname(defaultDbPath() ?? ""), ".window-state.json");
  const saved = existsSync(file) ? readFileSync(file) : null;
  return () => {
    if (saved) writeFileSync(file, saved);
    else rmSync(file, { force: true });
  };
}

// ── Scratch profile ───────────────────────────────────────────────────────

/** A throwaway app-data + WebView2 folder pair that one source's scenarios run in. */
class ScratchProfile {
  readonly root = mkdtempSync(path.join(tmpdir(), "luminous-perf-"));
  readonly dataDir = path.join(this.root, "data");
  readonly webviewDir = path.join(this.root, "webview");
  readonly dbPath = path.join(this.dataDir, "luminous.db");

  constructor(private readonly exe: string) {}

  async launch(window: { width: number; height: number }): Promise<void> {
    const existing = process.env.WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS ?? "";
    spawn(this.exe, [], {
      detached: true,
      stdio: "ignore",
      env: {
        ...process.env,
        LUMINOUS_DATA_DIR: this.dataDir,
        // Takes precedence over the folder Tauri asks WebView2 for, in every
        // release (it's WebView2's own override), so localStorage and the
        // browser caches start empty too.
        WEBVIEW2_USER_DATA_FOLDER: this.webviewDir,
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
        if (ready.value === true) {
          pinWindow(window.width, window.height);
          return;
        }
      } catch {
        // not up yet
      } finally {
        cdp.close();
      }
    }
    throw new Error(`Luminous didn't expose a ready page on CDP port ${CDP_PORT} within 60s.`);
  }

  /** Writes (or, for null, deletes) app_state keys. The app must be closed, or it overwrites them on exit. */
  writeAppState(keys: Record<string, string | null>) {
    if (isRunning()) throw new Error("Refusing to write app_state while Luminous is running.");
    const db = new Database(this.dbPath);
    try {
      db.exec("PRAGMA busy_timeout = 5000");
      const del = db.query("DELETE FROM app_state WHERE key = ?1");
      const set = db.query(
        "INSERT INTO app_state (key, value) VALUES (?1, ?2) ON CONFLICT(key) DO UPDATE SET value = excluded.value",
      );
      db.transaction(() => {
        for (const [k, v] of Object.entries(keys)) {
          if (v === null) del.run(k);
          else set.run(k, v);
        }
      })();
    } finally {
      db.close();
    }
  }

  findSong(where: string, ...params: (string | number)[]): BaselineTrack | null {
    const db = new Database(this.dbPath, { readonly: true });
    try {
      return db
        .query(`SELECT ${TRACK_COLUMNS} FROM songs WHERE ${where} AND unavailable = 0 AND cue_path IS NULL`)
        .get(...params) as BaselineTrack | null;
    } finally {
      db.close();
    }
  }

  /** Cues `song` at 0:00 for the next launch, on its own (playlist 0 with no ad-hoc list makes Player::new restore it as a one-item queue). */
  cue(song: BaselineTrack) {
    this.writeAppState({
      last_song_id: String(song.id),
      last_position_nanosec: "0",
      last_playlist_id: "0",
      last_item_uuid: null,
      last_adhoc_song_ids: null,
    });
  }

  dispose(keep: boolean) {
    if (keep) {
      log(`kept scratch profile: ${this.root}`);
      return;
    }
    // WebView2 can hold files for a moment after the app exits.
    rmSync(this.root, { recursive: true, force: true, maxRetries: 10, retryDelay: 500 });
  }
}

// The fixed view every measured launch opens to: Collection → Songs, nothing
// selected. +page.svelte restores the tab/sub-tab from these app_state keys at
// boot; a fresh WebView2 profile has no localStorage to override them.
const canonicalView = (subTab: "songs" | "albums") => ({ active_tab: "collection", active_sub_tab: subTab });

// First-run UI that would otherwise cover the measured view. walkthrough_completed
// is the legacy flag the tour still honours, and the only one older builds know.
const FIRST_RUN_DONE = { welcome_seen: "true", walkthrough_completed: "true" };

// An empty value records "no default library" as a deliberate choice, so the
// app never links a watched folder and writes its genre hierarchy sidecar
// into your music folder (hierarchy_sidecar::ensure_default).
const NO_DEFAULT_LIBRARY = { default_library_path: "" };

// ── Scenarios ─────────────────────────────────────────────────────────────

interface PlaybackState {
  state: "stopped" | "playing" | "paused";
  current_song: { id: number; title: string } | null;
  position_nanosec: number;
}
interface EqualizerConfig {
  enabled: boolean;
  [key: string]: unknown;
}

/** Shared by every source: builds rows, measures a scenario's settled median and peak. */
class Recorder {
  readonly rows: CsvRow[] = [];
  private readonly peak = new PeakTracker();

  constructor(private readonly opts: Options) {}

  /** Starts the background peak poll; call before the scenario's work begins. */
  begin() {
    this.peak.start();
  }

  /** Settles, samples, stops the peak poll and records the row. */
  async finish(label: string, settleSec: number, extra: Partial<CsvRow> = {}) {
    log(`${label}: settling ${settleSec}s`);
    await sleep(settleSec);
    const settled = await sampleMedian(this.opts.samples, this.opts.intervalSec, {
      onSample: (s) => {
        this.peak.record(s);
        console.log(`  ${formatSnapshot("sample", s)}`);
      },
    });
    const peak = (await this.peak.stop()) ?? settled;
    log(`${label}: peak working set ${peak.workingSetMb.toFixed(1)} MB, private bytes ${peak.privateBytesMb.toFixed(1)} MB`);
    this.rows.push(this.row(label, settled, peak, extra));
  }

  /** Stops a peak poll whose scenario failed, so it doesn't keep polling into the next one. */
  async abandon() {
    await this.peak.stop();
  }

  private row(label: string, s: Snapshot, peak: Snapshot, extra: Partial<CsvRow>): CsvRow {
    return {
      timestamp: new Date().toISOString(),
      label,
      app_version: this.opts.appVersion,
      commit: appCommit(this.opts.exeRepo),
      os: osLabel(),
      library_tracks: "",
      window: this.opts.window,
      process_count: s.processCount,
      working_set_mb: s.workingSetMb.toFixed(1),
      private_bytes_mb: s.privateBytesMb.toFixed(1),
      samples: this.opts.samples,
      scan_seconds: "",
      profile: "scratch",
      peak_working_set_mb: peak.workingSetMb.toFixed(1),
      peak_private_bytes_mb: peak.privateBytesMb.toFixed(1),
      ...extra,
    };
  }
}

const libraryTracks = (cdp: CdpClient) =>
  invoke<{ total_songs: number }>(cdp, "get_library_stats").then((s) => s.total_songs);

/**
 * Plays the cued song from 0:00 with EQ + analyzer on for the playback
 * scenario's length, then pauses. The caller checked the length against the
 * song's halfway mark, where a listen gets recorded (Player::on_position_update).
 */
async function playbackScenario(rec: Recorder, label: string, song: BaselineTrack, tracks: number) {
  rec.begin();
  try {
    await withCdp(async (c) => {
      const cued = await invoke<PlaybackState>(c, "get_playback_state");
      if (cued.current_song?.id !== song.id || cued.position_nanosec > 1e9) {
        throw new Error(
          `Expected "${song.title}" cued at 0:00, but the player has "${cued.current_song?.title ?? "nothing"}" ` +
            `at ${(cued.position_nanosec / 1e9).toFixed(1)}s.`,
        );
      }
      const eq = await invoke<EqualizerConfig>(c, "get_equalizer_state");
      if (!eq.enabled) await invoke(c, "apply_equalizer_config", { config: { ...eq, enabled: true } });
      await invoke(c, "resume");
      log(`playing "${song.title}" from 0:00`);
    });
    await rec.finish(label, SETTLE_PLAYBACK_SEC, { library_tracks: tracks });
  } catch (e) {
    await rec.abandon();
    throw e;
  }
  const played = await withCdp(async (c) => {
    await invoke(c, "pause");
    return invoke<PlaybackState>(c, "get_playback_state");
  });
  if (played.current_song?.id !== song.id) {
    rec.rows.pop();
    throw new Error(`Playback moved off the baseline track (to "${played.current_song?.title}") — not recording ${label}.`);
  }
}

/**
 * Scrolls the page's tallest scroll container (the Albums grid) to the bottom
 * a viewport at a time, so every album card mounts and requests its cover —
 * the first request for each generates its thumbnail (covermanager's
 * serve_art_request). Waits for the images on screen at each step to finish,
 * so the scroll keeps pace with thumbnailing rather than racing past it.
 */
const SCROLL_ALBUM_GRID = `async function () {
  const scrollers = [...document.querySelectorAll("*")].filter((el) => {
    const style = getComputedStyle(el);
    return /(auto|scroll)/.test(style.overflowY) && el.scrollHeight > el.clientHeight + 10;
  });
  const grid = scrollers.sort((a, b) => b.scrollHeight - a.scrollHeight)[0];
  if (!grid) throw new Error("No scrollable Albums grid found");
  // Only images in view: lazy-loaded ones below the fold stay incomplete until scrolled to.
  const settleImages = async () => {
    for (let i = 0; i < 50; i++) {
      const view = grid.getBoundingClientRect();
      const pending = [...grid.querySelectorAll("img")].filter((img) => {
        const r = img.getBoundingClientRect();
        return !img.complete && r.bottom > view.top && r.top < view.bottom;
      });
      if (!pending.length) return;
      await new Promise((r) => setTimeout(r, 100));
    }
  };
  let steps = 0;
  while (grid.scrollTop + grid.clientHeight < grid.scrollHeight - 2) {
    grid.scrollTop += grid.clientHeight * 0.8;
    steps++;
    await new Promise((r) => setTimeout(r, 150));
    await settleImages();
  }
  return { steps, cards: grid.querySelectorAll("img").length };
}`;

async function localSource(opts: Options, rec: Recorder, folders: string[]) {
  const profile = new ScratchProfile(opts.exe);
  log(`local: scratch profile ${profile.root}`);
  try {
    // Setup launch: just creates the database, so the keys below can go in before anything runs.
    await profile.launch(opts);
    await closeGracefully();
    profile.writeAppState({ ...FIRST_RUN_DONE, ...NO_DEFAULT_LIBRARY, ...canonicalView("songs") });

    // 1. First scan, with art extraction — the same calls as adding folders in Settings → Library.
    await profile.launch(opts);
    rec.begin();
    let tracks: number;
    try {
      const scan = await withCdp((c) =>
        timed(async () => {
          for (const dir of folders) await invoke(c, "add_directory", { path: dir });
          await invoke(c, "scan_directories", { force: false });
        }),
      );
      tracks = await withCdp(libraryTracks);
      log(`initial scan of ${tracks} tracks in ${folders.length} folder(s) took ${scan.seconds}s`);
      await rec.finish("initial-scan", SETTLE_AFTER_SCAN_SEC, { library_tracks: tracks, scan_seconds: scan.seconds });
    } catch (e) {
      await rec.abandon();
      throw e;
    }
    await closeGracefully();

    const song = profile.findSong("path = ?1 COLLATE NOCASE", path.normalize(opts.track));
    if (!song) throw new Error(`The scan didn't pick up --track "${opts.track}"; is it under ${folders.join(", ")}?`);
    profile.cue(song);

    // 2. Idle, in a fresh launch over the already-scanned library.
    await profile.launch(opts);
    rec.begin();
    const visualizerMounted = await withCdp((c) => c.eval("document.querySelectorAll('canvas').length > 0"));
    if (visualizerMounted.value !== true) {
      log("warning: no visualizer canvas found — the playback scenario won't exercise the analyzer's rendering.");
    }
    await rec.finish("idle", SETTLE_IDLE_SEC, { library_tracks: tracks });

    // 3. Forced full scan — same call as Settings → Force Full Scan.
    rec.begin();
    try {
      const scan = await withCdp((c) => timed(() => invoke(c, "scan_directories", { force: true })));
      log(`forced full scan took ${scan.seconds}s`);
      await rec.finish("after-full-scan", SETTLE_AFTER_SCAN_SEC, { library_tracks: tracks, scan_seconds: scan.seconds });
    } catch (e) {
      await rec.abandon();
      throw e;
    }

    // 4. Playback with EQ + analyzer on.
    await playbackScenario(rec, "playback-eq-analyzer", song, tracks);
    await closeGracefully();

    // 5. Album grid: a fresh launch into Collection → Albums, scrolled end to end.
    profile.writeAppState(canonicalView("albums"));
    await profile.launch(opts);
    rec.begin();
    try {
      const scroll = await withCdp((c) => timed(() => callInPage<{ steps: number; cards: number }>(c, SCROLL_ALBUM_GRID)));
      log(`scrolled the Albums grid in ${scroll.result.steps} steps (${scroll.seconds}s)`);
      await rec.finish("album-grid", SETTLE_AFTER_SCAN_SEC, { library_tracks: tracks });
    } catch (e) {
      await rec.abandon();
      throw e;
    }
    await closeGracefully();
  } finally {
    if (isRunning()) await closeGracefully();
    profile.dispose(opts.keepProfiles);
  }
}

interface RemoteServer {
  label: "webdav" | "subsonic";
  save: string;
  sync: string;
  input: Record<string, unknown>;
}

function remoteServer(source: "webdav" | "subsonic"): RemoteServer {
  const env = (name: string) => process.env[`LUMINOUS_PERF_${source.toUpperCase()}_${name}`];
  const url = env("URL");
  if (!url) throw new Error(`--sources ${source} needs LUMINOUS_PERF_${source.toUpperCase()}_URL (see the script header).`);
  // Auto-sync off: a scheduled sync mid-scenario would land in the readings.
  const common = { id: null, name: `perf ${source}`, url, enabled: true, autoSyncEnabled: false };
  if (source === "webdav") {
    return {
      label: source,
      save: "save_webdav_server",
      sync: "sync_webdav_server",
      input: { ...common, username: env("USER") ?? null, password: env("PASSWORD") ?? null, remotePath: env("PATH") ?? null },
    };
  }
  return {
    label: source,
    save: "save_subsonic_server",
    sync: "sync_subsonic_server",
    input: { ...common, username: env("USER") ?? "", password: env("PASSWORD") ?? null, reportPlays: false },
  };
}

async function remoteSource(opts: Options, rec: Recorder, baseline: BaselineTrack, server: RemoteServer) {
  const { label } = server;
  const profile = new ScratchProfile(opts.exe);
  log(`${label}: scratch profile ${profile.root}`);
  try {
    await profile.launch(opts);
    await closeGracefully();
    profile.writeAppState({ ...FIRST_RUN_DONE, ...NO_DEFAULT_LIBRARY, ...canonicalView("songs") });

    // 1. First sync — the same calls as adding the server in Settings → Sources and Sync Now.
    await profile.launch(opts);
    const saved = await withCdp(async (c) => {
      try {
        return await invoke<{ id: number }>(c, server.save, { input: server.input });
      } catch (e) {
        if (/not found|unknown command/i.test(String(e))) return null;
        throw e;
      }
    });
    if (!saved) {
      log(`${label}: this build has no ${server.save} command — skipping ${label}.`);
      return;
    }
    rec.begin();
    let tracks: number;
    try {
      const sync = await withCdp((c) => timed(() => invoke(c, server.sync, { id: saved.id })));
      tracks = await withCdp(libraryTracks);
      log(`${label} sync of ${tracks} tracks took ${sync.seconds}s`);
      await rec.finish(`${label}-sync`, SETTLE_AFTER_SCAN_SEC, { library_tracks: tracks, scan_seconds: sync.seconds });
    } catch (e) {
      await rec.abandon();
      throw e;
    }
    await closeGracefully();

    // The remote copy of the baseline track: same title and album, within a second of its length.
    const song = profile.findSong(
      "title = ?1 COLLATE NOCASE AND album = ?2 COLLATE NOCASE AND ABS(length_nanosec - ?3) < 1000000000",
      baseline.title,
      baseline.album,
      baseline.length_nanosec,
    );
    if (!song) throw new Error(`${label}: the server has no copy of "${baseline.title}" (${baseline.album}).`);
    log(`${label} baseline track: ${describeTrack(song)}`);
    profile.cue(song);

    // 2. Idle, then 3. streamed playback, in a fresh launch.
    await profile.launch(opts);
    rec.begin();
    await rec.finish(`${label}-idle`, SETTLE_IDLE_SEC, { library_tracks: tracks });
    await playbackScenario(rec, `${label}-playback`, song, tracks);
    await closeGracefully();
  } finally {
    if (isRunning()) await closeGracefully();
    profile.dispose(opts.keepProfiles);
  }
}

// ── Run ───────────────────────────────────────────────────────────────────

function checkPreconditions(opts: Options) {
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
    execFileSync("git", ["log", "-1", "--format=%ct", "--", ...APP_SOURCE_PATHS], { cwd: opts.exeRepo, encoding: "utf8" }).trim(),
  );
  if (statSync(opts.exe).mtimeMs / 1000 < lastSourceChange) {
    throw new Error(`${opts.exe} is older than the last app-source commit — rebuild it (bun run tauri build --no-bundle).`);
  }
  if (appCommit(opts.exeRepo).endsWith("-dirty")) {
    log("warning: app source has uncommitted changes; the commit column will be marked -dirty.");
  }
}

async function main() {
  const opts = parseArgs();
  checkPreconditions(opts);
  const real = readRealProfile(opts.track);
  const folders = opts.libraries.length ? opts.libraries : real.folders;
  if (opts.sources.includes("local") && !folders.length) {
    throw new Error("Your profile has no watched folders; pass --library <dir>.");
  }
  // Fail on missing server settings now, not after the local scenarios have run.
  const remotes = opts.sources.filter((s): s is "webdav" | "subsonic" => s !== "local").map(remoteServer);

  const baseline = real.track;
  // The playback scenario must stop short of the halfway mark, where a listen
  // gets recorded to play stats (and scrobbled) — see Player::on_position_update.
  const playSeconds = SETTLE_PLAYBACK_SEC + opts.samples * (opts.intervalSec + 2) + 10;
  if (baseline.length_nanosec / 2e9 <= playSeconds) {
    throw new Error(
      `${describeTrack(baseline)} is too short: the playback scenario plays ~${playSeconds}s, which would pass its halfway ` +
        `mark and record a play. Pick a track longer than ${Math.ceil((playSeconds * 2) / 60)} minutes.`,
    );
  }
  log(`baseline track: ${describeTrack(baseline)}`);
  if (opts.sources.includes("local")) log(`local library: ${folders.join(", ")}`);

  const rec = new Recorder(opts);
  const restoreWindowState = protectRealWindowState();
  try {
    if (opts.sources.includes("local")) await localSource(opts, rec, folders);
    for (const server of remotes) await remoteSource(opts, rec, baseline, server);
  } finally {
    restoreWindowState();
    // Rows from scenarios that finished are kept even when a later one fails.
    for (const r of rec.rows) {
      if (opts.dryRun) console.log(csvLine(r));
      else appendCsvRow(opts.csv, r);
    }
    log(opts.dryRun ? "dry run — nothing written" : `appended ${rec.rows.length} rows to ${opts.csv}`);
  }
  log("compare with: python scripts/perf-chart.py --baseline <ver> --candidate " + opts.appVersion);
}

main().catch((err) => {
  console.error(`[perf] ${err instanceof Error ? err.message : err}`);
  process.exit(1);
});
