# AGENTS.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Quick Start Commands

**Development server** (includes frontend hot reload + Rust backend compilation):
```bash
bun run tauri dev
```

**Frontend-only dev** (faster, without backend):
```bash
bun run dev
```

**Run all tests:**
```bash
bun run test:run              # Vitest frontend tests
cd src-tauri && cargo test    # Rust backend tests
cd src-tauri && cargo test --test equalizer_bdd  # Cucumber BDD tests
```

**Type checking & linting:**
```bash
bun run check              # One-time check
bun run check:watch        # Watch mode
```

**Build for release:**
```bash
bun run tauri build        # Creates installers for current OS
```

**Git hooks** (auto-formats staged Rust files):
```bash
bun run install:git-hooks  # Run once per clone to enable pre-commit hook
```

## Architecture Overview

Luminous is a **Tauri 2 desktop app** with a layered architecture:

### Frontend (Svelte 5 + TypeScript)
- **Location:** `src/`
- **Framework:** SvelteKit + Vite (static adapter, SPA mode)
- **State:** Svelte 5 Runes (reactive, fine-grained reactivity)
- **CSS:** Tailwind 4
- **Key stores** (in `src/lib/stores/*.svelte.ts`):
  - `player.svelte.ts` — playback state, queue, volume, shuffle/repeat
  - `collection.svelte.ts` — library metadata, folder list
  - `playlists.svelte.ts` — playlist CRUD, undo/redo
  - `theme.svelte.ts` — color schemes, artwork extraction

### Backend (Rust + Tauri 2)
- **Location:** `src-tauri/`
- **Core modules** (in `src-tauri/src/`):
  - `audio.rs` — Symphonia (decoding) + CPAL (output) pipeline with gapless playback
  - `player.rs` — playback state machine (shuffle, repeat, queue control)
  - `collection.rs` — library scanner (incremental, respects file mod times) + file watcher
  - `db.rs` — SQLite schema, connection pool (r2d2), migrations
  - `playlist.rs` — playlist CRUD + undo/redo command stack
  - `equalizer.rs` — biquad DSP filters (10-band graphic, 20-band parametric)
  - `analyzer.rs` — real-time FFT spectrum processing
  - `lyrics.rs` — LRCLIB + Lyrics.ovh clients
  - `covermanager.rs` — embedded art extraction + iTunes API fallback
  - `tageditor.rs` — lofty tag reader/writer + AcoustID fingerprinting
  - `commands/` — all `#[tauri::command]` IPC handlers

### IPC Bridge (Tauri Commands)
- Frontend invokes Rust via `invoke()` (async)
- Backend emits events back via listeners (e.g., `playback-state`, `track-changed`)
- See `src-tauri/src/commands/mod.rs` for full command registry

### State Flow
1. **Initialization:** Frontend calls `get_playback_state()` command → PlayerStore hydrated
2. **Backend events:** Audio thread emits `playback-position` every ~250ms; player state changes emit `playback-state`
3. **Frontend reactivity:** Svelte 5 Runes (fine-grained) automatically re-render only affected components
4. **Persistence:** Player state (volume, queue, playlist) saved to SQLite on each change

## Frontend Structure

### Layout & Views
- `src/routes/+layout.svelte` — Root shell (Sidebar, TopNav, RightPanel, PlayerBar)
- `src/routes/+page.svelte` — Main canvas (swaps between CollectionView, PlaylistView, etc.)
- `src/lib/components/` — Reusable components (MoodBar, Equalizer, LyricsView, etc.)

### Stores & State Management
All stores use **Svelte 5 Runes** (no external state library):
- Reactive variables declared with `$state`
- Computed values with `$derived`
- Effects with `$effect`
- Class-based stores instantiated at app startup (`player = new PlayerStore()`)

### Testing
- **Framework:** Vitest + @testing-library/svelte
- **Files:** `src/**/*.test.ts` and `src/**/*.spec.ts`
- **Setup:** `vitest.setup.ts` configures jsdom, mocks Tauri API
- **Run single test:** `bun run test -- player.test.ts`
- **Watch mode:** `bun run test` (no `run` suffix)

## Backend Structure

### Rust Edition & Tooling
- **Edition:** 2021
- **Formatting hook:** Pre-commit auto-runs `cargo fmt` on staged `.rs` files
- **Testing:** Unit tests inline (`#[cfg(test)]`), BDD tests in `features/` + `src-tauri/tests/`

### Command Pattern
All IPC handlers in `src-tauri/src/commands/*.rs` follow this pattern:
```rust
#[tauri::command]
pub async fn my_command(
    state: tauri::State<'_, AppState>,
    arg1: String,
) -> Result<ResponseType, String> {
    // Access: state.db, state.player, state.audio, state.playlists
}
```
- Commands are `async` (tokio runtime available)
- Errors must serialize to `String` (Tauri limitation)
- All state accessed via `AppState` (thread-safe via Arc + Mutex/parking_lot)

### Audio Pipeline
- **Decoding:** Symphonia crate (supports MP3, FLAC, WAV, AAC, Ogg)
- **Output:** CPAL with WASAPI Exclusive (Windows) / ALSA Direct (Linux) for bit-perfect playback
- **Playback thread:** Runs in background, decodes into ring buffer
- **Gapless:** Double-buffered queue with next track pre-loaded

### Database
- **Engine:** SQLite + r2d2 connection pool
- **Initialization:** `db.rs` handles schema creation + migrations
- **Access:** All queries via `AppState.db` (connection pooled)
- **Performance:** FTS5 (full-text search) for track/album queries; indices on common filters

## Development Workflow

### Adding a Frontend Feature
1. Create component in `src/lib/components/NewFeature.svelte`
2. Update relevant store in `src/lib/stores/` if state needed
3. Hook into layout or route (`src/routes/`)
4. Test with Vitest (`src/lib/components/NewFeature.test.ts`)
5. Run `bun run check` to catch type errors

### Adding a Backend Feature
1. Implement logic in appropriate module (`src-tauri/src/mymodule.rs`)
2. Create command handler in `src-tauri/src/commands/mycommand.rs`
3. Register command in `src-tauri/src/commands/mod.rs`
4. Write unit tests inline or in BDD (Cucumber)
5. Frontend invokes via `invoke("my_command", { args })` + listen for events if needed

### Database Schema Changes
1. Edit schema in `db.rs`
2. Increment migration version
3. All queries must be backwards-compatible during rollout (or coordinate with users)

### Testing Strategy
- **Frontend:** Unit test stores and components (Vitest)
- **Backend:** Unit test modules, BDD test workflows (Cucumber)
- **Integration:** Manual `bun run tauri dev` testing (hot reload for quick iteration)

## Key Design Patterns

### Allocation-Free Audio Thread
The playback thread (`audio.rs`) must **never allocate** on the audio callback:
- Pre-allocate ring buffers on init
- Use `parking_lot::Mutex` (no poisoning) instead of `std::sync::Mutex`
- No `Vec::push`, `String`, or other heap operations in the audio loop

### Command Stack for Undo/Redo
Playlist modifications use a command pattern (`playlist.rs`):
- Each mutation wrapped in a `Command` struct (e.g., `AddTrackCommand`)
- Commands stored in a LIFO stack for undo; forward stack for redo
- Persistent: commands can be replayed on restart if needed

### Event-Driven State Updates
Frontend always reacts to backend events, never assumes state:
- `invoke("play_song")` doesn't immediately update `playerStore.state`
- Backend emits `playback-state` event → frontend listener updates store
- Guarantees consistency between frontend display and backend reality

## Performance Notes

### Frontend
- **Virtualization:** Use `svelte-virtual-list-ts` for large lists (collections, playlists)
- **Runes:** Svelte 5 automatically tracks dependencies; avoid unnecessary `$derived` chains
- **Tailwind:** JIT compilation; unused classes pruned in production

### Backend
- **Library scanning:** Incremental (checks file mod times); doesn't re-scan unchanged files
- **Database queries:** Use indices; FTS5 for search; prepared statements to avoid reparse overhead
- **FFT analysis:** Moodbar runs in background thread; doesn't block playback

## CI/CD & Release

### Pre-commit Hook
Automatically formats staged Rust files:
```bash
bun run install:git-hooks  # Enable once per clone
```

### Version Bumping & Release
```bash
bun run bump-version       # Updates version in package.json + Cargo.toml
bun run release            # Runs tests, builds, creates GitHub release with v<version> tag
```

Release tag format: `v0.50.0` (matches semantic versioning in package.json + Cargo.toml)

### Supported Platforms
- **Windows** (MSVC toolchain required; see README for setup)
- **Linux** (Ubuntu/Debian packages; GTK, WebKit2, ALSA dev headers)
- **macOS** (build supported but not actively tested)

## Dependencies & Notable Crates

**Frontend:**
- `@tauri-apps/api` — IPC bridge
- `@testing-library/svelte` — Component testing
- `svelte-virtual-list-ts` — Large list virtualization

**Backend:**
- `symphonia` — Audio decoding (all codecs)
- `cpal` — Cross-platform audio output
- `rusqlite` + `r2d2` — SQLite with connection pooling
- `lofty` — Tag reading/writing
- `rustfft` — Spectrum analysis
- `tokio` — Async runtime
- `cucumber` — BDD tests

## Troubleshooting

**Tauri dev won't start:**
- Ensure git hook is installed: `bun run install:git-hooks`
- Check Rust toolchain: `cargo --version`
- Clear Tauri cache: `rm -rf src-tauri/target`

**Frontend type errors after dependency update:**
- Run `bun run check` (Svelte type checking includes runtime validation)
- Verify store usage: Svelte 5 Runes don't need destructuring ($-prefix variables are reactive)

**Audio playback crackling/stuttering:**
- Check `audio.rs` — ensure no allocations in the playback loop
- Verify WASAPI Exclusive mode enabled (Windows) for bit-perfect output
- Profile with `cargo flamegraph` if CPU-bound

**Tests failing in CI but passing locally:**
- Vitest in CI uses jsdom (not browser); confirm jsdom-compatible selectors/methods
- Rust tests: check for platform-specific code (especially file paths)
