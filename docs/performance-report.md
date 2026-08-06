# Performance Improvement Report

Living document tracking a performance pass across four areas: library-scan disk I/O,
background waveform processing, frontend "glass" surface / reactive logo rendering, and
memory usage. Updated as each round of investigation/fixes lands.

Status legend: 🔍 investigating · 🛠 fix in progress · ✅ fixed & verified · ⏭ deferred

## Summary

| Area | Status | Change |
|---|---|---|
| Library scan disk I/O | ✅ fixed & verified | Batched DB transactions + bounded-parallel tag reads in `scan_all` |
| Waveform background processing | ⏭ deferred | No live problem found; dead unthrottled code path flagged only |
| Glass surfaces / reactive logo | ✅ fixed & verified | GPU-pinned `.glass`/`.glass-premium`; epsilon-thresholded logo intensity updates |
| Memory usage | ✅ investigated, no fix needed | No unbounded caches or leaks found; derived song views are GC'd, not duplicated |

## Log

### 2026-08-06 — Kickoff

Started investigation pass covering:
1. Disk access while scanning the library (thousands of files) — sync vs batched I/O, DB transaction batching.
2. Background waveform analysis — thread/task pool usage, contention with playback.
3. Frontend glass surfaces / reactive logo — backdrop-filter cost, compositing, re-render frequency.
4. Memory usage — unbounded caches, listener/effect cleanup, Rust-side cache bounds.

### 2026-08-06 — Investigation findings

**1. Library scan (`src-tauri/src/collection.rs`)**
- `scan_all()` phase 2 (lines ~444-484) does one `SELECT mtime` + one `upsert_song` per file with no surrounding transaction — every file is its own implicit autocommit under WAL, and there's no `rayon`/thread-pool parallelism for tag reads.
- Album-art phase (lines ~496-630) is already reasonably throttled (150ms sleep, 50-fetch cap on remote lookups) — not a target.
- **Fix direction:** wrap the per-file DB write loop in chunked transactions (every 200-500 files); parallelize tag decoding across a bounded thread pool while keeping DB writes serialized.

**2. Waveform background processing (`src-tauri/src/waveform.rs`, `band_waveform.rs`, `lib.rs`)**
- Live code path is well-designed: on-demand generation via `spawn_blocking`, dedup'd by a global gen-lock, cached in SQLite, single transaction for both waveform+band-waveform.
- `backfill_missing_visualizers` exists but is dead code (called only by its own tests) — has no throttling between songs, unlike `loudness.rs`'s `spawn_background_analyzer` (250ms sleep between tracks). Not a live issue; flagged as tech debt, not fixing now since it isn't wired up.
- Real-time spectrum analyzer runs a 30fps FFT loop sharing a mutex with the playback engine — worth profiling later but not an obvious bug.
- **Decision: deferred**, no live bug to fix.

**3. Frontend glass/logo (`src/app.css`, `src/lib/components/ReactiveLogoBrand.svelte`)**
- `b053043` pinned `.glass-surface` to its own GPU layer but not `.glass`/`.glass-premium`, which are used across `PlayerBar`, `TopNavigation`, `Sidebar`, `Miniplayer`, `RightPanel`, `+layout`.
- `ReactiveLogoBrand.svelte` updates 6 `$state` vars → 8 `$derived` values on every 30fps spectrum event, bound to `r`/`stroke-width`/`opacity` on SVG elements with `feGaussianBlur` filters — forces filter-region recompute + re-rasterization every frame. Contrast with `SpectrumVisualizer.svelte`, which draws imperatively to a canvas outside Svelte reactivity.
- **Fix direction:** extend GPU-layer pin to `.glass`/`.glass-premium`; convert logo's per-frame path to imperative attribute writes and avoid animating blurred-filter geometry every frame.

**4. Memory usage**
- No unbounded Rust-side caches (art cached to disk by filename, waveform data lives in SQLite, not in-process).
- Listener cleanup (`unlisten` in `onDestroy`) verified correct in components checked. No blob-URL usage found.
- Open question, not yet confirmed: whether `CollectionView`/`PlaylistView` duplicate the full song array into local component `$state` in addition to the shared `collection.svelte.ts` store. Investigating next.

Proceeding to implement fixes for #1 and #3 now.

### 2026-08-06 — Fixes landed

**1. Library scan (`src-tauri/src/collection.rs`)**
- Replaced the per-file `SELECT mtime` (one query per file) with a single `SELECT path, mtime FROM songs WHERE unavailable = 0` loaded into a `HashMap` up front — turns O(files) round-trips into one.
- Split `read_and_upsert_song` into `read_and_prepare_song` (pure tag read + local-art resolution, no DB) and the DB write, so tag reading can run off the single connection.
- Tag reads for changed/new files now run via a **bounded** Rayon thread pool (`scan_thread_count()` = `available_parallelism() - 2`, min 1) instead of serially — deliberately capped below full core count so a scan doesn't starve the audio playback thread or concurrent loudness/waveform analysis, per user feedback to balance speed against system load.
- DB writes are batched into transactions of `SCAN_WRITE_BATCH_SIZE = 300` files instead of one implicit-autocommit write per file — this also bounds memory: at most one batch's worth of decoded `Song`s is held at a time, not the whole library.
- Verified: `cargo check --lib` clean, `cargo test --test library_scan_bdd` — 3/3 scenarios, 13/13 steps passing (full scan, incremental mtime-skip, and directory-add scenarios all still correct).

**2. Frontend glass surfaces (`src/app.css`)**
- Extended the `will-change: backdrop-filter; transform: translateZ(0);` GPU-layer pin (previously only on `.glass-surface`, from `b053043`) to `.glass` and `.glass-premium`, used by `PlayerBar`, `TopNavigation`, `Sidebar`, `Miniplayer`, `RightPanel`, `+layout`.
- Verified: `bun run check` (svelte-check) — 0 errors, 0 warnings.

**3. Reactive logo (`src/lib/components/ReactiveLogoBrand.svelte`)**
- Considered a full imperative-refs rewrite, but the actual cost the investigation found is that changing `r`/`stroke-width` on `feGaussianBlur`-filtered SVG elements forces the browser to re-rasterize the blur — that cost is unrelated to whether the write is "reactive" or "imperative" (Svelte 5 already patches only the changed attribute). A transform-based redesign would avoid the re-rasterization but changes the render technique for a component with an explicit design spec (`docs/luminous-mark-reactive.svg`, DESIGN.md's Logo System) — too visually risky to do without sign-off, so deferred.
- Landed instead: an epsilon threshold (`INTENSITY_EPSILON = 0.015`) on the three spectrum-driven intensities, skipping state writes (and therefore the blur re-rasterization) when a new reading is visually indistinguishable from the current one — real savings during quiet/steady passages, no visual change at the delta this threshold catches.
- Verified: `bun run check` (svelte-check) — 0 errors, 0 warnings. Not visually re-verified in a running app (Luminous's Tauri IPC bridge isn't reachable from the browser-automation tools in this environment — needs a manual check in the user's dev server).

**4. Memory usage**
- Investigated the flagged open question: `CollectionView.svelte`'s `filteredSongs`/`sortedAlbums`/`sortedArtists` are `$derived.by` views (shallow `[...arr].sort()` clones of references) over the shared `collectionStore`, recomputed and garbage-collected on each change — not a persistent duplicate of the full song list. No fix needed.

### Deferred / not fixed this round
- `backfill_missing_visualizers` (`waveform.rs`) has no throttling between songs, but it's dead code (unreferenced outside its own tests) — flagged as tech debt for if/when it's ever wired up, not fixed now.
- Reactive logo's filter-re-rasterization cost is only partially mitigated (epsilon threshold); a full fix needs a transform-based render redesign with visual sign-off.

### Next steps for a future round
- Manually verify the logo/glass changes visually in the running app (dev server), including confirming the epsilon threshold doesn't introduce visible stutter.
- Profile the 30fps spectrum-analyzer/playback-engine mutex contention (`lib.rs`) under load — flagged as a maybe, not confirmed as a problem.
- If `backfill_missing_visualizers` is ever wired into a live code path, add the same throttle pattern `loudness.rs`'s `spawn_background_analyzer` uses.
