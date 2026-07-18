# Walkthrough — Loudness Normalization: EBU R128 Analysis with ReplayGain Fallback (#77)

Branch: `claude/goal-feature-77-1883a5` (worktree `.claude/worktrees/goal-feature-77-1883a5`)

## What this delivers

Issue #77 asked for consistent perceived loudness across tracks, so quiet
acoustic albums and loud modern masters play back at a similar level. This
lands the full pipeline: a background **EBU R128 (BS.1770) analyzer**, a
**ReplayGain 2.0 tag fallback** for tracks not yet analyzed, a user-configurable
**target level / mode / fallback gain**, and a compact settings section to
control it — all riding on the `loudness_gain` DSP slot #47 already reserved.

The headline behaviors:

- **Background R128 analysis** — a low-priority thread decodes each
  not-yet-analyzed local/collection track (most-recently-added first), computes
  integrated loudness per ITU-R BS.1770-4, and writes
  `songs.ebur128_integrated_loudness_lufs`. It never competes with playback or
  scanning — one track at a time, throttled between tracks.
- **ReplayGain tag fallback** — `REPLAYGAIN_TRACK_GAIN` / `REPLAYGAIN_ALBUM_GAIN`
  tags are read at scan time and used until a track's own R128 analysis lands.
- **Priority order at playback**: measured R128 loudness → ReplayGain tag
  (track or album, per the user's mode setting) → a fixed fallback gain (default
  -6 dB, safe against clipping unanalyzed/unknown tracks).
- **Settings**: enable toggle (off by default), target level (-24..-14 LUFS,
  default -18), track/album mode, fallback gain, and a live "N tracks remaining"
  analysis-progress line — all in the existing Equalizer settings tab.

## How the gain reaches the DSP chain

`AudioEngine::set_loudness_gain` (the dormant slot #47 wired into
`decode → loudness gain → EQ preamp → EQ bands → fade envelope → volume`) is a
single atomic f32 — the audio callback never allocates or blocks to read it.

The gain is computed and applied at three points in [player.rs](src-tauri/src/player.rs):

1. **Every non-gapless track start** (`play_at_index`, the queue-direct-play
   branch of `next_track`) — applied right before `audio.play(...)`, before any
   of the new track's samples reach the buffer.
2. **Gapless handovers** (`on_gapless_transition`) — applied exactly when the
   engine reports `TrackTransitioned`, i.e. the moment the previous track's last
   sample was actually consumed and the next one becomes audible. Setting it
   earlier (at `AboutToFinish`/preload time) would wrongly affect the *previous*
   track's still-draining tail, since the gain slot is global, not tagged per
   buffered sample.
3. **Settings changes** (`Player::refresh_loudness_gain`) — recomputes the gain
   for whatever's currently playing, so toggling the target level is audible
   immediately instead of waiting for the next track.

## Analysis pipeline

[loudness.rs](src-tauri/src/loudness.rs) is the new module:

- `decode_channels` mirrors `analyzer::decode_all_samples`'s offline Symphonia
  decode loop, but keeps channels deinterleaved (capped at stereo) instead of
  downmixing to mono, since BS.1770 needs per-channel K-weighting before the
  channels are combined.
- `analyze_integrated_loudness` feeds those channels through the `bs1770` crate
  (`ChannelLoudnessMeter` → `reduce_stereo` → `gated_mean`) to get LUFS.
- `spawn_background_analyzer` is the worker thread: picks the next unanalyzed
  track (`ebur128_integrated_loudness_lufs IS NULL`, newest-added first), and on
  failure (corrupt/unsupported file) writes a sentinel value (`-100.0`, clearly
  outside any real measurement) so it's never retried — `compute_gain_linear`
  filters out anything outside a plausible LUFS range and falls through to the
  ReplayGain/fallback path instead.
- `compute_gain_linear` is the pure gain-math function (unit tested): measured
  loudness takes priority; otherwise a ReplayGain tag (normalized to the
  -18 LUFS RG reference, adjusted for the user's target) is used; otherwise the
  fallback gain. Result is clamped to ±30/+12 dB before converting to linear, as
  a backstop against runaway gain from bad tags or measurements.

## Schema (migration 5)

- `songs.replaygain_track_gain` / `replaygain_album_gain` (nullable REAL) —
  parsed from lofty's `ItemKey::ReplayGainTrackGain` / `ReplayGainAlbumGain` at
  scan time in [collection.rs](src-tauri/src/collection.rs).
- `loudness_settings` table (single row) — `enabled`, `target_lufs`, `mode`
  (`track`/`album`), `fallback_gain_db`.

`ebur128_loudness_range_lu` (loudness range) was already in the schema from
before this issue but is not populated — only integrated loudness is needed for
gain computation, and adding LRA analysis didn't seem worth the complexity for
this pass.

## What's intentionally out of scope

- **R128_TRACK_GAIN / R128_ALBUM_GAIN Opus tags** (a different fixed-point
  encoding, -23 LUFS reference) aren't parsed — only the standard
  `REPLAYGAIN_*` Vorbis/ID3 tags. Opus files without those tags simply use the
  fallback gain until the background analyzer reaches them, which it always
  eventually does.
- **Surround/multichannel BS.1770 weighting** — analysis is capped at the first
  two channels (mono/stereo covers the overwhelming majority of music files).

## Files changed

- [src-tauri/src/db.rs](src-tauri/src/db.rs) — migration 5.
- [src-tauri/src/models.rs](src-tauri/src/models.rs) — `Song` RG fields,
  `LoudnessSettings`, `LoudnessMode`, `LoudnessAnalysisProgress`.
- [src-tauri/src/collection.rs](src-tauri/src/collection.rs) — RG tag ingestion,
  column plumbing.
- [src-tauri/src/commands/player.rs](src-tauri/src/commands/player.rs) — raw
  song queries updated for the new columns.
- [src-tauri/src/loudness.rs](src-tauri/src/loudness.rs) — new module (analysis,
  gain math, background worker, settings persistence).
- [src-tauri/src/player.rs](src-tauri/src/player.rs) — gain application at track
  boundaries.
- [src-tauri/src/commands/loudness.rs](src-tauri/src/commands/loudness.rs) — new
  Tauri commands.
- [src-tauri/src/lib.rs](src-tauri/src/lib.rs) — module/command registration,
  background analyzer startup.
- [src-tauri/Cargo.toml](src-tauri/Cargo.toml) — `bs1770` dependency.
- [src/lib/components/Equalizer.svelte](src/lib/components/Equalizer.svelte) —
  Loudness Normalization settings section.
- [src/lib/locales/en.ts](src/lib/locales/en.ts) /
  [fr.ts](src/lib/locales/fr.ts) — `loudness.*` strings.
- [scripts/tauri-ipc-mock.ts](scripts/tauri-ipc-mock.ts) — mock IPC for the new
  commands (screenshot harness).

## Testing / Verification

- `cargo test --lib` — 23 passed (6 new `loudness::tests` covering the gain
  priority order, target-level adjustment, sentinel filtering, and album/track
  mode), 1 pre-existing ignored test.
- `bun run check` — 0 errors.
- `bun run take-screenshots --name=equalizer` — confirms the new section
  renders correctly (see `docs/screenshots/equalizer.png`).
- `bun run tauri dev` — launched against the real library; loaded, rendered
  covers, and ran the new migration + background analyzer with no errors or
  panics in the logs.

## Next steps for your approval

- Try it live: enable Loudness Normalization in Settings → Equalizer, play a
  few tracks from albums with very different mastering loudness, and confirm
  the level feels more consistent. The "N tracks remaining" line should count
  down as your library gets analyzed in the background.
- Once you're happy, I'll merge this branch, clean up the worktree, and close
  issue #77 referencing the merge commit.
