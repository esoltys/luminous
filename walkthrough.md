# Walkthrough — Play Statistics & Track Ratings (#76)

Branch: `claude/issue-76-play-stats` (worktree `.claude/worktrees/issue-76`)

## What this fixes

The `songs` table has always carried `rating`, `playcount`, `skipcount`, and
`lastplayed` columns, and the Home view has always queried them — but **nothing
ever wrote them**, so "Recently Played" and "Most Played" were permanently
empty, and the "Plays" column in album detail was stuck at 0. This change adds
the write paths plus the approved two-tier rating UX (quick heart in lists,
full 5-star editor in detail surfaces).

## Backend changes

| File | Change |
|------|--------|
| `src-tauri/src/stats.rs` (new) | `record_play` (playcount+1, stamps lastplayed), `record_skip`, `set_rating` with half-star normalization (`-1` = unrated, else snapped to 0.5–5.0). Fully unit-tested. |
| `src-tauri/src/player.rs` | `on_position_update` now records the listen when the existing 50% scrobble point is crossed (the old `TODO` at line 487) and returns the song id for event emission. New `note_manual_skip` records a skip only when the track hasn't reached its scrobble point. |
| `src-tauri/src/commands/stats.rs` (new) | `set_song_rating` IPC command — persists, syncs the in-memory current song, emits `song-stats-changed`. |
| `src-tauri/src/commands/player.rs` | `next_track` command now records the skip before advancing. |
| `src-tauri/src/lib.rs` | Position tick loop emits `song-stats-changed` when a play is recorded; the `MediaTrackNext` media key also records skips. Command registered. |
| `src-tauri/src/commands/tageditor.rs` | `get_song_details` now returns `rating`. |

No DB migration needed — all columns already existed.

**Semantics** (matching our reference implementation's model):
- A track "counts" once it passes 50% of its duration → `playcount + 1`, `lastplayed = now`.
- Skipping (next button, media key) *before* that point → `skipcount + 1`. Natural completion never counts as a skip; skipping after the 50% point doesn't either (it already counted as a play).
- Heart = rating 5.0; unhearting clears to unrated. Stars can set any half-step from 0.5–5.0.

## Frontend changes

| Surface | Change |
|---------|--------|
| `StarRating.svelte` (new) | 5-star control with half-star precision (click left/right half of a star), hover preview, click-again-to-clear, read-only mode. Accent-colored fill per DESIGN.md. |
| `HeartToggle.svelte` (new) | Favorite heart; filled accent when favorited. |
| Collection → Songs | Heart in the Actions column of every row. |
| Playlist view | Heart in the Actions column (hidden for unavailable tracks). |
| Player bar | Heart next to the format badge for the current track. |
| Album detail | New "Rating" column with interactive stars; the existing "Plays" column now actually increments. |
| Tag editor | Rating row (stars, `md` size) — saves immediately, DB-only (never written to the file). |
| `player.svelte.ts` | Listens for `song-stats-changed` to keep the current song's rating in sync across surfaces; new `toggleFavorite()`. |
| Locales | New `rating.*` keys + `collection.tableHeaderRating` in English and French. |
| `vite.config.js` | Added the `svelteTesting` plugin — this repo's first Svelte *component* test needed the browser-condition resolution (also unblocks issue #35's component-test work). |

## Verification

- `cargo test` — 7 passed (4 new stats tests: play increment + lastplayed stamp, skip isolation, rating persistence, normalization snap/clamp).
- `cargo check` / `cargo fmt` — clean.
- `bun run test:run` — 127 passed (6 new StarRating component tests).
- `bun run check` — 0 errors, 0 warnings.

## How to verify manually (dev server)

1. Play any track past its halfway point → the Home view's "Recently Played" / "Most Played" rows populate (revisit Home), and the album detail "Plays" count increments.
2. Skip a track early (next button or media key) → `skipcount` increments (visible in DB; UI surface for skip counts comes with #13's column work).
3. Click the heart on a row in Collection → it fills with the accent color; the same song's heart in the player bar (if playing) updates live.
4. Open an album → click stars in the Rating column, including half-star clicks on the left half of a star; click the same value again to clear.
5. Open the tag editor on any song → the Rating row shows the same value; changing it saves instantly without pressing Save.
6. Events: rating changes made in any list are reflected in the player bar via `song-stats-changed`.

## Follow-ups this unblocks

- #26 smart playlists (rating/playcount/lastplayed rule fields now have data)
- #83 scrobbling (same play-completion hook)
- #44 queue drawer History tab (`lastplayed` ordering)
