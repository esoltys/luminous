#!/usr/bin/env bun
/**
 * Snapshots Luminous's total memory footprint (main process + all child
 * processes, e.g. WebView2 renderer/GPU processes on Windows) for the
 * baseline in docs/PERFORMANCE.md.
 *
 * Usage:
 *   bun run scripts/measure-memory.ts --label idle
 *   bun run scripts/measure-memory.ts --label "after-scan" --csv docs/performance-baseline.csv
 *   bun run scripts/measure-memory.ts --watch --interval 5
 */

import { execFileSync } from "node:child_process";
import { appendFileSync, existsSync, writeFileSync } from "node:fs";

const BINARY_NAME = "LuminousMusicPlayer";
const CSV_HEADER = "timestamp,label,process_count,working_set_mb,private_bytes_mb\n";

interface Snapshot {
  processCount: number;
  workingSetMb: number;
  privateBytesMb: number;
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
  };
}

function snapshotWindows(): Snapshot {
  // Sum WorkingSet64/PrivateMemorySize64 across the main exe and every
  // descendant process (WebView2 renderer/GPU/crashpad, etc.) so the total
  // matches what a user perceives as "Luminous's memory usage", not just
  // the thin main process.
  const script = `
$ErrorActionPreference = 'Stop'
$main = Get-Process -Name '${BINARY_NAME}' -ErrorAction SilentlyContinue | Select-Object -First 1
if (-not $main) { Write-Output 'NOTFOUND'; exit 0 }
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
Write-Output "$count,$ws,$priv"
`;
  const out = execFileSync("powershell", ["-NoProfile", "-Command", script], {
    encoding: "utf8",
  }).trim();

  if (out === "NOTFOUND" || out === "") {
    throw new Error(
      `Process '${BINARY_NAME}' not found. Is Luminous running? (Task Manager shows the exe as "${BINARY_NAME}.exe")`,
    );
  }
  const [count, ws, priv] = out.split(",").map(Number);
  return {
    processCount: count,
    workingSetMb: ws / 1024 / 1024,
    privateBytesMb: priv / 1024 / 1024,
  };
}

function snapshotLinux(): Snapshot {
  // Sum RSS (proxy for working set) and Pss/Private (proxy for private
  // bytes, via /proc/<pid>/smaps_rollup) across the main binary and any
  // child processes (e.g. a WebKitGTK web process).
  const pidsOut = execFileSync("pgrep", ["-f", BINARY_NAME], { encoding: "utf8" }).trim();
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
      const status = execFileSync("cat", [`/proc/${pid}/status`], { encoding: "utf8" });
      const rssMatch = status.match(/^VmRSS:\s+(\d+)/m);
      if (rssMatch) rssKb += Number(rssMatch[1]);

      try {
        const rollup = execFileSync("cat", [`/proc/${pid}/smaps_rollup`], { encoding: "utf8" });
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
  };
}

function snapshot(): Snapshot {
  if (process.platform === "win32") return snapshotWindows();
  if (process.platform === "linux") return snapshotLinux();
  throw new Error(`Unsupported platform: ${process.platform} (this script covers Windows and Linux)`);
}

function report(label: string, csv: string | undefined) {
  const s = snapshot();
  const timestamp = new Date().toISOString();
  const line = `[${timestamp}]${label ? ` ${label}:` : ""} ${s.processCount} process(es), working set ${s.workingSetMb.toFixed(1)} MB, private bytes ${s.privateBytesMb.toFixed(1)} MB`;
  console.log(line);

  if (csv) {
    if (!existsSync(csv)) writeFileSync(csv, CSV_HEADER);
    appendFileSync(
      csv,
      `${timestamp},${label},${s.processCount},${s.workingSetMb.toFixed(1)},${s.privateBytesMb.toFixed(1)}\n`,
    );
  }
}

async function main() {
  const { label, csv, watch, intervalSec } = parseArgs();

  if (!watch) {
    report(label, csv);
    return;
  }

  console.log(`Watching every ${intervalSec}s. Press Ctrl+C to stop.`);
  // eslint-disable-next-line no-constant-condition
  while (true) {
    try {
      report(label, csv);
    } catch (err) {
      console.error(String(err instanceof Error ? err.message : err));
    }
    await new Promise((resolve) => setTimeout(resolve, intervalSec * 1000));
  }
}

main().catch((err) => {
  console.error(String(err instanceof Error ? err.message : err));
  process.exit(1);
});
