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
