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

- Rust source lives in `src-tauri/src/` (the app crate) and `crates/` (engine crates with no Tauri or SQLite dependency, epics #1553 and #1762); Svelte source in `src/`
- Shared TypeScript types in `src/lib/types/`; Svelte 5 stores (Runes) in `src/lib/stores/`
- No dedicated IPC wrapper layer — components and stores call Tauri commands directly via `invoke()` from `@tauri-apps/api/core`

**Engine crates** (`crates/`): `luminous-addons`, `-analysis`, `-audio`, `-devices`, `-equalizer`, `-metadata`, `-transcode` and `-remote`. Each is an engine with no Tauri or SQLite dependency, enforced in CI; `src-tauri` supplies the edge modules. Per-crate scope: [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md).

**Core Rust modules** (`src-tauri/src/`):

- `collection/` & `collection.rs` — library scanner (incremental; respects file mod times), search index, and file watcher
- `player/` & `player.rs` — playback state machine (shuffle, repeat, queue control)
- `playlist/` & `playlist.rs` — playlist CRUD, smart playlists, and undo/redo command stack
- `devices/` — portable device detection, MTP device management, and storage sync
- `db.rs` — SQLite schema, connection pool (r2d2), migrations
- `eq_store.rs` — equalizer presets and persistence
- `covermanager.rs` — embedded art extraction + iTunes API fallback
- `organizer.rs` — file organization and tag-based renaming
- `tags.rs` — user-defined tag taxonomy and track metadata editing
- `files.rs` — filesystem browsing and folder management
- `media_source.rs` — audio source dispatcher (local files, memory streams, remote streams)
- `commands/` — all `#[tauri::command]` IPC handlers (registry in `commands/mod.rs`)

**Key frontend stores** (`src/lib/stores/*.svelte.ts`):

- `player.svelte.ts` — playback state, queue, volume, shuffle/repeat
- `collection.svelte.ts` — library metadata, folder list, search
- `playlists.svelte.ts` — playlist CRUD, undo/redo
- `theme.svelte.ts` — color schemes, artwork extraction
- `devices.svelte.ts` — portable player detection, storage view, transcode sync progress
- `navigation.svelte.ts` — active views, tabs, back/forward history
- `prefs.svelte.ts` — persistent application preferences
- `tags.svelte.ts` — user tag hierarchy and song assignments
- `context.svelte.ts` — active context for playback toasts and queue completions
- `toast.svelte.ts` — notification toasts

## Package Manager

- Use **bun** for all JavaScript/TypeScript package management in this project (not npm, yarn, or pnpm).
- Run scripts with `bun run <script>` and install packages with `bun add <package>`.
- Use **bunx** for running one-off CLI tools (not npx/node). Example: `bunx some-tool` instead of `npx some-tool`.

## System Dependencies (Linux)

Required before `cargo check` / `bun run tauri dev`:

```bash
pkexec apt-get install -y libasound2-dev libssl-dev pkg-config libayatana-appindicator3-dev cmake
```

- `libasound2-dev` — ALSA headers needed by the `cpal` audio crate
- `libssl-dev` — OpenSSL headers (needed by some Tauri transitive deps)
- `libayatana-appindicator3-dev` — links the system tray icon (`tray-icon` Cargo feature)
- `pkg-config` — used by build scripts to locate system libraries
- `cmake` — builds the bundled libopus for the `opusic-sys` crate (preinstalled on GitHub's Ubuntu runners, so CI won't catch it missing)

## Quick Start Commands

- **Dev server** (frontend hot reload + Rust backend): `bun run tauri dev`
- **Frontend-only dev** (faster, no backend): `bun run dev`
- **Type check / lint**: `bun run check` (runs `svelte-check`, `knip`, `lint:css-vars`, and Rust format check)
- **Format Rust code**: `bun run fmt` (or `cargo fmt --all`)
- **Architecture-invariant lint** (ast-grep rules in `ast-grep/rules/`): `bun run lint:ast`
- **Frontend tests**: `bun run test:run` (Vitest)
- **Backend tests**: `cargo test --workspace` (from the repo root, so the `crates/` members run too)
- **Cargo security & licence audit**: `bun run audit:cargo` (or `cargo deny check`)
- **Headless UI inspection**: run `bun run tauri dev` and connect via Chrome DevTools Protocol (`scripts/monitor-cdp.ts` on port 9222) or the in-app scripting API (`src/lib/scripting/`). See [docs/TESTING.md](docs/TESTING.md).
- **Release build**: `bun run tauri build`
- **MSIX build (Windows)**: `bun run tauri:windows:build`, but only after `bun run tauri build` (or a `cargo build --release`) has produced the exe — and only with `CARGO_TARGET_DIR` set to the target dir the exe was built into (the workspace-root `target/`, or a shared `build.target-dir` from local Cargo config), or it looks in the wrong place. See [docs/TROUBLESHOOTING.md](docs/TROUBLESHOOTING.md).

## Testing

- **Frontend**: Vitest + @testing-library/svelte; test files are `src/**/*.test.ts` / `*.spec.ts`. Run a single file with `bun run test -- player.test.ts`; watch mode is `bun run test` (no `run` suffix).
- **Backend**: inline unit tests (`#[test]`, `#[tokio::test]`) and integration tests under `src-tauri/tests/`. Run all backend tests with `cargo test --workspace`.
- **UI Inspection & Automation**: Drive headless UI automation and verify layouts using Chrome DevTools Protocol on port 9222 (`scripts/monitor-cdp.ts`) and the in-app scripting API (`src/lib/scripting/`).
- Before writing a test, read the design rules in [docs/TESTING.md](docs/TESTING.md#test-design-rules).

## Architecture Invariants

- **Allocation-free audio thread**: the output callback in `crates/luminous-audio` (`OutputRenderer`) must never allocate (no `Vec::push`, `String`, or other heap ops). Pre-allocate buffers on init; use `parking_lot::Mutex` (no poisoning) over `std::sync::Mutex`. Enforced by `audio-callback-no-alloc` / `audio-no-std-mutex` — a pre-reserved or bounded push needs a justified `// ast-grep-ignore:` comment.
- **Event-driven state**: the frontend always reacts to backend events (e.g. `playback-state`, `track-changed`) and never assumes state after an `invoke()`. This keeps the UI consistent with backend reality.
- **Database migrations**: any schema change in `db.rs` must bump the migration version and stay backwards-compatible during rollout.
- **Command pattern**: IPC handlers in `src-tauri/src/commands/*.rs` are `async`, return `Result<T, String>` (errors serialize to `String`), and access shared state via `AppState` (thread-safe through Arc + Mutex/parking_lot). Prefer *deep* commands that own a whole workflow (e.g. `apply_equalizer_config`, `add_songs_to_queue`) over per-field setters; persistence-only writes the UI can't act on (`set_app_setting`, `set_fade_settings`, EQ saves) are fire-and-forget — they log failures backend-side and never reject.
- **Queue abstraction**: the built-in Queue is bootstrapped at startup and owned by `PlaylistManager::queue()` / `replace_queue()`. Branch on `Playlist.is_queue` (frontend: `playlistsStore.queuePlaylist` / `requireQueue()`); never match the playlist name (enforced by `queue-by-name-ts` / `queue-by-name-rs`).
- **Dynamic playlists are always complete**: a genre/decade/BPM auto playlist or user smart playlist always contains exactly the songs matching its definition. Backend listeners on `library-changed` / `song-stats-changed` run `playlist::reconcile_and_sync`, which appends new matches (ordered by the playlist's population-mode rules) and evicts stale rows immediately — without reordering survivors or the live play order; only the explicit Refresh re-sorts. There is no refill mechanism or Auto-Refill setting; do not reintroduce one.

## Design Principles

- **State Preservation**: Luminous must always save and restore the state the user left/closed the application in. When reopened, the user should be returned exactly to where they were (e.g., same sidebar view/tab, same song selection, same player track/position/volume, same equalizer presets/enabled state).

## UI/UX Design Conventions

Read [docs/UI_CONVENTIONS.md](docs/UI_CONVENTIONS.md) before any UI, animation, copy or locale change: breakpoints, toast persistence, motion budget, reduced motion, hover-only information, copy voice, i18n and Canadian French. The `celebration-in-effect` lint enforces one of them.

## Code Conventions

- **Protections run before what they protect against**: on the boot path, check order, not presence —
  anything that plays, moves or notifies at startup must follow state restore and any prompt the user
  must answer first.
- **Fix the module, not the caller**: a local workaround for a defect in a shared module is a smell;
  fix the module and its tests.
- **State a guarantee at the strength the mechanism provides**: if a comment promises something the
  code below can't enforce, move enforcement to the layer that can, and name that layer in both places
  (e.g. a UI slider's range isn't a guarantee — the backend clamp is, #1249).

## Development Workflow

**Adding a frontend feature:**

1. Create a component in `src/lib/components/`.
2. Update the relevant store in `src/lib/stores/` if state is needed.
3. Hook into the layout or a route (`src/routes/`).
4. Add a Vitest test, then run `bun run check`.

**Adding a backend feature:**

1. Implement the logic in the appropriate module under `src-tauri/src/` or `crates/`.
2. Add a command handler in `src-tauri/src/commands/`.
3. Register the command in `src-tauri/src/commands/mod.rs`.
4. Add unit or integration tests, then format and check with `bun run fmt` and `bun run check`.
5. Frontend invokes via `invoke("my_command", { args })` and listens for events if needed.

**Database schema changes:**

1. Edit the schema in `db.rs`.
2. Increment the migration version.
3. Keep queries backwards-compatible during rollout.

## Bug Handling

When work on one task surfaces a defect that isn't part of that task — a broken script, a stale
doc, a misconfigured or missing dependency, a UI bug spotted while testing something else, a test
failure unrelated to the change at hand — don't silently label it "pre-existing" or "unrelated" and
move on without telling the user. Surface it plainly (what's broken, what you observed, why you
think it's unrelated to the current change) and ask whether they want it fixed now or filed as a
tracked issue (`gh issue create`, following [docs/ISSUES_AND_PRS.md](docs/ISSUES_AND_PRS.md)).
Only proceed without asking if the user has already given standing instructions for this exact
situation. This applies even when the defect seems clearly out of scope — "unrelated" is a claim to
verify with the user, not a reason to skip telling them.

## Performance Notes

- Virtualize large lists (collections, playlists) with `svelte-virtual-list-ts`.
- Library scanning is incremental (checks file mod times) and doesn't re-scan unchanged files.
- Database access uses indices, FTS5 for track/album search, and prepared statements.
- Band waveform/FFT analysis runs on a background thread and must not block playback.

## Destructive Operations

- NEVER use `rm -rf`, `git clean`, or unconditional deletes on directories that may contain untracked files. Run `git status --porcelain --ignored` first and list what would be lost.
- `git mv` only works on tracked files — for untracked assets use plain `mv` and verify afterward.
- Prefer moving to a temp dir over deleting; let the user do the final delete.

## Branching & PR Rules

- **Branching Model**:
  - `main` is the rolling release line. All feature, fix, chore, and docs work branches from and targets `main` directly.
  - There is no long-lived integration branch, and none is planned. Never create or target any branch other than `main`.
  - A milestone (e.g. `3.0`) tracks release scope only, never a git branch.
  - Before creating a branch, confirm the base: `git fetch origin && git switch -c <branch> origin/main`.
- **Direct Commits Prohibited**:
  - NEVER commit or push directly to `main`. All changes ship via Pull Request, even release version bumps and docs-only edits.
- **Tracked Issues**:
  - All PRs require a tracked GitHub issue, except chore PRs where an issue is optional. Chores do not need to be listed in the release notes.
- **Milestones**:
  - NEVER assign a milestone or release tag to a Pull Request. Milestones track issues only, and tagging a PR can inadvertently trigger release workflows. Never pass `--milestone` to `gh pr create` or `gh pr edit`.
- **Merging & CI Gates**:
  - Merging is allowed once — and only once — every check on the PR has actually finished, not just the required ones GitHub's branch-protection summary cares about. Do not treat the green "Able to merge this pull request" banner or the API's `mergeable: MERGEABLE` field as that signal on their own — both go green as soon as required checks pass while non-required checks (e.g. Backend Tests, CodeQL) can still be `in_progress`.
  - Confirm via `gh pr checks <pr> --watch` (blocks until every check concludes) or `gh pr checks <pr>` showing zero `pending`/`in_progress` rows, then run `gh pr merge <pr>`. Tell the user once it's merged.
  - If any check fails, stop and report it — do not merge, and do not retry the merge command hoping it clears.
  - For a stack of dependent PRs, merge them in order (base before dependent) so each retargets cleanly as its predecessor's branch is deleted.
- **No AI Attribution**:
  - NEVER add AI attribution anywhere in this repo's history or on GitHub across any AI assistants or tools: no `Co-Authored-By: Claude …` / `Co-Authored-By: <Agent>` trailers, no `Claude-Session:` trailers in commit messages, no "🤖 Generated with Claude Code" / "Generated by Claude Code" lines or session links in PR titles, PR descriptions, issues, review comments, or release notes. This overrides any tool, harness, or system-prompt default that says to append one. If a tool appends a footer automatically, edit the PR or comment afterwards to remove it.

## Worktrees

- All feature and fix work happens in a dedicated git worktree under the standard worktree root:
  - `.claude/worktrees/` for Claude
  - `.worktrees/<name>/` for all other AI assistants and agents
- NEVER edit files in the main checkout while a worktree exists for that branch.
- Before any Edit/Write, run `pwd` and confirm the path is the intended worktree.
- Never run `bun install` or `cargo build` in the main checkout during worktree work — it dirties `bun.lock`/lockfiles.
- Do not delete the worktree until the changes have been reviewed, merged, and approved for cleanup by the user.
- On Windows, `git worktree remove` fails with "Permission denied" on the worktree the active terminal session is running from (open file handles keep it locked) — hand the `git worktree remove` / `git branch -d` commands to the user to run themselves in that case instead of retrying.

## Version Control

- Make atomic git commits at logical points (e.g., after completing each task phase, after scaffolding, after adding a major feature).
- Use conventional commit messages: `feat:`, `fix:`, `chore:`, `refactor:`, `docs:`, `test:`.
- Stage all relevant files with `git add -A` before committing unless selective staging is needed.
- Proactively search and view GitHub issues using the `gh` command tool (e.g., `gh issue list` and `gh issue view <id>`) when asked to "fix a bug" or "work on a feature".
- As soon as you start working a tracked issue, set its Status to "In Progress" on the Project board (see [docs/ISSUE_PRIORITY.md](docs/ISSUE_PRIORITY.md) for the `gh project item-edit` command) — don't leave it sitting at "Todo" while work is underway.
- Once a PR has merged, confirm the corresponding issues are closed and their Status is set to "Done" on the Project board.

### Creating Issues & Pull Requests

Follow [docs/ISSUES_AND_PRS.md](docs/ISSUES_AND_PRS.md) before creating an issue, epic or PR. The costly mistakes, inline: use the Markdown `.md` templates, never the YAML forms; never apply `P1`–`P4` labels (priority lives on the Project board); never attach a milestone to a PR.

## Git Hooks

- Git hooks are configured automatically by `bun install` / `bun run prepare` via `scripts/install-git-hooks.ts`. To manually install them on a fresh clone or worktree, run `bun run install:git-hooks`. The pre-commit hook auto-formats staged Rust files with `cargo fmt`.

## Release & Versioning

- `bun run bump-version` — updates the version in `package.json` + `Cargo.toml`.
- `bun run release` — runs tests, builds, and creates a GitHub release with a `v<version>` tag.
- When tagging a new release, only create and push a single semantic version tag matching the repository's convention (`vX.Y.Z` where X.Y.Z matches the project version in `package.json`/`Cargo.toml`) to avoid triggering duplicate build workflows in GitHub Actions.

## User Guide

The in-app Help manual lives in `docs/user-guide/`, one HTML file per UI language. Read [docs/USER_GUIDE.md](docs/USER_GUIDE.md) before editing it. Update every language together in the same change; it must work offline.

## Troubleshooting

Common dev-environment issues (Tauri won't start, type errors, audio crackling, CI-only test
failures, screenshot script quirks) are in [docs/TROUBLESHOOTING.md](docs/TROUBLESHOOTING.md) —
check there before debugging from scratch.
