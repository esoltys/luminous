# Walkthrough — Play Statistics & Track Ratings (#76)

Branch: `claude/issue-76-play-stats` (worktree `.claude/worktrees/issue-76`)

## What this fixes

The `songs` table has always carried `rating`, `playcount`, `skipcount`, and
`lastplayed` columns, and the Home view has always queried them — but **nothing
ever wrote them**, so "Recently Played" and "Most Played" were permanently
empty, and the "Plays" column in album detail was stuck at 0. This change adds
the write paths plus a rating UX with a **single, user-selectable style**:
**Heart** (default) or **5-Star**, chosen in Settings → General. One control
style renders consistently across every surface — never both at once.

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
- Heart = rating 5.0; unhearting clears to unrated. Stars can set any half-step from 0.5–5.0. Both styles share the same underlying `rating` column, so switching styles never loses data (a 3.5-star track simply shows as un-hearted in heart mode).
- Every stats write emits a `song-stats-changed` event carrying the song's full current stats (`rating`, `playcount`, `skipcount`, `lastplayed`); the collection store, playlists store, album detail view, and player store all listen and patch their copies in place — a heart set in the play dock updates the playlist and album detail rows instantly, and vice versa.

## Frontend changes

| Surface | Change |
|---------|--------|
| Settings → General | New **"Rating style"** dropdown: Heart (default) or 5-star, persisted via app settings (`rating_style`). |
| `SongRating.svelte` (new) | The single rating control used everywhere — renders a heart or the star row per the setting. |
| `StarRating.svelte` (new) | 5-star control with half-star precision (click left/right half of a star), hover preview, click-again-to-clear. Accent-colored fill per DESIGN.md. |
| `HeartToggle.svelte` (new) | Favorite heart; filled accent when favorited. |
| `prefs.svelte.ts` (new store) | Loads/saves the rating style. |
| Collection → Songs | New sortable **Rating** column. |
| Playlist view | New Rating column (blank for unavailable tracks). |
| Player bar | Rating control next to the format badge for the current track. |
| Album detail | New Rating column; the existing "Plays" column now actually increments. |
| Tag editor | Rating row — saves immediately, DB-only (never written to the file). |
| `utils/stats.ts` (new) | Shared `applySongStats` helper + event payload type used by all listeners. |
| Stores | `player`, `collection`, and `playlists` stores (plus album detail's local list) subscribe to `song-stats-changed` and patch songs in place — cross-view sync. |
| Locales | `rating.*`, `settings.ratingStyle*`, `collection.tableHeaderRating` keys in English and French. |
| `vite.config.js` | Added the `svelteTesting` plugin — this repo's first Svelte *component* test needed the browser-condition resolution (also unblocks issue #35's component-test work). |

## Verification

- `cargo test` — 7 passed (4 new stats tests: play increment + lastplayed stamp, skip isolation, rating persistence, normalization snap/clamp).
- `cargo check` / `cargo fmt` — clean.
- `bun run test:run` — 132 passed (6 StarRating + 5 SongRating component tests).
- `bun run check` — 0 errors, 0 warnings.

## How to verify manually (dev server)

1. Play any track past its halfway point → the Home view's "Recently Played" / "Most Played" rows populate (revisit Home), and the album detail "Plays" count increments.
2. Skip a track early (next button or media key) → `skipcount` increments (visible in DB; UI surface for skip counts comes with #13's column work).
3. Heart a track **in the play dock** → the same track's heart updates instantly in Collection, the active playlist, and album detail (and vice versa from any list).
4. Settings → General → switch "Rating style" to **5-star** → every rating control across the app becomes a star row; half-star clicks on the left half of a star; click the same value again to clear. Switch back to Heart — a 5.0-rated track shows a filled heart.
5. Open the tag editor on any song → the Rating row shows the same value; changing it saves instantly without pressing Save.
6. Sort the Collection songs table by the new Rating column header.

## Follow-ups this unblocks

- #26 smart playlists (rating/playcount/lastplayed rule fields now have data)
- #83 scrobbling (same play-completion hook)
- #44 queue drawer History tab (`lastplayed` ordering)
