# Luminous Music Player

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
- **Command pattern**: IPC handlers in `src-tauri/src/commands/*.rs` are `async`, return `Result<T, String>` (errors serialize to `String`), and access shared state via `AppState` (thread-safe through Arc + Mutex/parking_lot).

## Design Principles

- **State Preservation**: Luminous must always save and restore the state the user left/closed the application in. When reopened, the user should be returned exactly to where they were (e.g., same sidebar view/tab, same song selection, same player track/position/volume, same equalizer presets/enabled state).
- See [DESIGN.md](../DESIGN.md)

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
- Moodbar/FFT analysis runs on a background thread and must not block playback.

## Version Control

- Make atomic git commits at logical points (e.g., after completing each task phase, after scaffolding, after adding a major feature).
- Use conventional commit messages: `feat:`, `fix:`, `chore:`, `refactor:`, `docs:`, `test:`.
- Stage all relevant files with `git add -A` before committing unless selective staging is needed.
- Proactively search and view GitHub issues using the `gh` command tool (e.g., `gh issue list` and `gh issue view <id>`) when asked to "fix a bug" or "work on a feature".
- When working on a bug or feature, always work in a dedicated git worktree. Do not merge the temporary branch or delete the worktree until the user has approved the changes.
- Present the Walkthrough (`walkthrough.md`) to the user and wait for their explicit feedback and approval before merging. When presenting the Walkthrough, also spin up the Tauri development server (`bun run tauri dev`) so the user can manually verify changes interactively instead of relying solely on screenshots.
- Only after the user has reviewed the Walkthrough and approved the changes may you merge the temporary branch, clean up (remove) the worktree, and update/comment on and close the relevant GitHub issues using the `gh` CLI tool. Note that an issue must not be closed until the corresponding changes have been successfully merged into the target branch.
- **Creating Issues**: When asked to create a bug/issue on GitHub:
  1. Inspect the relevant templates under `.github/ISSUE_TEMPLATE/` (e.g., `bug_report.md`, `feature_request.md`).
  2. Perform a codebase search or analysis to fill out the template's sections (Description, Root Cause Analysis, Affected Components & Code Locations, Proposed Solution) accurately.
  3. Write the issue body to a temporary scratch file in the workspace or the artifacts scratch directory.
  4. Create the issue using the GitHub CLI: `gh issue create --title "<Title>" --body-file "<PathToScratchFile>" --label "<Label>" --milestone "<Milestone>"`.
  5. Verify the created issue by running `gh issue view <id>`.
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
