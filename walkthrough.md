# Walkthrough — Exclusive Routing, Gapless Double-Buffering & 20-Band Parametric EQ (#47)

Branch: `claude/issue-47-audio-pipeline` (worktree `.claude/worktrees/issue-47`)

## What this delivers

Issue #47 asked for the audiophile pipeline core: bit-perfect routing, gapless
track-to-track playback, and an upgrade from the 10-band EQ to a 20-band
parametric one. This change lands the **pipeline contract** that three queued
features (#77 loudness, #79 fades/crossfade, #82 radio streaming) build on, plus
the full 20-band parametric EQ end to end.

The headline behaviors:

- **Gapless double-buffering** — the next track is decoded *ahead* of the
  boundary and fed into the same output buffer, so album sides and DJ mixes play
  through with no silence or click between tracks.
- **Ordered, zero-allocation DSP chain** — `decode → loudness gain → EQ preamp →
  EQ bands → fade envelope → volume → output`, with each stage skipped when it's
  neutral (so a flat, EQ-off signal reaches the device untouched).
- **20-band parametric EQ** — a new mode alongside the existing 10-band graphic
  one, with per-band frequency / gain / Q, persisted and restored.

## The gapless architecture (how it actually works)

Rebuilding the CPAL device on every track wedges the OS audio subsystem (a hazard
the existing code already documents), so the decode thread keeps **one** output
stream and ring buffer for its whole life. Gapless rides on top of that:

1. When the playing track is within 8 s of its end boundary, the engine emits a
   new **`AboutToFinish`** event.
2. The player computes the next track *without mutating any state*
   (`peek_next_natural` — respects queue, shuffle order, repeat mode,
   stop-after-current) and sends **`PreloadNext`**. The decode thread opens and
   primes that track behind the current one.
3. When the current file is exhausted, decoding **continues into the same ring
   buffer** — no pause, no drain. The buffer never empties, so there's no gap.
4. When the output callback actually consumes the last sample of the finished
   track, the engine emits **`TrackTransitioned`**; the player then commits its
   queue/index/scrobble bookkeeping — the same result as `on_track_finished`, but
   without ever issuing a new `Play`.

If the context changes mid-flight (you toggle shuffle, edit the queue), the primed
track is dropped (`ClearPreload`) and `on_gapless_transition` self-heals by
falling back to the normal advance. Pause/seek during the drain reopen the correct
song. When nothing is preloaded (end of playlist, preload failure), the original
drain-then-`TrackFinished` path still runs — so this is strictly additive.

## Pipeline contract for #77 / #78 / #79 / #82

Baked in now so those features don't force a second refactor (per the enrichment
comment on the issue):

- **Loudness gain slot (#77)** — a lock-free per-track multiplier applied *before*
  the EQ preamp. `AudioEngine::set_loudness_gain`.
- **Fade envelope slot (#79)** — a lock-free multiplier after the EQ, before
  volume. `AudioEngine::set_fade_gain`. `AboutToFinish` is the crossfade trigger.
- **`end_nanosec` boundary (#78)** — decode now truncates at the CUE cut, not just
  at EOF. The engine half of CUE support, essentially free while restructuring.
- **Source-agnostic decode (#82)** — file opening goes through `open_media_source`
  returning a `Box<dyn MediaSource>`, so an HTTP radio stream slots in later.

On exclusive routing: CPAL 0.15 drives WASAPI in shared mode only (no exclusive
path in the crate), so true bit-perfect exclusive output would need a forked/newer
CPAL. What this change guarantees instead is the **software half**: the DSP chain
is a genuine passthrough when neutral — no resample, no gain, no EQ math runs — so
nothing in our code alters the samples. I flagged the exclusive-mode limit rather
than silently claiming it; happy to file a follow-up if you want to pursue it.

## Backend changes

| File | Change |
|------|--------|
| `src-tauri/src/audio.rs` | Gapless double-buffer, `AboutToFinish`/`TrackTransitioned` events, `PreloadNext`/`ClearPreload` commands, `ActiveTrack` abstraction, `end_nanosec` truncation, ordered DSP chain with lock-free loudness/fade slots, pre-allocated visualizer scratch, `open_media_source` seam. |
| `src-tauri/src/player.rs` | `peek_next_natural` (no-side-effect next-track resolution), `prepare_gapless_next`, `on_gapless_transition` (commit + self-heal). |
| `src-tauri/src/lib.rs` | Event loop handles the two new events; EQ startup restore reads mode + parametric JSON. |
| `src-tauri/src/commands/player.rs` | Shuffle/repeat changes clear a stale preload. |
| `src-tauri/src/equalizer.rs` | New `Parametric20` mode + `ParametricBand {freq, gain_db, q}`, 20 log-spaced defaults, per-mode filter cascade, explicit-Q biquad. **10-band graphic API untouched** (BDD suite unchanged). |
| `src-tauri/src/commands/equalizer.rs` | `set_equalizer_mode`, `set_parametric_band`, `reset_parametric_bands`; `get_equalizer_state` returns mode + parametric. |
| `src-tauri/src/db.rs` | Migration 4: `equalizer_settings.mode` + `.parametric` (JSON), backward-compatible defaults. |

## Frontend changes

| Surface | Change |
|---------|--------|
| Settings → Equalizer | **Mode segmented control** (10-band graphic ⇄ 20-band parametric). Graphic keeps its preset dropdown; parametric gets a **Reset Bands** button. |
| `Equalizer.svelte` | 20 gain sliders; click a band to select it and reveal a **frequency (log-scaled) + Q** detail panel; log-frequency curve preview for both modes. |
| `en.ts` / `fr.ts` | New strings: parametric title/subtitle, mode labels, frequency/Q/band, reset. |

## How to verify

Dev server is running (`bun run tauri dev`). Suggested checks:

1. **Gapless** — queue two tracks (ideally a continuous album) and let the first
   play out; the second should start with no gap. Watching the logs you'll see
   `AboutToFinish` → preload → `TrackTransitioned`.
2. **Gapless + skip interaction** — while a track is near its end, hit next or
   toggle shuffle; playback should stay correct (self-heal path).
3. **Parametric EQ** — Settings → Equalizer → **20-band parametric**. Drag a band,
   click it to pick a frequency and Q, hear the change live. Switch back to
   10-band graphic — your graphic gains are preserved. Restart the app; mode and
   bands persist.
4. **Bit-perfect passthrough** — with the EQ disabled and preamp at 0, the signal
   path is a pure copy.

Screenshots: `docs/screenshots/equalizer.png` (graphic) and
`docs/screenshots/equalizer-parametric.png` (parametric).

## Test status

- `cargo test` — 18/18 (5 new equalizer unit tests, 3 new audio-helper tests)
- `cargo test --test equalizer_bdd` — 3/3 (unchanged; graphic API preserved)
- `bun run test:run` — 132/132
- `bun run check` — 0 errors in app code (one pre-existing unrelated error in a
  build script, `scripts/embedded-art-cache.ts`, missing `music-metadata` types)
- `cargo build` / `cargo fmt` — clean

## Notes / limitations

- **Exclusive mode** isn't achieved (CPAL 0.15 is shared-mode only); the DSP chain
  is a verified passthrough instead. Flagged above for a possible follow-up.
- Loudness (#77) and fade (#79) slots are wired but dormant — their multipliers
  stay at 1.0 until those features drive them.
