Luminous v0.31.0 is the initial tagged release of Luminous Music Player, bringing a beautiful, modern desktop interface to your music collection:

### 🌟 Core Features
- **High-Performance Audio Playback Engine**: Built on CPAL and rodio, providing cross-platform hardware-accelerated playback for MP3, FLAC, ALAC, and WAV files with seeking and volume controls.
- **Asynchronous Metadata & Artwork Scanner**: Scans folders for music tracks, parsing embedded ID3/Vorbis metadata and caching album artwork.
- **Synced Karaoke-Style Lyrics**: Automatically pulls, caches, and scrolls timestamped lyrics in sync with current audio playback.
- **Interactive Playlist Organizer**: Create custom music playlists with interactive drag-and-drop track reordering.
- **Modern Responsive Svelte 5 / Tauri UI**: Sleek layout featuring theme presets, custom styling, and responsive panels.

### 🛠️ Developer & CI/CD Tooling
- **Cucumber BDD Tests**: Automated behavior-driven tests for the Rust audio backend.
- **Vitest Suite**: Detailed unit testing for all reactive stores.
- **Release Automation**: Integrated custom bump-version and release scripting with GitHub Actions.
