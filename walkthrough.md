# Walkthrough: Hide the player bar when nothing is playing (#71)

## Summary

The floating player bar (`PlayerBar.svelte`) no longer renders on first launch, before any track has ever been selected. It fades/slides in the moment a track starts playing, and — once shown in a session — stays visible even through pauses or an empty queue (matches the issue's requirement #5).

## Changes

- **[player.svelte.ts](src/lib/stores/player.svelte.ts)** — added a sticky `hasEverPlayed` flag on `PlayerStore`. It flips to `true` the first time `currentSong` is set (via the initial `get_playback_state` fetch, `playback-state` events, or `track-changed` events) and never resets, even if the queue empties out later in the session.
- **[+layout.svelte](src/routes/+layout.svelte)** — the floating PlayDock wrapper is now gated behind `{#if playerStore.hasEverPlayed}`, with a `fly` transition (slide up + fade, 300ms) for a smooth entrance instead of a jump cut.
- **[PlayerBar.svelte](src/lib/components/PlayerBar.svelte)** — removed the now-redundant inner `in:fade` transition on the `<footer>`, since the parent `{#if}` block in the layout owns the enter/exit animation.
- **Seven view files** — `pb-24` (the reserved bottom padding that kept content from being obscured by the always-on bar) is now conditional on `playerStore.hasEverPlayed` via `class:pb-24={...}`, so content expands to fill the freed space when the bar is hidden:
  - [AlbumDetailView.svelte](src/lib/components/AlbumDetailView.svelte)
  - [ArtistDetailView.svelte](src/lib/components/ArtistDetailView.svelte) (two spots — the playlists section and its empty-state spacer)
  - [CollectionView.svelte](src/lib/components/CollectionView.svelte)
  - [FoldersView.svelte](src/lib/components/FoldersView.svelte)
  - [HomeView.svelte](src/lib/components/HomeView.svelte)
  - [LyricsView.svelte](src/lib/components/LyricsView.svelte)
  - [PlaylistView.svelte](src/lib/components/PlaylistView.svelte)
- **[Sidebar.svelte](src/lib/components/Sidebar.svelte)** — the bottom scanning/"Rescan Library" section had the same hardcoded offset (`mb-24`, not `pb-24`) reserving space for the always-on bar, but wasn't in the issue's original file list. Fixed the same way — conditional on `playerStore.hasEverPlayed` — so it slides down to fill the sidebar when the bar is hidden.
- **[player.test.ts](src/lib/stores/player.test.ts)** — new unit test covering the sticky-latch behavior: `hasEverPlayed` starts `false`, flips to `true` on the first `track-changed` event with a song, and stays `true` after a later `track-changed` event clears the song.

## Verification performed

- `bun run check` — 0 errors.
- `bun run test:run` — 121/121 tests passing (including the new sticky-flag test).
- Manual browser verification (frontend dev server + mocked Tauri IPC layer): confirmed the player bar is absent on a fresh load with no track loaded, the songs list fills the freed space, the bar animates in on a simulated `track-changed` event, and it remains visible (showing "Nothing playing") after the track clears again. Spot-checked `HomeView` for the same padding behavior.
- Searched the whole `src/` tree for any other `pb-24`/`mb-24` offsets tied to the player bar — the 7 issue-listed views plus `Sidebar.svelte` were the complete set; nothing else missed.

## Note on this session

Partway through, the original worktree directory (`.claude/worktrees/issue-71-6c79ce`) was accidentally emptied. No commits were lost (nothing had been committed yet), but the in-progress uncommitted edits were — they were fully redone from scratch in this new worktree at `.worktrees/issue-71-6c79ce`, per your instruction to use `.worktrees/` instead of `.claude/worktrees/` going forward. The stale `.claude/worktrees/issue-71-6c79ce` directory is still on disk (now empty) — it's locked by this session's own process and couldn't be deleted from within the session; it'll need to be removed manually once this session ends.

## How to verify

`bun run tauri dev` is starting up now — once the window opens:
1. On first launch (no track ever played), confirm the player bar is absent and the content area fills the space.
2. Play any track — the bar should slide/fade in smoothly.
3. Pause, or let the queue run out — the bar should stay visible.
4. Check a few other views (Home, an album, a playlist, lyrics) to confirm nothing is hidden behind — or awkwardly gapped above — where the bar used to always be.
