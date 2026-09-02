# Performance Baseline

Tracks Luminous's steady-state memory footprint so a PR can be checked against a known baseline
before/after a change, instead of relying on "it feels heavier." See #706.

## Methodology

- **What's measured**: total memory across the app's main process and every child process it
  spawns (e.g. WebView2 renderer/GPU processes on Windows, a WebKitGTK web process on Linux) —
  this matches what a user perceives as "Luminous's memory usage" in Task Manager/`top`, not just
  the thin main process.
- **Numbers reported**: working set (Windows) / RSS (Linux) as the "what's actually resident"
  figure, and private bytes (Windows) / private Pss (Linux, via `/proc/<pid>/smaps_rollup`) as the
  "what's not shared with other processes" figure. Private bytes/Pss is the more meaningful number
  for comparing builds, since working set/RSS includes shared pages (e.g. WebView2 runtime code)
  that don't change with Luminous's own code.
- **Build measured**: a release build (`bun run tauri build`), not the dev server — the dev
  server's Vite/HMR overhead isn't representative of what ships to users.
- **Tool**: `bun run measure-memory -- --label <scenario>` (`scripts/measure-memory.ts`) takes one
  snapshot and prints it; add `--csv docs/performance-baseline.csv` to append a row, or `--watch
  --interval <sec>` to poll continuously while driving the app through a scenario.

## Scenarios

1. **Idle** — app freshly launched, library already scanned from a prior run, no playback.
2. **After a full library scan** — freshly launched, then a full rescan triggered and run to
   completion.
3. **During playback** — a track playing, with the equalizer and spectrum analyzer enabled.

## Baseline results

| Date | App version | OS | Library size | Scenario | Working set (MB) | Private bytes (MB) |
| --- | --- | --- | --- | --- | --- | --- |
| _pending_ | 2.0.0 | Windows 11 | ~2,000 tracks | Idle | | |
| _pending_ | 2.0.0 | Windows 11 | ~2,000 tracks | After full scan | | |
| _pending_ | 2.0.0 | Windows 11 | ~2,000 tracks | During playback (EQ + analyzer on) | | |

## Candidate areas if numbers look high

- `src-tauri/src/db.rs` — the r2d2 pool caps at 8 connections, each with `PRAGMA cache_size=-32000`
  (32MB), so SQLite's own page cache alone can reach ~256MB in the worst case where all 8
  connections are hot simultaneously. Cover art extraction, analyzer buffers, and the library
  scanner (batched at 300 songs/chunk) are already bounded by design and are not expected to be
  concerns.
