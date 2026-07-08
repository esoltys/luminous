# Luminous Agent Rules

## Author
- **Name**: Eric James Soltys
- **Email**: ericjamessoltys@outlook.com

## System Dependencies (Linux)
Required before `cargo check` / `bun run tauri dev`:
```bash
pkexec apt-get install -y libasound2-dev libssl-dev pkg-config
```
- `libasound2-dev` — ALSA headers needed by the `cpal` audio crate
- `libssl-dev` — OpenSSL headers (needed by some Tauri transitive deps)
- `pkg-config` — used by build scripts to locate system libraries

## Version Control
- Always initialize a git repository for new projects.
- Make atomic git commits at logical points (e.g., after completing each task phase, after scaffolding, after adding a major feature).
- Use conventional commit messages: `feat:`, `fix:`, `chore:`, `refactor:`, `docs:`, `test:`.
- Stage all relevant files with `git add -A` before committing unless selective staging is needed.
- Proactively search and view GitHub issues using the `gh` command tool (e.g., `gh issue list` and `gh issue view <id>`) when asked to "fix a bug" or "work on a feature".
- Always update/comment on and close the relevant GitHub issues using the `gh` CLI tool before completing the task. Note that an issue must not be closed until the corresponding changes have been successfully merged into the target branch.

## Package Manager
- Use **bun** for all JavaScript/TypeScript package management in this project (not npm, yarn, or pnpm).
- Run scripts with `bun run <script>` and install packages with `bun add <package>`.

## Tech Stack
- **Frontend**: SvelteKit + Svelte 5 (Runes) + TypeScript + Tailwind CSS v4
- **Backend**: Rust + Tauri v2
- **Database**: SQLite via rusqlite + r2d2
- **Audio**: Symphonia (decode) + CPAL (output)

## Project Structure
- Rust source lives in `src-tauri/src/`
- Svelte source lives in `src/`
- Shared TypeScript types in `src/lib/types/`
- Tauri IPC wrappers in `src/lib/ipc/`
- Svelte 5 stores in `src/lib/stores/`

## Design Principles
- **State Preservation**: Luminous must always save and restore the state the user left/closed the application in. When reopened, the user should be returned exactly to where they were (e.g., same sidebar view/tab, same song selection, same player track/position/volume, same equalizer presets/enabled state).
