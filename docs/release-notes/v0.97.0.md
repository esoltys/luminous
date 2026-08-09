# Release Notes - Luminous v0.97.0

Luminous v0.97.0 is a large polish-and-stability release since v0.96.0, closing out both the
Windows (#164) and Linux (#128) 1.0 QA pre-release checklists alongside several new features.

### 🌟 New Features
- **Toggleable Song-Table Columns**: Every song column is now toggleable and consistent across
  Collection, Queue, Custom Playlists, Auto-Playlists, and Album views, grouped into Visible /
  Metatags / Luminous sections (#178, #197).
- **Joyful Micro-Animations & Sound Cues**: Celebration moments — favouriting a song, finishing
  an import, hitting a milestone, app updates — now get a motion pass (heart pulse, check pop,
  gold ring) plus optional synthesized sound cues (off by default, toggle under Settings →
  General), and toasts stay on screen until dismissed (#182, #191, #195).
- **Your Own AcoustID API Key**: A field under Settings now accepts a personal AcoustID API key
  instead of relying solely on the environment variable or the app's built-in fallback (#163).
- **OS File Associations**: Luminous now registers itself as an "Open With" handler for every
  audio format it decodes (MP3, FLAC, OGG, Opus, M4A, AAC, ALAC, WAV, AIFF, WV, MPC, APE, TTA,
  DSF, DFF, ASF, WMA, M4B) and every playlist format it parses (M3U, M3U8, PLS, XSPF), including
  while the app is already running (#206).
- **Editable BPM, Grouping & Initial Key**: These previously dead, never-populated columns are
  now fully working — read at scan time, stored, written back to the file, and editable from the
  Song Tag Editor (#205).

### 🎨 Visual & UX Improvements
- **New Display Font — Expose**: Replaced Space Grotesk with Expose across the app UI, in-app
  Help guides, and design reference docs; self-hosted so it still works offline (#198, #208).
- **Playlist & Album Toolbar Redesign**: Low-frequency actions (import/export/duplicate-cleanup/
  clear/delete) moved into a single overflow menu, Undo/Redo/Columns became icon-only controls,
  and the header layout was unified across Custom Playlists, Auto-Playlists, and Albums (#196,
  #199, #202); Songs/Playlists toolbars were further realigned for consistent spacing (#205).
- **Linux Polish**: Opaque playbar with accent glow (blur is a compositor no-op on
  Linux/WebKitGTK), working miniplayer edge/corner resize handles, and Home page cover art fixes
  (#204).
- **Miniplayer Fixes**: Fixed the miniplayer/full-player window size drifting larger with every
  toggle (#209), and improved hover-overlay opacity, mouse-exit detection, and cover art scaling
  so art fills the window instead of being capped at 240px (#210).
- **Sidebar Resizer**: Stopped the resizer from changing width on hover, which was triggering
  unnecessary redraws of the middle content pane (#203).
- **Smaller Fixes**: Right-click now selects a song before opening its context menu, sticky Auto/
  Smart playlist badges no longer render above the header while scrolling, the Sort dropdown is
  now data-driven from visible columns, and ~20 missing French translations were filled in
  (#205).

### 🐛 Stability & Bug Fixes
- **Auto-Playlist Refill Freeze**: Clicking a song in a dynamic auto-play playlist could freeze
  the app entirely after a runaway loop appended batches of 25 songs to the queue indefinitely;
  playback state now updates correctly and song lookups are batched instead of queried one at a
  time (#194).
- **Watched-Folder Disconnects**: The realtime file watcher hard-deleted every song under a
  watched drive the instant it disconnected mid-session — a real data-loss bug. It now soft-
  flags those songs unavailable instead, matching the existing scan-time protection; a failed
  track no longer loops forever trying `next_track()` under Repeat Playlist (#200).
- **Album View Playback Deadlock**: Play All and per-track play appeared broken because of a
  deadlock in the audio engine's error handler that permanently wedged the player mutex app-wide
  — not just on the album page (#197).
- **Recently Added Grouping**: A multi-track release with varying per-track artists and no
  `album_artist` no longer splits into two separate cards; Clean Up also now detects and removes
  duplicate song entries (#173).
- **Auto-Playlists Disappearing**: Genre/decade auto-playlists no longer vanish when a filtered
  population mode (Favourites, Essentials, Discover, Deep Cuts) matches fewer than 25 tracks, and
  now show a localized fill-mode suffix (e.g. "1980s Rock Deep Cuts") (#168).
- **Date Added Column**: Fixed the column always rendering blank and sorting as a no-op — the
  `added` timestamp existed in the database but was never sent to the frontend.
- **Organize Fixes**: Resolved fake-duplicate cycling, routed physical duplicates and external
  path collisions into a `Duplicates` folder instead of overwriting files, and fixed a database
  UNIQUE constraint failure (#164). Empty-folder cleanup no longer silently fails on Windows/
  OneDrive drives with read-only leftover files (e.g. `desktop.ini`).
- **Linux Packaging**: Added missing project metadata and an AppStream metainfo file so Linux
  package managers show accurate author, license, and description info for `.deb`/`.rpm`/
  `.appimage` builds (#207).
