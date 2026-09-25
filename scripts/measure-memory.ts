#!/usr/bin/env bun
/**
 * Snapshots Luminous's total memory footprint (main process + all child
 * processes, e.g. WebView2 renderer/GPU processes on Windows) for the
 * baseline in docs/PERFORMANCE.md.
 *
 * For a release-to-release comparison on Windows, prefer
 * scripts/perf-memory-scenarios.ts, which drives every scenario the same way
 * each run and calls into this module. Use this CLI directly for Linux, or for
 * ad-hoc before/after checks while working on a change.
 *
 * Usage:
 *   bun run scripts/measure-memory.ts --label idle
 *   bun run scripts/measure-memory.ts --label idle --samples 5 --csv docs/performance-history.csv
 *   bun run scripts/measure-memory.ts --watch --interval 5
 *
 * Options:
 *   --samples <n>         take n readings --interval seconds apart and report the median (default 1)
 *   --app-version <ver>   version recorded in the CSV (default: package.json's; pass the upcoming
 *                         release's version when measuring before the version bump)
 *   --tracks <n>          library size recorded in the CSV
 *   --window <WxH>        window size recorded in the CSV, if you pinned one
 *   --allow-hidden        measure even when the main window is minimized/hidden (Windows); by
 *                         default this refuses, since a hidden WebView2 reads ~100MB lower
 */

import { execFileSync } from "node:child_process";
import { appendFileSync, existsSync, readFileSync, writeFileSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const BINARY_NAME = "LuminousMusicPlayer";
const REPO_ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");

export const CSV_COLUMNS = [
  "timestamp",
  "label",
  "app_version",
  "commit",
  "os",
  "library_tracks",
  "window",
  "process_count",
  "working_set_mb",
  "private_bytes_mb",
  "samples",
  "scan_seconds",
] as const;

export type CsvRow = Record<(typeof CSV_COLUMNS)[number], string | number>;

export type WindowState = "visible" | "minimized" | "hidden" | "unknown";

export interface Snapshot {
  processCount: number;
  workingSetMb: number;
  privateBytesMb: number;
  windowState: WindowState;
}

function parseArgs() {
  const args = process.argv.slice(2);
  const get = (flag: string) => {
    const i = args.indexOf(flag);
    return i !== -1 ? args[i + 1] : undefined;
  };
  return {
    label: get("--label") ?? "",
    csv: get("--csv"),
    watch: args.includes("--watch"),
    intervalSec: Number(get("--interval") ?? "5"),
    samples: Number(get("--samples") ?? "1"),
    appVersion: get("--app-version") ?? packageVersion(),
    tracks: get("--tracks") ?? "",
    window: get("--window") ?? "",
    allowHidden: args.includes("--allow-hidden"),
  };
}

function snapshotWindows(): Snapshot {
  // Sum WorkingSet64/PrivateMemorySize64 across the main exe and every
  // descendant process (WebView2 renderer/GPU/crashpad, etc.) so the total
  // matches what a user perceives as "Luminous's memory usage", not just
  // the thin main process. Also reports whether the main window is visible:
  // a minimized/hidden WebView2 holds far less memory, so a reading taken
  // that way isn't comparable to one taken with the window on screen.
  const script = `
$ErrorActionPreference = 'Stop'
$main = Get-Process -Name '${BINARY_NAME}' -ErrorAction SilentlyContinue | Select-Object -First 1
if (-not $main) { Write-Output 'NOTFOUND'; exit 0 }
Add-Type -Namespace LumMem -Name User32 -MemberDefinition @'
[DllImport("user32.dll")] public static extern bool IsIconic(IntPtr h);
[DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr h);
'@
$hwnd = $main.MainWindowHandle
$winState = if ($hwnd -eq 0) { 'hidden' } elseif ([LumMem.User32]::IsIconic($hwnd)) { 'minimized' } elseif (-not [LumMem.User32]::IsWindowVisible($hwnd)) { 'hidden' } else { 'visible' }
$all = Get-CimInstance Win32_Process | Select-Object ProcessId, ParentProcessId
$pids = New-Object System.Collections.Generic.HashSet[int]
$pids.Add($main.Id) | Out-Null
$frontier = @($main.Id)
while ($frontier.Count -gt 0) {
  $next = @()
  foreach ($p in $frontier) {
    foreach ($child in ($all | Where-Object { $_.ParentProcessId -eq $p })) {
      if ($pids.Add([int]$child.ProcessId)) { $next += [int]$child.ProcessId }
    }
  }
  $frontier = $next
}
$ws = 0; $priv = 0; $count = 0
foreach ($procId in $pids) {
  $proc = Get-Process -Id $procId -ErrorAction SilentlyContinue
  if ($proc) {
    $ws += $proc.WorkingSet64
    $priv += $proc.PrivateMemorySize64
    $count++
  }
}
Write-Output "$count,$ws,$priv,$winState"
`;
  const out = execFileSync("powershell", ["-NoProfile", "-Command", script], {
    encoding: "utf8",
  }).trim();

  if (out === "NOTFOUND" || out === "") {
    throw new Error(
      `Process '${BINARY_NAME}' not found. Is Luminous running? (Task Manager shows the exe as "${BINARY_NAME}.exe")`,
    );
  }
  const [count, ws, priv, winState] = out.split(",");
  return {
    processCount: Number(count),
    workingSetMb: Number(ws) / 1024 / 1024,
    privateBytesMb: Number(priv) / 1024 / 1024,
    windowState: winState as WindowState,
  };
}

function snapshotLinux(): Snapshot {
  // Sum RSS (proxy for working set) and Pss/Private (proxy for private
  // bytes, via /proc/<pid>/smaps_rollup) across the main binary and any
  // child processes (e.g. a WebKitGTK web process).
  // Anchor to a path/start boundary so this doesn't match an unrelated
  // process that merely has the binary name as a substring elsewhere in
  // its argv (e.g. a shell command referencing the binary name in a string).
  const pidsOut = execFileSync("pgrep", ["-f", `(^|/)${BINARY_NAME}(\\s|$)`], {
    encoding: "utf8",
  }).trim();
  if (!pidsOut) {
    throw new Error(`Process matching '${BINARY_NAME}' not found. Is Luminous running?`);
  }
  const rootPids = pidsOut.split("\n").map(Number);
  const allPids = new Set<number>(rootPids);
  let frontier = rootPids;
  while (frontier.length > 0) {
    const next: number[] = [];
    for (const p of frontier) {
      let children: string;
      try {
        children = execFileSync("pgrep", ["-P", String(p)], { encoding: "utf8" }).trim();
      } catch {
        continue;
      }
      if (!children) continue;
      for (const c of children.split("\n").map(Number)) {
        if (!allPids.has(c)) {
          allPids.add(c);
          next.push(c);
        }
      }
    }
    frontier = next;
  }

  let rssKb = 0;
  let privateKb = 0;
  let count = 0;
  for (const pid of allPids) {
    try {
      const status = readFileSync(`/proc/${pid}/status`, { encoding: "utf8" });
      const rssMatch = status.match(/^VmRSS:\s+(\d+)/m);
      if (rssMatch) rssKb += Number(rssMatch[1]);

      try {
        const rollup = readFileSync(`/proc/${pid}/smaps_rollup`, { encoding: "utf8" });
        const clean = rollup.match(/^Private_Clean:\s+(\d+)/m);
        const dirty = rollup.match(/^Private_Dirty:\s+(\d+)/m);
        privateKb += Number(clean?.[1] ?? 0) + Number(dirty?.[1] ?? 0);
      } catch {
        // smaps_rollup unavailable (older kernel); fall back to RSS as the
        // private-bytes proxy for this process.
        if (rssMatch) privateKb += Number(rssMatch[1]);
      }
      count++;
    } catch {
      // process exited between listing and reading; skip it
    }
  }

  return {
    processCount: count,
    workingSetMb: rssKb / 1024,
    privateBytesMb: privateKb / 1024,
    // No portable way to ask a Wayland/X11 compositor whether the window is
    // minimized — keep it on screen by hand when measuring on Linux.
    windowState: "unknown",
  };
}

export function snapshot(): Snapshot {
  if (process.platform === "win32") return snapshotWindows();
  if (process.platform === "linux") return snapshotLinux();
  throw new Error(`Unsupported platform: ${process.platform} (this script covers Windows and Linux)`);
}

function median(values: number[]): number {
  const sorted = [...values].sort((a, b) => a - b);
  const mid = Math.floor(sorted.length / 2);
  return sorted.length % 2 ? sorted[mid] : (sorted[mid - 1] + sorted[mid]) / 2;
}

/**
 * Takes `count` snapshots `intervalSec` apart and returns the per-metric
 * median, so one GC/allocation spike doesn't become the recorded figure.
 * Throws if the window isn't visible on any sample, unless `allowHidden`.
 */
export async function sampleMedian(
  count: number,
  intervalSec: number,
  { allowHidden = false, onSample }: { allowHidden?: boolean; onSample?: (s: Snapshot) => void } = {},
): Promise<Snapshot> {
  const samples: Snapshot[] = [];
  for (let i = 0; i < count; i++) {
    if (i > 0) await new Promise((r) => setTimeout(r, intervalSec * 1000));
    const s = snapshot();
    assertVisible(s, allowHidden);
    onSample?.(s);
    samples.push(s);
  }
  return {
    processCount: samples[samples.length - 1].processCount,
    workingSetMb: median(samples.map((s) => s.workingSetMb)),
    privateBytesMb: median(samples.map((s) => s.privateBytesMb)),
    windowState: samples[samples.length - 1].windowState,
  };
}

export function assertVisible(s: Snapshot, allowHidden: boolean) {
  if (allowHidden || s.windowState === "visible" || s.windowState === "unknown") return;
  throw new Error(
    `Luminous's window is ${s.windowState}. A hidden WebView2 reads ~100MB lower than a visible one, ` +
      `so this snapshot wouldn't be comparable to the baseline. Restore the window (and let it settle ` +
      `for a minute) first, or pass --allow-hidden if that's deliberate.`,
  );
}

export function formatSnapshot(label: string, s: Snapshot, timestamp = new Date().toISOString()): string {
  return `[${timestamp}]${label ? ` ${label}:` : ""} ${s.processCount} process(es), working set ${s.workingSetMb.toFixed(1)} MB, private bytes ${s.privateBytesMb.toFixed(1)} MB (window ${s.windowState})`;
}

export function osLabel(): string {
  return process.platform === "win32" ? "windows" : process.platform;
}

export function packageVersion(): string {
  return JSON.parse(readFileSync(path.join(REPO_ROOT, "package.json"), "utf8")).version;
}

/** Paths whose changes affect the built app — used to tie a measurement to the code it measured. */
export const APP_SOURCE_PATHS = ["src", "src-tauri", "bun.lock"];

/**
 * Short hash of the last commit that touched the app's own source (not
 * docs/scripts), suffixed `-dirty` when those paths have uncommitted changes.
 */
export function appCommit(): string {
  try {
    const hash = execFileSync("git", ["log", "-1", "--format=%h", "--", ...APP_SOURCE_PATHS], {
      cwd: REPO_ROOT,
      encoding: "utf8",
    }).trim();
    const dirty = execFileSync("git", ["status", "--porcelain", "--", ...APP_SOURCE_PATHS], {
      cwd: REPO_ROOT,
      encoding: "utf8",
    }).trim();
    return dirty ? `${hash}-dirty` : hash;
  } catch {
    return "";
  }
}

/** Appends a row, creating the file with a header if needed; refuses on a header mismatch. */
export function appendCsvRow(csv: string, row: CsvRow) {
  const header = CSV_COLUMNS.join(",");
  if (!existsSync(csv)) {
    writeFileSync(csv, `${header}\n`);
  } else {
    const existing = readFileSync(csv, "utf8").split(/\r?\n/, 1)[0];
    if (existing !== header) {
      throw new Error(`${csv} has header "${existing}", expected "${header}". Refusing to append a mismatched row.`);
    }
  }
  appendFileSync(csv, `${CSV_COLUMNS.map((c) => row[c]).join(",")}\n`);
}

async function report(opts: ReturnType<typeof parseArgs>) {
  const s =
    opts.samples > 1
      ? await sampleMedian(opts.samples, opts.intervalSec, {
          allowHidden: opts.allowHidden,
          onSample: (x) => console.log(`  ${formatSnapshot("sample", x)}`),
        })
      : snapshot();
  assertVisible(s, opts.allowHidden);
  const timestamp = new Date().toISOString();
  console.log(formatSnapshot(opts.label, s, timestamp));

  if (opts.csv) {
    appendCsvRow(opts.csv, {
      timestamp,
      label: opts.label,
      app_version: opts.appVersion,
      commit: appCommit(),
      os: osLabel(),
      library_tracks: opts.tracks,
      window: opts.window,
      process_count: s.processCount,
      working_set_mb: s.workingSetMb.toFixed(1),
      private_bytes_mb: s.privateBytesMb.toFixed(1),
      samples: Math.max(1, opts.samples),
      scan_seconds: "",
    });
  }
}

async function main() {
  const opts = parseArgs();

  if (!opts.watch) {
    await report(opts);
    return;
  }

  console.log(`Watching every ${opts.intervalSec}s. Press Ctrl+C to stop.`);
  // eslint-disable-next-line no-constant-condition
  while (true) {
    try {
      await report({ ...opts, samples: 1 });
    } catch (err) {
      console.error(String(err instanceof Error ? err.message : err));
    }
    await new Promise((resolve) => setTimeout(resolve, opts.intervalSec * 1000));
  }
}

if (import.meta.main) {
  main().catch((err) => {
    console.error(String(err instanceof Error ? err.message : err));
    process.exit(1);
  });
}
