# Walkthrough — Functional Back/Forward Navigation (#67)

Branch: `feature/67-back-forward-navigation`

## What this delivers

Issue #67 asked for the Back/Forward chevrons in `TopNavigation.svelte` to
actually navigate, browser-style, instead of just moving a dead index
counter. This wires them to a real navigation history stack:

- **History stack lives on `collectionStore`** (`collection.svelte.ts`),
  since that's already the source of truth for "where the user is":
  `activeTab`, `activeSubTab`, `playlistsSubTab`, `selectedArtistName`,
  `selectedAlbumName`, `selectedPlaylistId`, `selectedAutoPlaylist`.
- **Snapshots are recorded from the existing property setters**, not a new
  `$effect` — every one of those fields already has a getter/setter (used
  for `localStorage` persistence), so each setter now also calls
  `scheduleRecordHistory()`. Every navigation call site in the app already
  goes through these setters (`viewArtist`, `viewAlbum`, `viewPlaylist`,
  `viewAutoPlaylist`, `Sidebar.svelte`'s tab clicks, the detail views' own
  "back" links), so no other component needed to change.
- **Multi-field navigations collapse into one history entry.** A single
  action like `viewArtist()` writes several fields in sequence
  (`activeTab`, `activeSubTab`, `selectedArtistName`, clears search). Each
  setter call schedules a microtask-coalesced snapshot; since a synchronous
  function body runs to completion before any microtask fires, all those
  writes land in the *same* tick and produce exactly one history entry —
  not one per field.
- **Replaying history doesn't re-record itself.** `goBack()`/`goForward()`
  set an `isNavigatingHistory` guard while writing the snapshot back onto
  the store's setters, so applying a history entry doesn't push a new one.
  A `JSON.stringify` equality check in `recordHistory()` is a second,
  independent safety net against duplicate/stray entries.
- **New navigation truncates stale forward history** — going back and then
  navigating somewhere new drops the old "forward" branch, same as a web
  browser.
- **Capped at 50 entries** to avoid unbounded growth over a long session.
- **Seeded on boot** — `init()` now calls `recordHistory()` once after
  restoring the persisted `activeTab`/`activeSubTab`/etc. from
  `localStorage`, so Back/Forward have a starting point immediately after
  a relaunch instead of an empty stack.
- **Playlist detail views reload their tracks on replay** — `selectedPlaylistId`
  gates which view shows, but the actual track list comes from
  `playlistsStore.activePlaylistId`/`activePlaylistTracks`. Replaying a
  snapshot with a non-null `selectedPlaylistId` also calls
  `playlistsStore.selectPlaylist(id)` so Back/Forward into a playlist shows
  its real tracks, not stale ones.

## Files changed

- [src/lib/stores/collection.svelte.ts](src/lib/stores/collection.svelte.ts) —
  `NavigationView` snapshot type, `history`/`historyIndex` state,
  `canGoBack`/`canGoForward` getters, `goBack()`/`goForward()`, and the
  `scheduleRecordHistory()` hook added to each navigation-relevant setter.
- [src/lib/components/TopNavigation.svelte](src/lib/components/TopNavigation.svelte) —
  removed the dead local `historyStack`/`historyIndex` state and the
  "Would navigate to..." placeholder handlers; buttons now call
  `collectionStore.goBack()` / `goForward()` and disable based on
  `canGoBack` / `canGoForward`.
- [src/lib/stores/collection.test.ts](src/lib/stores/collection.test.ts) —
  two new tests: Back/Forward round-tripping through `viewArtist`/`viewAlbum`,
  and forward-history truncation after navigating anew from a Back'd-into
  state.

## What's intentionally out of scope

- The history *stack itself* isn't persisted across app restarts — only a
  single seed entry from the already-persisted view. Issue #67 called this
  optional ("at minimum... seed the stack with a single initial entry");
  full stack persistence felt like scope creep for what's fundamentally a
  same-session convenience feature.
- `playlistsStore.activePlaylistId` (which playlist's tracks are loaded,
  separate from `collectionStore.selectedPlaylistId` which gates the view)
  isn't itself part of the history snapshot — it's re-derived by calling
  `selectPlaylist()` on replay instead, so it can't drift from whichever
  playlist `selectedPlaylistId` says should be showing.

## Testing / Verification

- `bun run check` (svelte-check) — clean, no new errors.
- `bun run test` — full suite (219 tests) passes, including the 2 new
  history tests in `collection.test.ts`.
- Not visually verified in the running app — Luminous's `invoke()` calls
  need real Tauri IPC, so per your stated preference I'm leaving live
  verification to you in `bun run tauri dev` rather than using a browser
  preview or the screenshot harness. Worth clicking through: Home →
  Collection → an artist → an album → Playlists → a playlist, then Back
  through each step and Forward back to the playlist, and confirm a
  relaunch still has a working (if single-entry) history.

## Next steps for your approval

Once you've clicked through Back/Forward in your dev server and you're
happy with it, let me know and I'll merge this branch and close issue #67.
