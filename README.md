<p align="center">
  <img width="150" height="150" src="app-icon.png" alt="Luminous">
</p>

<h1 align="center">Luminous</h1>
<p align="center">A high-performance home for the music you already own.</p>
<p align="center">
  <a href="https://esoltys.dev/luminous/"><strong>Luminous Homepage</strong></a>
</p>

<p align="center">

[![Rust](https://img.shields.io/badge/Rust-ea4335?style=flat-square&logo=rust&logoColor=white)](https://www.rust-lang.org)
[![Tauri](https://img.shields.io/badge/Tauri-24c8db?style=flat-square&logo=tauri&logoColor=white)](https://tauri.app)
[![TypeScript](https://img.shields.io/badge/TypeScript-3178c6?style=flat-square&logo=typescript&logoColor=white)](https://www.typescriptlang.org)
[![Svelte](https://img.shields.io/badge/Svelte-ff3e00?style=flat-square&logo=svelte&logoColor=white)](https://svelte.dev)

</p>

Luminous is a fast, local-first player for your own audio library. Built for anyone who wants the convenience of a modern streaming app without giving up ownership of their music. Turn a folder of files into a library you'll enjoy browsing and listening to.

**[Issues](https://github.com/esoltys/luminous/issues)** - file a bug report or a feature request

**[r/LuminousMusicPlayer](https://www.reddit.com/r/LuminousMusicPlayer/)** - announcements, general discussion, Q&A

Luminous doesn't accept PRs as this is a hobby project that I enjoy working on in my spare time.

## Quick Install

[<img src="https://get.microsoft.com/images/en-us%20dark.svg" width="200"/>](https://apps.microsoft.com/detail/9PNQ2NFSQ7XW?launch=true&mode=full)

| Platform    | Download |
| ----------- | -------- |
| **Windows** | [Microsoft Store](https://apps.microsoft.com/detail/9PNQ2NFSQ7XW) (recommended — installs and updates automatically)<br>[.exe / .msix](https://github.com/esoltys/luminous/releases/latest) (manual/sideloaded install)<br>[Portable .zip](https://github.com/esoltys/luminous/releases/latest) (no install; keeps your library and settings in a `data` folder beside the app) |
| **Linux**   | [.deb / .rpm / Flatpak](https://github.com/esoltys/luminous/releases/latest) for your distro<br>[AppImage](https://github.com/esoltys/luminous/releases/latest) (runs on any distro; add an empty `portable` file beside it for portable mode) |

## Features

- **Library**: indexes local folders with instant search, including `field:value` syntax (e.g. `genre:rock`) you can save as smart playlists
- **Playback**: gapless MP3, FLAC, Opus, WAV and more; 10-band graphic and 20-band parametric EQ with AutoEq headphone profiles; ASIO output on Windows
- **Remote sources**: OpenSubsonic servers (Navidrome, Gonic, …) and WebDAV, synced into the same library
- **Playlists**: smart and auto playlists (decades, genres, BPM), Song and Artist Radio built from your own library, custom genre hierarchies
- **Artist and album pages**: filled in from MusicBrainz and fanart.tv, with an info sidebar for whatever you're viewing
- **Lyrics**: time-synced line by line or word by word, with inline editing and a miniplayer
- **Tagging and organizing**: tag editor for single or bulk edits (with MusicBrainz Picard handoff) and automatic file organization from your own naming template
- **Loves, ratings and stats**: 1–5 stars and loves synced with ListenBrainz, Last.fm scrobbling, listening streaks and top artists
- **Your choice of look**: preset and cover-art-driven themes, seven languages, portable mode

See the [release notes](https://github.com/esoltys/luminous/releases) for what's new in each version.

---

## Building Luminous

Luminous is a cross-platform application that can be built and run on both Linux and Windows.

### Linux (Ubuntu/Debian, Arch/CachyOS)

#### 1. Install System Dependencies
Ensure the required build tools, GTK, WebKit, ALSA, and SSL development headers are installed:

*   **Ubuntu/Debian**:
    ```bash
    sudo apt update
    sudo apt install -y build-essential curl wget file libssl-dev libgtk-3-dev libwebkit2gtk-4.1-dev libsoup-3.0-dev libayatana-appindicator3-dev librsvg2-dev libasound2-dev pkg-config cmake
    ```
*   **Arch/CachyOS**:
    ```bash
    sudo pacman -S --needed base-devel curl wget file openssl gtk3 webkit2gtk-4.1 libappindicator-gtk3 librsvg pkg-config alsa-lib cmake
    ```

#### 2. Install Bun & Rust
*   **Bun**: Install the JavaScript runtime & package manager:
    ```bash
    curl -fsSL https://bun.sh/install | bash
    ```
*   **Rust**: Install the Rust toolchain:
    ```bash
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
    ```

#### 3. Run Development Server

```bash
bun install
bun run tauri dev
```
#### 4. Build Production Bundle
Production bundles include updater artifacts, which must be signed. If you haven't generated a signing keypair yet, see [Generating an Updater Signing Key](#generating-an-updater-signing-key) below first.
```bash
bun run tauri build
```
This produces `.deb`/`.rpm`/AppImage on Linux.

---

### Windows

#### 1. Install Microsoft C++ Build Tools
Download and install the [Visual Studio Installer](https://visualstudio.microsoft.com/visual-cpp-build-tools/). Select the **Desktop development with C++** workload and ensure the MSVC C++ build tools are checked.

#### 2. Install Bun & Rust
Install the JavaScript runtime, package manager, and Rust toolchain

```powershell
winget install Oven-sh.Bun Rustlang.Rustup
```

#### 3. Run Development Server
Run the following commands in your terminal (e.g., PowerShell or Command Prompt):
```powershell
bun install
bun run tauri dev
```

#### 4. Build Production Bundle
Production bundles include updater artifacts, which must be signed. If you haven't generated a signing keypair yet, see [Generating an Updater Signing Key](#generating-an-updater-signing-key) below first.
```powershell
bun run tauri build
```

---

### Environment Variables

None are needed for `bun run tauri dev`. Set them in your shell before building; nothing reads a `.env` file. The API keys are compiled in when set at build time and can also be overridden at runtime; a key entered in the app's Settings takes priority over both.

| Variable | Used for | If unset |
| --- | --- | --- |
| `TAURI_SIGNING_PRIVATE_KEY`, `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` | Signing updater artifacts in `bun run tauri build` (read by the Tauri CLI) | The build fails; see [Generating an Updater Signing Key](#generating-an-updater-signing-key) |
| `FANART_API_KEY` | fanart.tv artist images | fanart.tv images are unavailable unless a key is entered in Settings |
| `MUSICBRAINZ_CLIENT_ID`, `MUSICBRAINZ_CLIENT_SECRET` | MusicBrainz sign-in | The built-in client is used |
| `LASTFM_API_KEY`, `LASTFM_API_SECRET` | Last.fm scrobbling | The built-in key is used |

---

### Generating an Updater Signing Key

Luminous's `tauri.conf.json` has `bundle.createUpdaterArtifacts` set to `true`, so `bun run tauri build` signs every bundle it produces. Without a signing key in the environment, the build fails. Official releases are signed with a repository secret (see [`.github/workflows/release.yml`](.github/workflows/release.yml)), but for local builds you need your own keypair:

1. Generate a keypair (you'll be prompted to set a password, or pass `--ci` to skip it):
    ```bash
    bunx tauri signer generate -w ~/.tauri/luminous.key
    ```
    This writes the private key to `~/.tauri/luminous.key` and prints the corresponding public key.
2. Set the private key and its password as environment variables before building:
    ```bash
    export TAURI_SIGNING_PRIVATE_KEY="$(cat ~/.tauri/luminous.key)"
    export TAURI_SIGNING_PRIVATE_KEY_PASSWORD="your-password"
    ```
    On Windows (PowerShell):
    ```powershell
    $env:TAURI_SIGNING_PRIVATE_KEY = Get-Content -Raw "$HOME\.tauri\luminous.key"
    $env:TAURI_SIGNING_PRIVATE_KEY_PASSWORD = "your-password"
    ```
3. Run `bun run tauri build` as usual. Your own key won't match the `updater.pubkey` baked into `tauri.conf.json`, so a locally-built app can't verify updates signed by the official release key (and vice versa) — that's expected for local builds and only matters if you're testing the updater flow itself.

---

## Testing and Specifications

Luminous uses automated tests at both the frontend (Svelte 5) and backend (Rust) layers.

`bun run check` and `bun run lint:ast` run the type checks, lints and architecture rules CI enforces.

### Frontend Unit & Integration Tests (Vitest)

Frontend tests are written with Vitest and test component rendering, Svelte 5 stores, and state updates with mocked Tauri APIs.

To run the frontend test suite:
```bash
bun run test:run
```

### Backend Unit & Integration Tests (Rust)

To run the backend unit and integration test suite:
```bash
cargo test --workspace
```
This runs unit tests across all workspace crates and integration tests under `src-tauri/tests/`.

### Formal Proofs (Lean)

Machine-checked Lean models of the player and playlist state machines live in [`verification/lean/`](verification/lean/README.md). Install [elan](https://github.com/leanprover/elan) (the toolchain is pinned in `lean-toolchain`), then:
```bash
cd verification/lean
lake build --wfail
```

More detail: [Testing](docs/TESTING.md), [Performance](docs/PERFORMANCE.md), [Troubleshooting](docs/TROUBLESHOOTING.md), [CI](docs/CI.md).
