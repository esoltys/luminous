# <img src="src-tauri/icons/128x128.png" width="32" height="32" alt="Luminous Logo" /> Luminous Music Player

[![Rust](https://img.shields.io/badge/Rust-ea4335?style=flat-square&logo=rust&logoColor=white)](https://www.rust-lang.org)
[![Tauri](https://img.shields.io/badge/Tauri-24c8db?style=flat-square&logo=tauri&logoColor=white)](https://tauri.app)
[![TypeScript](https://img.shields.io/badge/TypeScript-3178c6?style=flat-square&logo=typescript&logoColor=white)](https://www.typescriptlang.org)
[![Svelte](https://img.shields.io/badge/Svelte-ff3e00?style=flat-square&logo=svelte&logoColor=white)](https://svelte.dev)
[![Release](https://img.shields.io/badge/Release-0.31.0-blue?style=flat-square)](https://github.com/esoltys/luminous/releases)
[![Milestone 1.0](https://img.shields.io/badge/Milestone%201.0-40%25-green?style=flat-square)](https://github.com/esoltys/luminous/milestone/1)

Luminous is a high-performance desktop music player designed for modern local audio listening. Built with **Rust**, **Tauri v2**, **TypeScript**, and **Svelte 5 (Runes)**, it offers a lightweight, premium desktop experience with a beautiful dynamic user interface. You can download the compiled binaries for Windows and Linux from the [GitHub Releases](https://github.com/esoltys/luminous/releases) page.

<p align="center">
  <img src="docs/screenshots/home.png" width="100%" alt="Home View" />
</p>

<div align="center">
  <table>
    <tr>
      <td width="50%">
        <h4 align="center">Albums Grid</h4>
        <img src="docs/screenshots/albums.png" alt="Albums Grid" />
      </td>
      <td width="50%">
        <h4 align="center">Artists Grid</h4>
        <img src="docs/screenshots/artists.png" alt="Artists Grid" />
      </td>
    </tr>
    <tr>
      <td width="50%">
        <h4 align="center">Synced Lyrics</h4>
        <img src="docs/screenshots/lyrics.png" alt="Synced Lyrics" />
      </td>
      <td width="50%">
        <h4 align="center">Custom Theme Builder</h4>
        <img src="docs/screenshots/themes.png" alt="Custom Theme Builder" />
      </td>
    </tr>
  </table>
</div>

---

## Product Highlights

*   **Dynamic Theme Engine & Customizer**: Switch between curated, elegant color themes (such as Luminous Violet, Ruby Red, Nordic Blue, and Retro Amber) or design your own with an interactive, real-time Custom Theme Builder that updates the app live.
*   **Immersive Audio Visualizers**: View your sound with a real-time 32-bar logarithmic spectrum analyzer rendering at 30 FPS, colorized spectral moodbars, and SoundCloud-style peak waveform seek bars.
*   **10-Band Graphic Equalizer**: Fine-tune your listening experience with a precise cascaded biquad DSP equalizer featuring an interactive Catmull-Rom spline visualization and style-based presets.
*   **Karaoke Synced Lyrics**: Enjoy real-time, scrolling synced lyrics (LRC) fetched from LRCLIB and plain text from Lyrics.ovh, complete with local caching and visual lyrics indicators.
*   **AcoustID Audio Fingerprinting & Tag Editor**: Automatically identify tracks and fix incorrect metadata using AcoustID acoustic fingerprinting (via `fpcalc`) and write edits directly back to your audio files.
*   **High-Performance Library Scanning**: Index thousands of local tracks (MP3, WAV, FLAC, AAC, Ogg Vorbis) in seconds using an incremental scanner that skips unchanged files based on modification times.
*   **Smart Cover Art Engine**: Extract embedded artwork automatically using lofty tag parsing, with automatic iTunes Search API fallback and local file deduplication.
*   **Instant Search**: Locate any track, album, or artist in your collection instantly using database-level SQLite FTS5 (Full-Text Search).
*   **Seamless State Preservation**: Never lose your place. Luminous automatically restores your active sidebar, playlist selections, player volume, queue state, and equalizer configuration when reopened.

---

## Architecture

```
luminous/
├── features/                 # BDD Gherkin Feature Specifications
├── src/                      # Svelte 5 + TypeScript Frontend
│   ├── lib/
│   │   ├── components/       # PlayerBar, Visualizers, Equalizer, LyricsView, TagEditor, etc.
│   │   ├── stores/           # Global stores (player, collection, playlists, theme)
│   │   └── types/            # Frontend interfaces
│   └── routes/               # Layouts and navigation views
└── src-tauri/                # Tauri + Rust Backend Core
    ├── src/
    │   ├── analyzer.rs       # Real-time FFT spectrum processing
    │   ├── audio.rs          # Symphonia decoding thread & CPAL playback loop
    │   ├── collection.rs     # Lofty scanner & folder watcher
    │   ├── covermanager.rs   # Cover art extractor and iTunes search API fallback
    │   ├── db.rs             # SQLite schema migration & connection pool
    │   ├── equalizer.rs      # Biquad filter DSP configurations
    │   ├── lyrics.rs         # LRCLIB & Lyrics.ovh client integrations
    │   ├── models.rs         # Shared structs and types
    │   ├── moodbar.rs        # Spectral audio analysis scanner
    │   ├── player.rs         # Playback controller (Shuffle, Repeat, Next/Prev)
    │   ├── playlist.rs       # Playlist manager & undo/redo command stack
    │   ├── tageditor.rs      # lofty tag writer & AcoustID fingerprint generator
    │   ├── waveform.rs       # Background audio peak analyzer
    │   ├── commands/         # Tauri IPC command handlers
    │   └── lib.rs            # App entry point, background loops, & IPC registry
    └── Cargo.toml            # Rust dependencies (cpal, symphonia, rusqlite, lofty, rustfft)
```

---

## Building Luminous

Luminous is a cross-platform application that can be built and run on both Linux and Windows.

### Linux (Ubuntu/Debian)

#### 1. Install System Dependencies
Ensure the required build tools, GTK, WebKit, ALSA, and SSL development headers are installed:
```bash
sudo apt update
sudo apt install -y build-essential curl wget file libssl-dev libgtk-3-dev libwebkit2gtk-4.1-dev libsoup-3.0-dev libayatanaloop-dev libayatana-appindicator3-dev librsvg2-dev libasound2-dev pkg-config
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
bun run install:git-hooks # sets core.hooksPath to use the repository's tracked .githooks/pre-commit hook
bun run tauri dev
```
#### 4. Build Production Bundle
```bash
bun run tauri build
```

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
```powershell
bun run tauri build
```

---

### AcoustID / Chromaprint Setup (Optional)

To enable AcoustID audio fingerprinting and automatic metadata lookup, the `fpcalc` utility must be installed:

*   **Linux (Ubuntu/Debian)**:
    ```bash
    sudo apt install libchromaprint-tools
    ```
*   **Windows**:
    Download the binary from the [AcoustID Website](https://acoustid.org/chromaprint), extract it, and add the folder containing `fpcalc.exe` to your system `PATH`. Alternatively, you can set the `FPCALC_PATH` environment variable pointing directly to the binary:
    ```powershell
    # Windows PowerShell
    $env:FPCALC_PATH="C:\path\to\fpcalc.exe"
    ```

---

## Testing and Specifications

Luminous uses automated tests at both the frontend (Svelte 5) and backend (Rust) layers.

### Frontend Unit & Integration Tests (Vitest)

Frontend tests are written with Vitest and test component rendering, Svelte 5 stores, and state updates with mocked Tauri APIs.

To run the frontend test suite:
```bash
bun run test:run
```

### Backend Unit & Integration Tests (Rust)

To run the standard cargo unit test suite:
```bash
cd src-tauri
cargo test
```

### Backend BDD Features (Cucumber)

Luminous features are defined using Cucumber Gherkin specifications in the `features/` directory and implemented as integration tests in Rust using the `cucumber` crate.

To run the Cucumber BDD test suite:
```bash
cd src-tauri
cargo test --test equalizer_bdd
```
