# Luminous Music Player

## Product Scope

Luminous is not trying to be a best-of-all-worlds app. It covers the tagging, library, and
playback needs of most users well, but it deliberately does not chase feature parity with
specialist tools. For example: if a user needs heavy-duty batch tagging (AcoustID-driven
bulk retagging, complex fingerprint-based lookups, tag scripting), the answer is to direct
them to a dedicated tool like MusicBrainz Picard rather than building that depth into
Luminous. When scoping a feature request, prefer "good enough, well-integrated" over
matching a specialist tool's full depth — and when a request is clearly outside that bar,
recommend the existing dedicated tool instead of expanding scope.

This is not a blanket "no tagging features" rule — the line is *canonical lookup vs. personal
curation*. Picard's job is resolving a file against MusicBrainz's canonical database: correct
artist/album/track identity, official release metadata, AcoustID fingerprint matching, bulk
retagging a whole library against that source of truth. That's squarely out of scope for
Luminous. But organizing music *the user's own way* — layered on top of whatever canonical tags
already exist — is core to what a personal library manager should do, and belongs in Luminous
natively. Example: MusicBrainz has no opinion on whether a track is Folk Metal, Progressive
Metal, or Symphonic Metal — genre is often too broad and any subgenre taxonomy is inherently
personal/subjective. A user-defined tag system for exactly this (see #224) isn't scope creep
into Picard's territory; it's a different, complementary axis Picard was never meant to cover.

## Tech Stack

- **Frontend**: SvelteKit + Svelte 5 (Runes) + TypeScript + Tailwind CSS v4
- **Backend**: Rust (edition 2021) + Tauri v2
- **Database**: SQLite via rusqlite + r2d2
- **Audio**: Symphonia (decode) + CPAL (output)

## Project Structure

- Rust source lives in `src-tauri/src/`; Svelte source in `src/`
- Shared TypeScript types in `src/lib/types/`; Svelte 5 stores (Runes) in `src/lib/stores/`
- No dedicated IPC wrapper layer — components and stores call Tauri commands directly via `invoke()` from `@tauri-apps/api/core`

**Key frontend stores** (`src/lib/stores/*.svelte.ts`):

- `player.svelte.ts` — playback state, queue, volume, shuffle/repeat
- `collection.svelte.ts` — library metadata, folder list
- `playlists.svelte.ts` — playlist CRUD, undo/redo
- `theme.svelte.ts` — color schemes, artwork extraction

**Core Rust modules** (`src-tauri/src/`):

- `audio.rs` — Symphonia decode + CPAL output pipeline with gapless playback
- `player.rs` — playback state machine (shuffle, repeat, queue control)
- `collection.rs` — library scanner (incremental; respects file mod times) + file watcher
- `db.rs` — SQLite schema, connection pool (r2d2), migrations
- `playlist.rs` — playlist CRUD + undo/redo command stack
- `equalizer.rs` — biquad DSP filters (10-band graphic, 20-band parametric)
- `analyzer.rs` — real-time FFT spectrum processing
- `lyrics.rs` — LRCLIB + Lyrics.ovh clients
- `covermanager.rs` — embedded art extraction + iTunes API fallback
- `tageditor.rs` — lofty tag reader/writer + AcoustID fingerprinting
- `commands/` — all `#[tauri::command]` IPC handlers (registry in `commands/mod.rs`)

## Package Manager

- Use **bun** for all JavaScript/TypeScript package management in this project (not npm, yarn, or pnpm).
- Run scripts with `bun run <script>` and install packages with `bun add <package>`.
- Use **bunx** for running one-off CLI tools (not npx/node). Example: `bunx some-tool` instead of `npx some-tool`.

## System Dependencies (Linux)

Required before `cargo check` / `bun run tauri dev`:

```bash
pkexec apt-get install -y libasound2-dev libssl-dev pkg-config
```

- `libasound2-dev` — ALSA headers needed by the `cpal` audio crate
- `libssl-dev` — OpenSSL headers (needed by some Tauri transitive deps)
- `pkg-config` — used by build scripts to locate system libraries

## Quick Start Commands

- **Dev server** (frontend hot reload + Rust backend): `bun run tauri dev`
- **Frontend-only dev** (faster, no backend): `bun run dev`
- **Type check / lint**: `bun run check`
- **Frontend tests**: `bun run test:run` (Vitest)
- **Backend tests**: `cd src-tauri && cargo test`
- **Release build**: `bun run tauri build`

## Testing

- **Frontend**: Vitest + @testing-library/svelte; test files are `src/**/*.test.ts` / `*.spec.ts`. Run a single file with `bun run test -- player.test.ts`; watch mode is `bun run test` (no `run` suffix).
- **Backend**: inline unit tests (`#[cfg(test)]`) plus Cucumber BDD in `features/` + `src-tauri/tests/`. Run BDD suites like `cargo test --test equalizer_bdd`.

## Architecture Invariants

- **Allocation-free audio thread**: the playback callback in `audio.rs` must never allocate (no `Vec::push`, `String`, or other heap ops). Pre-allocate buffers on init; use `parking_lot::Mutex` (no poisoning) over `std::sync::Mutex`.
- **Event-driven state**: the frontend always reacts to backend events (e.g. `playback-state`, `track-changed`) and never assumes state after an `invoke()`. This keeps the UI consistent with backend reality.
- **Database migrations**: any schema change in `db.rs` must bump the migration version and stay backwards-compatible during rollout.
- **Command pattern**: IPC handlers in `src-tauri/src/commands/*.rs` are `async`, return `Result<T, String>` (errors serialize to `String`), and access shared state via `AppState` (thread-safe through Arc + Mutex/parking_lot). Prefer *deep* commands that own a whole workflow (e.g. `apply_equalizer_config`, `add_songs_to_queue`) over per-field setters; persistence-only writes the UI can't act on (`set_app_setting`, `set_fade_settings`, EQ saves) are fire-and-forget — they log failures backend-side and never reject.
- **Queue abstraction**: the built-in Queue is bootstrapped at startup and owned by `PlaylistManager::queue()` / `replace_queue()`. Branch on `Playlist.is_queue` (frontend: `playlistsStore.queuePlaylist` / `requireQueue()`); never match the playlist name.
- **Dynamic playlists are always complete**: a genre/decade/BPM auto playlist or user smart playlist always contains exactly the songs matching its definition. Backend listeners on `library-changed` / `song-stats-changed` run `playlist::reconcile_and_sync`, which appends new matches (ordered by the playlist's population-mode rules) and evicts stale rows immediately — without reordering survivors or the live play order; only the explicit Refresh re-sorts. There is no refill mechanism or Auto-Refill setting; do not reintroduce one.

## Design Principles

- **State Preservation**: Luminous must always save and restore the state the user left/closed the application in. When reopened, the user should be returned exactly to where they were (e.g., same sidebar view/tab, same song selection, same player track/position/volume, same equalizer presets/enabled state).
- See [DESIGN.md](../DESIGN.md)

## UI/UX Design Conventions

- **Toast persistence**: Toasts must never auto-dismiss unless an explicit `durationMs` is passed by the
  caller. By default, toasts stay visible until the user clicks the `X` button.

- **Explicit-action-only celebrations**: Micro-animations (heart pulse, confetti, etc.) must fire only in
  response to a deliberate user interaction (e.g., a click handler). Never trigger them from a reactive
  `$effect` or a prop-change watcher — doing so causes false positives when the track changes and a
  different (already-favourited) song loads.

- **Context-aware completion messages**: End-of-queue or completion toasts must include the name of what
  finished (e.g., "Jazz Classics complete"), not generic text. The `playerStore.activeContextName` field
  carries this name; auto-playlists (Favourites, Recently Added, etc.) must pass their `displayName` to
  `playerStore.playSongs()` so the context propagates correctly.

- **Icon semantics**: Avoid icons that imply system-level tracking or achievement recording (e.g.,
  `<Trophy>`). For milestone/completion moments, prefer neutral icons like `<Star>` that convey
  "special" without implying a leaderboard or achievement system.

- **Instructive, not descriptive, copy voice**: Hint/help text under a toggle, button, or tab should
  tell the user what to do or what happens when they act ("Scan watched folders for new files"), not
  describe the feature in third person ("Scans watched folders for new files") or lead with an adverb
  ("Automatically scans..."). Match the imperative voice of the control's own label. This applies to
  `src/lib/locales/en.ts` hint/tooltip strings specifically — section headings and status/value labels
  are fine as descriptive text.

## Development Workflow

**Adding a frontend feature:**

1. Create a component in `src/lib/components/`.
2. Update the relevant store in `src/lib/stores/` if state is needed.
3. Hook into the layout or a route (`src/routes/`).
4. Add a Vitest test, then run `bun run check`.

**Adding a backend feature:**

1. Implement the logic in the appropriate module under `src-tauri/src/`.
2. Add a command handler in `src-tauri/src/commands/`.
3. Register the command in `src-tauri/src/commands/mod.rs`.
4. Add unit or BDD tests.
5. Frontend invokes via `invoke("my_command", { args })` and listens for events if needed.

**Database schema changes:**

1. Edit the schema in `db.rs`.
2. Increment the migration version.
3. Keep queries backwards-compatible during rollout.

## Performance Notes

- Virtualize large lists (collections, playlists) with `svelte-virtual-list-ts`.
- Library scanning is incremental (checks file mod times) and doesn't re-scan unchanged files.
- Database access uses indices, FTS5 for track/album search, and prepared statements.
- Band waveform/FFT analysis runs on a background thread and must not block playback.

## Branching Model

- **`main`** is the rolling 1.x release line. Milestone-1 fixes (and any 1.x feature work) land
  here and ship as rolling releases (1.1, 1.25, 1.44, etc.) per `docs/RELEASE_CHECKLIST.md`.
- **`next`** is the long-lived 2.0 feature-integration branch. New stories tracked under the
  "2.0" GitHub Milestone (see Issue Priority Labels below) target PRs at `next`, not `main`.
  `next` is a branch name — don't confuse it with the "2.0" Milestone used for issue triage;
  they're two different things that happen to be about the same release.
- Keep `next` current by periodically merging `main` into it (`git merge origin/main`, no
  automated sync) — at minimum before starting a new round of 2.0 work, and always right before
  eventually merging `next` back into `main`.
- When the "2.0" Milestone's issues are done, `next` merges into `main` via PR and becomes the
  new baseline. A fresh `next` (or renamed successor) gets cut for whatever comes after that.

## Issue Priority Labels

Open issues in the "2.0" milestone carry a `P1`–`P4` label indicating priority for work after
1.0 ships. Use this scheme when picking up 2.0 work or triaging a new issue into it:

- **P1** — real (non-cosmetic) bugs, plus foundational/first-run work that other issues depend on.
- **P2** — features touched often, expected for parity with comparable desktop music players, or
  that encourage exploring the library (cover art, artist bios and connections, browsing by
  genre) rather than just playing tracks.
- **P3** — lower-frequency or power-user features, and cosmetic/low-severity bugs.
- **P4** — speculative, large-scope, or low-demand work; revisit after core 2.0 features ship.

Pull current state with `gh issue list --milestone "2.0" --label P1` (swap the label for
P2–P4). Don't default new issues to P2/P3 — assign a label using the same criteria above.

## Version Control

- Make atomic git commits at logical points (e.g., after completing each task phase, after scaffolding, after adding a major feature).
- Use conventional commit messages: `feat:`, `fix:`, `chore:`, `refactor:`, `docs:`, `test:`.
- Stage all relevant files with `git add -A` before committing unless selective staging is needed.
- Proactively search and view GitHub issues using the `gh` command tool (e.g., `gh issue list` and `gh issue view <id>`) when asked to "fix a bug" or "work on a feature".
- When working on a bug or feature, always work in a dedicated git worktree. Note that Claude uses its own worktree flow in `.claude/worktrees/`, while all other AI assistants and agents must place their dedicated worktree in the `.worktrees/` directory (e.g., `.worktrees/<feature-or-bug-name>`). Do not merge the temporary branch or delete the worktree until the user has approved the changes.
- Present the Walkthrough (`walkthrough.md`) to the user and wait for their explicit feedback and approval before merging. Do NOT run `bun run tauri dev` as a background task (it does not work as expected). Ask the user to run the dev server (`bun run tauri dev`) and check manually.
- Only after the user has reviewed the Walkthrough and approved the changes may you merge the temporary branch, clean up (remove) the worktree, and update/comment on and close the relevant GitHub issues using the `gh` CLI tool. Note that an issue must not be closed until the corresponding changes have been successfully merged into the target branch.
- **Creating Issues & Pull Requests**:
  1. Inspect the relevant templates under `.github/ISSUE_TEMPLATE/` (e.g., `bug_report.md`, `feature_request.md`) when creating issues.
  2. Inspect `.github/PULL_REQUEST_TEMPLATE.md` when preparing or creating Pull Requests.
  3. Perform a codebase search or analysis to fill out the template's sections (Description, Root Cause Analysis, Affected Components & Code Locations, Proposed Solution) accurately.
  4. Write the issue or PR body to a temporary scratch file in the workspace or the artifacts scratch directory.
  5. Create the issue using the GitHub CLI:
     - For bugs: `gh issue create --title "<Title>" --body-file "<PathToScratchFile>" --label "bug" --milestone "<Milestone>"`
     - For features: `gh issue create --title "<Title>" --body-file "<PathToScratchFile>" --milestone "<Milestone>"` (no label needed)
  6. Verify the created issue by running `gh issue view <id>`.
- **Releases & Tagging**: When tagging a new release, only create and push a single semantic version tag matching the repository's convention (e.g., `vX.Y.Z` where X.Y.Z matches the project version in `package.json`/`Cargo.toml`) to avoid triggering duplicate build workflows in GitHub Actions.

## Git Hooks

- Run `bun run install:git-hooks` once per clone to enable the pre-commit hook, which auto-formats staged Rust files with `cargo fmt`.

## Release & Versioning

- `bun run bump-version` — updates the version in `package.json` + `Cargo.toml`.
- `bun run release` — runs tests, builds, and creates a GitHub release with a `v<version>` tag.
- Tag format is `vX.Y.Z`, matching the version in `package.json` / `Cargo.toml`. Push only a single semver tag to avoid triggering duplicate build workflows.

## Notable Crates

- **Frontend**: `@tauri-apps/api` (IPC bridge), `@testing-library/svelte`, `svelte-virtual-list-ts`.
- **Backend**: `symphonia` (decode), `cpal` (output), `rusqlite` + `r2d2` (SQLite pool), `lofty` (tags), `rustfft` (spectrum), `tokio` (async), `cucumber` (BDD).

## Troubleshooting

- **Tauri dev won't start**: ensure the git hook is installed (`bun run install:git-hooks`), check the Rust toolchain (`cargo --version`), or clear the Tauri cache (`rm -rf src-tauri/target`).
- **Frontend type errors after a dependency update**: run `bun run check`. Svelte 5 Runes don't need destructuring (`$`-prefixed variables are already reactive).
- **Audio playback crackling/stuttering**: verify no allocations happen in the `audio.rs` playback loop; profile with `cargo flamegraph` if CPU-bound.
- **Tests fail in CI but pass locally**: Vitest in CI uses jsdom (not a browser) — confirm jsdom-compatible selectors; for Rust, check for platform-specific code (especially file paths).
