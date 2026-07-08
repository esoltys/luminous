# Luminous Music Player

[![Rust](https://img.shields.io/badge/Rust-ea4335?style=flat-square&logo=rust&logoColor=white)](https://www.rust-lang.org)
[![Tauri](https://img.shields.io/badge/Tauri-24c8db?style=flat-square&logo=tauri&logoColor=white)](https://tauri.app)
[![Svelte](https://img.shields.io/badge/Svelte-ff3e00?style=flat-square&logo=svelte&logoColor=white)](https://svelte.dev)
[![TypeScript](https://img.shields.io/badge/TypeScript-3178c6?style=flat-square&logo=typescript&logoColor=white)](https://www.typescriptlang.org)
[![Tailwind CSS](https://img.shields.io/badge/Tailwind_CSS-38bdf8?style=flat-square&logo=tailwind-css&logoColor=white)](https://tailwindcss.com)

Luminous is a gorgeous, high-performance desktop music player designed for modern local audio listening. Built with **Tauri v2**, **Svelte 5 (Runes)**, **Rust**, and **Tailwind CSS**, it offers a lightweight, premium desktop experience with a beautiful dynamic user interface.

---

## 🚀 Product Highlights

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

## 🏗️ Architecture

```
luminous/
├── features/                 # BDD Gherkin Feature Specifications
│   ├── cover_art.feature     # BDD Specs for cover art extraction
│   ├── equalizer.feature     # BDD Specs for biquad equalizer
│   ├── library_scan.feature  # BDD Specs for local collection scanner
│   ├── lyrics.feature        # BDD Specs for synced lyrics caching
│   ├── playback_controls.feature # BDD Specs for player audio engine
│   ├── playlists.feature     # BDD Specs for playlist editor
│   └── tag_editor.feature    # BDD Specs for lofty metadata writes & AcoustID
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

## 🔬 Testing and Specifications

Luminous features are defined using BDD Gherkin files in the `features/` directory and backed by unit and integration tests.

To run the test suite:
```bash
cd src-tauri
cargo test
```

---

## 🛠️ Building Luminous

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
