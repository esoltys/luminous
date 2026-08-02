# Walkthrough: Compact Rows view for Albums/Artists

Implements [#220](https://github.com/esoltys/luminous/issues/220) on branch
`worktree-collection-rows-view` (worktree: `.claude/worktrees/collection-rows-view`).

## What changed

- **New toggle**: a Cards/Rows icon toggle (grid / rows icons) now sits above the
  "Showing N albums/artists" line in the Collection view's Albums and Artists tabs.
  Albums and Artists each remember their own view mode independently, and the
  choice persists across restarts (same mechanism as the waveform/moodbar seek
  bar toggle — stored via the backend `app_state` settings table).
- **Rows view**: a new compact row layout, reusing the same visual shell as the
  Home page's "Recently Added" rows — a small square cover thumbnail, title,
  subtitle, and trailing metadata, wrapped in a responsive grid so rows flow
  left-to-right then down (1-2-3 / 4-5-6) instead of stacking in a single column.
  - Album rows: cover art is the album's own single `CoverArt` (embedded/automatic/
    manual art via `sample_song_id`), title/artist, track-count label, and year.
    Click to open the album, double-click to queue/play it — no hover play
    button, matching the click-to-play removal already on `main` (#221).
  - Artist rows: cover art is derived the same way `ArtistCard`'s `CoverStack`
    picks its *front* tile (first album with real art, else the artist's first
    song) — pulled out into a shared `getArtistCoverStack()` helper so the card
    and the row can never disagree on which cover represents an artist.
- **Cards view is unchanged** — same `AlbumCard`/`ArtistCard` grid as before.

## Notable refactors (no behavior change)

- `ArtistCard.svelte`'s inline cover-selection logic moved to
  `utils/covers.ts::getArtistCoverStack()`, reused by the new `ArtistRowCard`.
- `AlbumCard.svelte`'s double-click "queue this album into a playlist and play
  it" logic moved to `utils/playlist.ts::queueAlbumAsPlaylist()`, reused by
  the new `AlbumRowCard` so both views behave identically on double-click.

## Files touched

- `src/lib/stores/prefs.svelte.ts` — `albumsViewMode` / `artistsViewMode`
- `src/lib/utils/covers.ts` — `getArtistCoverStack()` (+ test)
- `src/lib/utils/playlist.ts` — `queueAlbumAsPlaylist()`
- `src/lib/components/AlbumRowCard.svelte`, `ArtistRowCard.svelte` (new, + tests)
- `src/lib/components/CollectionView.svelte` — toggle UI, view-mode-aware rendering
- `src/lib/components/AlbumCard.svelte`, `ArtistCard.svelte` — refactored to use
  the shared helpers above
- `src/lib/locales/en.ts` / `fr.ts` — `collection.viewCards` / `collection.viewRows`

## Checks run

- `bun run check` — 0 errors, 0 warnings
- `bun run test:run` — 276/276 tests passing (14 new: toggle behavior in
  `CollectionView.test.ts`, `AlbumRowCard.test.ts`, `ArtistRowCard.test.ts`,
  `covers.test.ts`)

## How to verify manually

The dev server is running (`bun run tauri dev`, launched from this worktree).
In the app:

1. Go to **Collection → Albums**. You should see the new toggle above "Showing
   N albums" — click the rows icon to switch to the compact list, click the
   grid icon to switch back. Reload/restart the app and confirm it remembers
   "rows".
2. Go to **Collection → Artists** and repeat — confirm its toggle is
   independent of the Albums tab's choice.
3. In Albums rows view: click a row to open the album; double-click a row
   and confirm it queues/plays the same "Album: {name}" playlist as
   double-clicking a card does today (no hover play button, by design).
4. In Artists rows view: confirm each row's cover matches the front cover
   shown on that same artist's card in Cards view.

Not merged yet — waiting on your review. Let me know if anything should
change before I merge `worktree-collection-rows-view` into `main` and close
#220.
