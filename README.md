# Luminous Music Player

[![Rust](https://img.shields.io/badge/Rust-ea4335?style=flat-square&logo=rust&logoColor=white)](https://www.rust-lang.org)
[![Tauri](https://img.shields.io/badge/Tauri-24c8db?style=flat-square&logo=tauri&logoColor=white)](https://tauri.app)
[![Svelte](https://img.shields.io/badge/Svelte-ff3e00?style=flat-square&logo=svelte&logoColor=white)](https://svelte.dev)
[![TypeScript](https://img.shields.io/badge/TypeScript-3178c6?style=flat-square&logo=typescript&logoColor=white)](https://www.typescriptlang.org)
[![Tailwind CSS](https://img.shields.io/badge/Tailwind_CSS-38bdf8?style=flat-square&logo=tailwind-css&logoColor=white)](https://tailwindcss.com)


Luminous is a modern, high-performance desktop music player built with **Tauri v2**, **Svelte 5**, and **Rust**. It is designed as a lightweight, premium reimagining of the classic Strawberry Media Player, focusing strictly on high-fidelity local audio file playback and library indexing.

---

## 🚀 Key Features (Phase 1 & 2 Complete)

1. **Incremental Directory Scanner**:
   - Indexes local music directories (MP3, WAV, FLAC, AAC, Ogg Vorbis) in seconds using lofty tag parsing.
   - Leverages file modification times (`mtime`) to skip unchanged files for rapid incremental scans.
2. **Robust WAL SQLite Database**:
   - Uses SQLite connection pooling (`r2d2`) with WAL-mode journaling optimized for low latency and high read throughput.
   - Built-in SQLite FTS5 (Full-Text Search) sync triggers for instantaneous searching across tracks, artists, and albums.
3. **Thread-Isolated Audio Decoder**:
   - Custom container probing and decoding using Symphonia.
   - Raw floating-point PCM audio output using CPAL (ALSA/PulseAudio/Pipewire on Linux).
   - Rate-limiting prevents high-CPU memory usage and decodes audio packages in sync with real-time playback.
   - Computes played-sample timecodes to drive real-time progress seeking.
4. **Active Playlist & Queue Manager**:
   - Create, rename, delete playlists.
   - Custom track reordering with shift up/down actions, queue skips, and full BDD undo/redo stacks.
5. **Cover Art Engine**:
   - Extracts embedded artwork from audio tags using lofty, hashes files for deduplication under `app_data_dir/covers/`, scans local directories, and queries the iTunes Search API as a fallback.
   - Serves local files securely using a custom `luminous-art://` URI scheme protocol.
6. **Audio Visualizers**:
   - Real-time 32-bar logarithmic spectrum analyzer rendering at 30 FPS using `rustfft` and lock-free audio ring buffers.
   - SoundCloud-style vertical peak waveform seek bars and colorized spectral audio moodbars (Clementine/Strawberry-style).
7. **10-Band Graphic Equalizer**:
   - Cascaded peaking biquad IIR filter DSP design that adapts to sample-rate and channel configurations.
   - Pre-configured flat/vocal/bass/style presets and a smooth Catmull-Rom spline curve visualization.
8. **Karaoke Lyrics View**:
   - Fetches and highlights synced LRC lyrics from LRCLIB and plain text from Lyrics.ovh with manual text corrections.
   - Visual track-level `LRC` indicators on track rows showing which songs have lyrics cached.
9. **Metadata Tag Editor & Fingerprinting**:
   - Edit metadata fields (Title, Artist, Album, Album Artist, Composer, Genre, Year, Track/Disc numbers) and save directly back to the audio file on disk.
   - Calculate AcoustID audio fingerprints using `fpcalc` to query suggestions and autofill tags.
10. **State Preservation (Core Design Principle)**:
    - Automatically persists and restores the user's active sidebar view, sub-tab selections, playlist selections, player volume levels, shuffle/repeat parameters, and equalizer configurations on startup.
11. **Modern, Responsive UI**:
    - Built on Svelte 5 runes and styled with Tailwind CSS v4.
    - Custom dark mode theme, live search filters, sortable metadata tables, and active audio filetype format indicator pills (MP3, WAV, etc.).

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
│   │   ├── stores/           # Global stores (player, collection, playlists)
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

### 1. BDD Feature Specifications (Gherkin format)
Luminous features are defined using BDD Gherkin files in the `features/` directory:
*   `library_scan.feature`: Outlines BDD rules for watch folders, indexing, and search functionality.
*   `playback_controls.feature`: Outlines BDD behaviors for play, pause, resume, seek, and volume control.
*   `playlists.feature`: Outlines BDD expectations for playlist creation, track reordering, and BDD undo/redo stacks.
*   `cover_art.feature`: Outlines Gherkin specifications for embedded and remote cover art scanning and display.
*   `equalizer.feature`: Outlines DSP peaking filters and preset configurations.
*   `lyrics.feature`: Outlines LRCLIB synced lyrics caching and Lyrics.ovh text fallbacks.
*   `tag_editor.feature`: Outlines tag writes using lofty and AcoustID audio fingerprinting lookups.

### 2. Unit and Integration Tests
We maintain an automated Rust test suite covering database structures, migrations, and playlist CRUD operations:
*   **Database Migrations**: Checks connection pool setup, tables, FTS5 virtual index creation, and WAL configurations.
*   **Playlist CRUD Operations**: Validates creation, renaming, retrieval, and removal of playlists against local SQLite interfaces.

To run the test suite:
```bash
cd src-tauri
cargo test
```

---

## 🛠️ Build and Development

### Prerequisites (Linux)
Ensure ALSA and SSL development headers are installed:
```bash
sudo apt install libasound2-dev libssl-dev pkg-config
```

### AcoustID / Chromaprint Setup (Optional)
To enable AcoustID audio fingerprinting and auto-tagging, you need the `fpcalc` utility:
*   **Linux (Ubuntu/Debian)**:
    ```bash
    sudo apt install libchromaprint-tools
    ```
*   **macOS (Homebrew)**:
    ```bash
    brew install chromaprint
    ```
*   **Windows**:
    Download the binary from the [AcoustID Website](https://acoustid.org/chromaprint), extract it, and add the folder containing `fpcalc.exe` to your system `PATH`.

If `fpcalc` is not in your system `PATH`, you can set the `FPCALC_PATH` environment variable to point directly to the binary:
```bash
export FPCALC_PATH="/path/to/fpcalc"
```

### Dev Server
To start the Svelte frontend dev server and compile the Tauri app:
```bash
bun run tauri dev
```

### Production Build
To package Luminous for release:
```bash
bun run tauri build
```
