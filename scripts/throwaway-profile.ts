#!/usr/bin/env bun
/**
 * Shared throwaway-profile launcher for app scripts (benchmarks, screenshots,
 * tutorials, and motion graphics).
 *
 * Runs the real Luminous executable inside an isolated temporary directory pair
 * (`LUMINOUS_DATA_DIR` and `WEBVIEW2_USER_DATA_FOLDER`) with WebView2's Chrome
 * DevTools Protocol (CDP) port open (port 9222).
 *
 * This ensures:
 * 1. The test or benchmark instance never reads or writes the user's real database.
 * 2. `localStorage` and browser caches start clean.
 * 3. The process can be driven or inspected over CDP without touching the user's live app.
 * 4. The profile can be cleanly disposed of (or preserved with `keepProfiles`).
 */

import { Database } from "bun:sqlite";
import { execFileSync, spawn } from "node:child_process";
import { existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { defaultDbPath } from "./mock-library";
import { CdpClient } from "./monitor-cdp";

const REPO_ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
export const CDP_PORT = 9222;

export const TRACK_COLUMNS = "id, title, album, filetype, samplerate, bitdepth, bitrate, length_nanosec";

/** First-run UI flags that would otherwise cover the measured or captured view. */
export const FIRST_RUN_DONE = { welcome_seen: "true", walkthrough_completed: "true" };

/**
 * An empty value records "no default library" as a deliberate choice, so the
 * app never links a watched folder and writes its genre hierarchy sidecar
 * into a music folder (hierarchy_sidecar::ensure_default).
 */
export const NO_DEFAULT_LIBRARY = { default_library_path: "" };

/**
 * The fixed view measured launches and scene scripts open to:
 * Collection → Songs (or Albums), with nothing selected.
 */
export const canonicalView = (subTab: "songs" | "albums") => ({
  active_tab: "collection",
  active_sub_tab: subTab,
});

/**
 * Sensible baseline app_state for throwaway profiles:
 * - Suppresses first-run welcome and walkthrough tour.
 * - Disables default library auto-linking (prevents hierarchy sidecars in music folders).
 * - Opens to Collection → Songs view with nothing selected.
 */
export const DEFAULT_APP_STATE: Record<string, string> = {
  ...FIRST_RUN_DONE,
  ...NO_DEFAULT_LIBRARY,
  ...canonicalView("songs"),
};

export interface SongRecord {
  id: number;
  title: string;
  album: string;
  filetype: number;
  samplerate: number;
  bitdepth: number | null;
  bitrate: number;
  length_nanosec: number;
}

export interface WindowDimensions {
  width: number;
  height: number;
}

export interface ProfileOptions {
  /** Path to the Luminous executable. Defaults to target/release/LuminousMusicPlayer.exe. */
  exe?: string;
  /** Remote debugging port for CDP. Defaults to 9222. */
  port?: number;
  /** Pinned outer window size in physical pixels. */
  window?: WindowDimensions;
  /** Library folders to add and scan upon start. */
  libraryFolders?: string[];
  /** App state keys to pre-seed into SQLite before the first launch. Overrides DEFAULT_APP_STATE. */
  appState?: Record<string, string | null>;
  /** If true, keeps temporary profile folders on disk after disposal. */
  keepProfiles?: boolean;
}

export interface LaunchOptions {
  /** Window dimensions override for this launch. Defaults to ProfileOptions.window. */
  window?: WindowDimensions;
  /** Maximum seconds to wait for CDP readiness. Defaults to 60. */
  timeoutSec?: number;
}

const sleep = (sec: number) => new Promise((r) => setTimeout(r, sec * 1000));

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
  if (process.platform !== "win32") return "";
  return execFileSync("powershell", ["-NoProfile", "-Command", script], { encoding: "utf8" }).trim();
}

/** Pins the app's main window to position (100, 100) with the given dimensions. */
export function pinWindow(w: number, h: number): void {
  if (process.platform !== "win32") return;
  ps(`${PIN_WINDOW}[LumPerf.Win]::SetWindowPos($main.MainWindowHandle, [IntPtr]::Zero, 100, 100, ${w}, ${h}, 0x14) | Out-Null`);
}

/** Checks whether a Luminous process is currently running on the system. */
export function isRunning(): boolean {
  if (process.platform !== "win32") return false;
  return ps("@(Get-Process -Name LuminousMusicPlayer -ErrorAction SilentlyContinue).Count") !== "0";
}

/** Force-kills any running LuminousMusicPlayer processes. */
export function killProcess(): void {
  if (process.platform !== "win32") return;
  ps("Get-Process -Name LuminousMusicPlayer -ErrorAction SilentlyContinue | Stop-Process -Force");
}

/** Closes via WM_CLOSE (not a kill) so the app saves its state on the way out. */
export async function closeGracefully(timeoutSec = 30): Promise<void> {
  if (process.platform !== "win32") return;
  if (!isRunning()) return;
  ps("Get-Process -Name LuminousMusicPlayer -ErrorAction SilentlyContinue | ForEach-Object { $_.CloseMainWindow() | Out-Null }");
  for (let i = 0; i < timeoutSec; i++) {
    if (!isRunning()) return;
    await sleep(1);
  }
  throw new Error(`Luminous didn't exit within ${timeoutSec}s of being asked to close; close it by hand.`);
}

/**
 * Builds older than #1197 save window placement in the real profile's config
 * folder even under LUMINOUS_DATA_DIR (newer ones keep it in the scratch
 * profile), so the real file is snapshotted up front and put back after.
 * Returns the restore function.
 */
export function protectRealWindowState(): () => void {
  const dbPath = defaultDbPath();
  const file = path.join(path.dirname(dbPath ?? ""), ".window-state.json");
  const saved = existsSync(file) ? readFileSync(file) : null;
  return () => {
    if (saved) writeFileSync(file, saved);
    else rmSync(file, { force: true });
  };
}

/** Validates that environment preconditions for driving Luminous are met. */
export function checkPreconditions(exe: string): void {
  if (process.platform !== "win32") {
    throw new Error("This runner is Windows-only (it needs WebView2's CDP port). See docs/PERFORMANCE.md for Linux steps.");
  }
  if (isRunning()) {
    throw new Error("Luminous is already running. Close it first — launching a second instance would hand off to yours.");
  }
  if (!existsSync(exe)) {
    throw new Error(`The release build wasn't found at ${exe}. Build it first: bun run tauri build --no-bundle (or pass --exe).`);
  }
}

// ── IPC over CDP helpers (used for library scanning during setup) ──────────

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

// ── Throwaway AppProfile ──────────────────────────────────────────────────

/**
 * An isolated throwaway Luminous environment with dedicated temp AppData
 * and WebView2 user data directories.
 */
export class AppProfile {
  readonly root: string;
  readonly dataDir: string;
  readonly webviewDir: string;
  readonly dbPath: string;
  readonly exe: string;
  readonly port: number;
  private readonly defaultWindow?: WindowDimensions;
  private isDisposed = false;
  private hasLaunched = false;

  constructor(options?: ProfileOptions) {
    this.root = mkdtempSync(path.join(tmpdir(), "luminous-profile-"));
    this.dataDir = path.join(this.root, "data");
    this.webviewDir = path.join(this.root, "webview");
    this.dbPath = path.join(this.dataDir, "luminous.db");

    mkdirSync(this.dataDir, { recursive: true });
    mkdirSync(this.webviewDir, { recursive: true });

    this.exe = options?.exe ?? path.join(REPO_ROOT, "target", "release", "LuminousMusicPlayer.exe");
    this.port = options?.port ?? CDP_PORT;
    this.defaultWindow = options?.window;

    // Pull defaults downward: suppress first-run popups, set canonical songs view, and apply caller overrides
    this.writeAppState({
      ...DEFAULT_APP_STATE,
      ...(options?.appState ?? {}),
    });
  }

  /** Whether Luminous is currently running. */
  isRunning(): boolean {
    return isRunning();
  }

  /**
   * Spawns the app process with isolated environment variables and waits
   * until CDP is responding and the page is fully loaded.
   */
  async launch(options?: LaunchOptions): Promise<void> {
    if (this.isDisposed) throw new Error("Cannot launch a disposed AppProfile.");
    checkPreconditions(this.exe);
    this.hasLaunched = true;

    const existing = process.env.WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS ?? "";
    spawn(this.exe, [], {
      detached: true,
      stdio: "ignore",
      env: {
        ...process.env,
        LUMINOUS_DATA_DIR: this.dataDir,
        WEBVIEW2_USER_DATA_FOLDER: this.webviewDir,
        WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: `${existing} --remote-debugging-port=${this.port}`.trim(),
      },
    }).unref();

    const timeoutSec = options?.timeoutSec ?? 60;
    const win = options?.window ?? this.defaultWindow;

    for (let i = 0; i < timeoutSec; i++) {
      await sleep(1);
      const cdp = new CdpClient();
      try {
        await cdp.connect(this.port);
        const ready = await cdp.eval("document.readyState === 'complete' && !!window.__TAURI_INTERNALS__");
        if (ready.value === true) {
          if (win) pinWindow(win.width, win.height);
          return;
        }
      } catch {
        // App or devtools port not up yet
      } finally {
        cdp.close();
      }
    }
    throw new Error(`Luminous didn't expose a ready page on CDP port ${this.port} within ${timeoutSec}s.`);
  }

  /**
   * Gracefully asks the app to close via WM_CLOSE and waits for process exit.
   * If the app is already stopped, this is a safe no-op.
   */
  async close(timeoutSec = 30): Promise<void> {
    if (!this.hasLaunched || !this.isRunning()) return;
    await closeGracefully(timeoutSec);
    this.hasLaunched = false;
  }

  /**
   * Adds and scans the specified library folders over CDP.
   * The app must be running. Awaits scan completion before returning.
   */
  async scanLibrary(folders: string[]): Promise<void> {
    if (!this.isRunning()) throw new Error("Cannot scan library while Luminous is stopped.");
    const cdp = new CdpClient();
    await cdp.connect(this.port);
    try {
      for (const dir of folders) {
        await invoke(cdp, "add_directory", { path: dir });
      }
      await invoke(cdp, "scan_directories", { force: false });
    } finally {
      cdp.close();
    }
  }

  /** Writes (or deletes, if value is null) app_state keys in the profile DB. */
  writeAppState(keys: Record<string, string | null>): void {
    if (this.hasLaunched && this.isRunning()) throw new Error("Refusing to write app_state while Luminous is running.");

    const db = new Database(this.dbPath);
    try {
      db.exec("PRAGMA busy_timeout = 5000");
      db.exec("CREATE TABLE IF NOT EXISTS app_state (key TEXT PRIMARY KEY, value TEXT NOT NULL)");
      const stmtDelete = db.query("DELETE FROM app_state WHERE key = ?1");
      const stmtUpsert = db.query(
        "INSERT INTO app_state (key, value) VALUES (?1, ?2) ON CONFLICT(key) DO UPDATE SET value = excluded.value",
      );
      db.transaction(() => {
        for (const [k, v] of Object.entries(keys)) {
          if (v === null) stmtDelete.run(k);
          else stmtUpsert.run(k, v);
        }
      })();
    } finally {
      db.close();
    }
  }

  /** Finds a single playable song in the profile DB matching the SQL WHERE clause. */
  findPlayableSong(where: string, ...params: (string | number)[]): SongRecord | null {
    if (!existsSync(this.dbPath)) return null;
    const db = new Database(this.dbPath, { readonly: true });
    try {
      return db
        .query(`SELECT ${TRACK_COLUMNS} FROM songs WHERE ${where} AND unavailable = 0 AND cue_path IS NULL`)
        .get(...params) as SongRecord | null;
    } finally {
      db.close();
    }
  }

  /**
   * Finds a single playable song in the profile DB matching the SQL WHERE clause.
   * Alias for findPlayableSong.
   */
  findSong(where: string, ...params: (string | number)[]): SongRecord | null {
    return this.findPlayableSong(where, ...params);
  }

  /** Cues `song` at 0:00 for the next launch as a one-item queue. */
  cue(song: SongRecord): void {
    this.writeAppState({
      last_song_id: String(song.id),
      last_position_nanosec: "0",
      last_playlist_id: "0",
      last_item_uuid: null,
      last_adhoc_song_ids: null,
    });
  }

  /**
   * Closes any running instance and deletes the temporary profile folder.
   * Calling dispose multiple times is an idempotent no-op.
   */
  async dispose(keep = false): Promise<void> {
    if (this.isDisposed) return;
    this.isDisposed = true;

    if (this.hasLaunched && this.isRunning()) {
      try {
        await this.close(10);
      } catch {
        killProcess();
      }
    }

    if (keep) {
      console.log(`[throwaway-profile] kept profile: ${this.root}`);
      return;
    }

    if (existsSync(this.root)) {
      rmSync(this.root, { recursive: true, force: true, maxRetries: 10, retryDelay: 500 });
    }
  }
}

/**
 * Creates, launches, and optionally seeds/scans a throwaway profile in one call.
 */
export async function startProfile(options?: ProfileOptions): Promise<AppProfile> {
  const profile = new AppProfile(options);
  try {
    await profile.launch({ window: options?.window });

    if (options?.libraryFolders && options.libraryFolders.length > 0) {
      await profile.scanLibrary(options.libraryFolders);
    }

    return profile;
  } catch (err) {
    await profile.dispose(options?.keepProfiles ?? false);
    throw err;
  }
}

/**
 * Runs a scoped callback with a temporary profile, guaranteeing disposal on completion or error.
 */
export async function withProfile<T>(
  options: ProfileOptions | undefined,
  action: (profile: AppProfile) => Promise<T>,
): Promise<T> {
  const profile = await startProfile(options);
  try {
    return await action(profile);
  } finally {
    await profile.dispose(options?.keepProfiles ?? false);
  }
}
