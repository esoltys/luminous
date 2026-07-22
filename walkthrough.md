# Walkthrough — Advanced Filtering, Customizable Columns & Smart Playlist Integration (#13)

Branch: `feature-advanced-filter-and-columns`

## What this delivers

Issue #13 asked for customizable track table columns, a power-user filter
grammar in the search bar, and Smart/Advanced Playlist creation with a
distinct badge. All three landed, plus several rounds of bug fixes once
manual testing surfaced problems.

### 1. Advanced filter grammar (backend)

- **[filter_parser.rs](src-tauri/src/filter_parser.rs)** — new tokenizing lexer/parser for
  search queries: bare terms plus `field:value` / `field:<op>value` with
  `= != > >= < <=`. Text fields (`artist`, `album`, `title`, `genre`,
  `composer`), numeric fields (`year`, `bitrate`, `track`, `disc`, `rating`,
  `playcount`, `skipcount`), and `duration` (`MM:SS` or seconds).
- **[collection.rs:382](src-tauri/src/collection.rs#L382)** — `search_songs` compiles parsed
  field terms into parameterized SQL `WHERE` clauses, `AND`ed with the
  existing FTS5 full-text match for bare terms. Invalid syntax degrades to
  plain FTS5 rather than erroring.

### 2. Smart playlists: creation, population, naming, categorization

- **[playlist.rs:376](src-tauri/src/playlist.rs#L376)** — `populate_dynamic_playlist` runs the
  rule query immediately on creation/update and inserts matches into
  `playlist_items`, so a new smart playlist isn't empty until some later
  sync.
- **Fixed a real bug found in manual testing**: the `contains` operator was
  being serialized literally into the spec string instead of being
  translated to the parser's actual grammar, so every smart playlist came
  back empty. Fixed in `SmartPlaylistBuilderModal.svelte`.
- **[SmartPlaylistBuilderModal.svelte](src/lib/components/SmartPlaylistBuilderModal.svelte)** —
  suggests a descriptive name from the current rules (`genre:jazz` → "Jazz
  Mix", `artist:"Miles Davis"` → "Miles Davis Selection",
  `rating:>=4 genre:rock` → "4★+ · Rock Mix", decade ranges → "1980s Rock
  Mix"), while preserving manual edits to the name field. Also fixed an
  infinite render loop caused by mutating the reactive `rules` array
  in-place while computing the suggestion.
- **Categorization fix**: system genre/decade auto-playlists and
  user-created Smart playlists both use `dynamic_enabled` rows, so they
  need to be told apart correctly. System auto-playlists store the raw
  genre/decade as `dynamic_spec` (e.g. `"Rock"`, `"decade:1980s"`); Smart
  playlists always contain a `field:` rule (e.g. `"genre:jazz"`). Smart
  playlists render in the **Custom Playlists** tab; system auto-playlists
  stay in **Auto Playlists**.

### 3. Auto-playlist correctness (found during manual testing, not in original scope but blocking it)

- Auto-playlists (genre/decade) are now forced to regenerate when their
  track count drifts from the fixed 25-song target, and existing
  under-threshold playlists are pruned rather than left stale.
  [playlist.rs]
- Genre/decade auto-playlists with fewer than 25 qualifying songs are
  skipped instead of created half-full.
- Added an info banner at the top of the Auto Playlists view explaining
  how auto-playlists are generated and refreshed.
- **Auto-Play → Auto-Refill rename**: renamed throughout the UI and both
  locale files (`en.ts`, `fr.ts`) for clarity — the setting controls
  whether an auto-playlist's contents refresh automatically, not playback.
- The "Auto" badge on `AutoPlaylistCard` and `PlaylistCard` (carousel) now
  only shows when Auto-Refill is actually enabled for that playlist,
  instead of unconditionally on every auto-playlist.

### 4. Custom columns & layout (frontend)

- **[CollectionView.svelte](src/lib/components/CollectionView.svelte)** — Columns popover to
  toggle `Format`, `Year`, `Genre`, `Bitrate`, `Rating`, `Play Count`,
  `Duration`, persisted to `localStorage` via `collectionStore`. Dedicated
  **Format** column (uppercase codec/extension). Clean "All results
  filtered out" empty state with a one-click "Reset Search & Filters"
  button.

### 5. Badges & card styling

- **[PlaylistCard.svelte](src/lib/components/PlaylistCard.svelte)** /
  **[AutoPlaylistCard.svelte](src/lib/components/AutoPlaylistCard.svelte)** — `Auto`/`Smart`
  badges moved to top-right, using theme accent tokens
  (`bg-brand-accent text-brand-accent-contrast`) instead of hardcoded
  colors. Smart playlists get a distinct purple-to-indigo gradient
  background.

## Files changed

18 files, +1215/-140 vs `main`. Backend: `filter_parser.rs` (new),
`collection.rs`, `playlist.rs`, `commands/playlist.rs`, `lib.rs`. Frontend:
`SmartPlaylistBuilderModal.svelte` (new), `CollectionView.svelte`,
`TopNavigation.svelte`, `PlaylistsCollectionView.svelte`, `PlaylistCard.svelte`,
`AutoPlaylistCard.svelte`, `collection.svelte.ts`, `playlists.svelte.ts`,
`filterParser.ts` (new), `+page.svelte`, `en.ts`, `fr.ts`,
`CollectionView.test.ts`.

## What's intentionally out of scope

- Grouping tracks by Album/Artist and the full sort-selector matrix
  described in the issue's design doc weren't built — this branch focused
  on filtering, columns, and the Smart Playlist pipeline. Worth a
  follow-up issue if still wanted.
- The `:` hint popover in `TopNavigation.svelte` for field-suggestion
  autocomplete wasn't added; the filter grammar works today by typing the
  full `field:value` syntax without inline help.
- Exclusive/bit-perfect audio routing is unrelated to this issue — not
  applicable here.

## Testing / Verification

- `cargo test` (backend, incl. BDD scenarios) — passes, 0 failures.
- `bun run check` (svelte-check) — 0 errors, 1 pre-existing warning
  (`SmartPlaylistBuilderModal.svelte:101`, a `state_referenced_locally`
  lint on an intentional one-time initial value — the subsequent
  `$effect` keeps it in sync reactively).
- `bun run test:run` — 224/224 passing. One test
  (`PlaylistsCollectionView.test.ts`) had stale mock data left over from
  before the categorization fix (used the old `"genre:Rock"` dynamic_spec
  convention for a *system* auto-playlist, which the new logic correctly
  reclassifies as a Smart playlist); updated the mock to the current
  `"Rock"` convention to match `playlist.rs`'s actual output.
- Not visually verified by me — per your stated preference, live
  verification is yours in `bun run tauri dev` rather than a browser
  preview or the screenshot harness. Your dev server for this worktree
  (`luminous.exe`) already appears to be running.

Worth clicking through:
- Search bar: try `genre:jazz`, `rating:>=4 year:<2000`, and a bare-word
  search — confirm all return sensible results and bad syntax doesn't
  error.
- Collection view: toggle columns in the Columns menu, confirm the Format
  column shows codec, and confirm the "all filtered out" empty state and
  its reset button.
- Playlists: build a Smart Playlist (e.g. `genre:rock`), confirm it's
  non-empty immediately, shows the suggested name, appears under **Custom
  Playlists** with the purple gradient + top-right Smart badge.
- Auto Playlists tab: confirm the new info banner, and that Auto badges
  only appear when Auto-Refill is toggled on for that playlist.

## Next steps for your approval

Once you've clicked through the above in your dev server and you're happy
with it, let me know and I'll merge this branch into `main` and close issue
#13.
