# Luminous Music Player

Luminous is a modern, high-performance desktop music player built with **Tauri v2**, **Svelte 5**, and **Rust**. It is designed as a lightweight, premium reimagining of the classic Strawberry Media Player, focusing strictly on high-fidelity local audio file playback and library indexing.

---

## 🚀 Key Features (Phase 1 BDD Complete)

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
5. **Modern, Responsive UI**:
   - Built on Svelte 5 runes and styled with Tailwind CSS v4.
   - Custom dark mode theme, live search filters, sortable metadata tables, and active audio filetype format indicator pills (MP3, WAV, etc.).

---

## 🏗️ Architecture

```
luminous/
├── features/                 # BDD Gherkin Feature Specifications
│   ├── library_scan.feature  # BDD Specs for local collection scanner
│   ├── playback_controls.feature # BDD Specs for player audio engine
│   └── playlists.feature     # BDD Specs for playlist editor
├── src/                      # Svelte 5 + TypeScript Frontend
│   ├── lib/
│   │   ├── components/       # PlayerBar, CollectionView, PlaylistView, Sidebar, etc.
│   │   ├── stores/           # Global runes stores (player, collection, playlists)
│   │   └── types/            # Frontend interfaces
│   └── routes/               # Layouts and navigation views
└── src-tauri/                # Tauri + Rust Backend Core
    ├── src/
    │   ├── audio.rs          # Symphonia decoding thread & CPAL playback loop
    │   ├── collection.rs     # Lofty scanner & folder watcher
    │   ├── db.rs             # SQLite schema migration & connection pool
    │   ├── models.rs         # Shared structs and types
    │   ├── player.rs         # Playback controller (Shuffle, Repeat, Next/Prev)
    │   ├── playlist.rs       # Playlist manager & undo/redo command stack
    │   ├── commands/         # Tauri IPC invocation wrappers
    │   └── lib.rs            # App entry point, background loops, & IPC registry
    └── Cargo.toml            # Rust cargo dependencies (cpal, symphonia, rusqlite, lofty)
```

---

## 🔬 Testing and Specifications

### 1. BDD Feature Specifications (Gherkin format)
Luminous features are defined using BDD Gherkin files in the `features/` directory:
*   `library_scan.feature`: Outlines BDD rules for watch folders, indexing, and search functionality.
*   `playback_controls.feature`: Outlines BDD behaviors for play, pause, resume, seek, and volume control.
*   `playlists.feature`: Outlines BDD expectations for playlist creation, track reordering, and BDD undo/redo stacks.

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
