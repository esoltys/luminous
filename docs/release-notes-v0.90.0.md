# Release Notes - Luminous v0.90.0

Luminous v0.90.0 contains exciting new features, design enhancements, and stability improvements since v0.75.0:

### 🌟 New Features
- **Smart Playlist Builder & Advanced Filter Engine**: Rule-driven dynamic playlists (`artist`, `album`, `genre`, `year`, `rating`, etc.) with live track matching preview, decade range auto-naming (*1980s Rock Mix*), customizable table columns, and dedicated purple gradient styling (#13).
- **Native OS Now Playing Integration**: System Media Transport Controls via Souvlaki (Windows SMTC, Linux MPRIS2, macOS Now Playing) enabling system media keys, lock screen track metadata, play/pause, seek, and track skipping (#101).
- **Live Search Auto-Suggestions & History**: Top search bar live auto-suggestions for tracks, albums, artists, custom playlists, and auto-playlists with direct view navigation and recent search history dropdown (#100).
- **Seamless History Navigation**: Top-bar navigation arrows, keyboard shortcuts, and mouse thumb button support for navigating backwards/forwards through app history (#67).
- **Context-Aware Recently & Frequently Played**: Retains album/playlist playback context under Recently Played and Most Frequently Played views with interactive PlayerBar album art click navigation (#99).
- **Auto-Refill & Dynamic Playlists**: Renamed Auto-Play to Auto-Refill across UI/locales with clear status badges, 25-song minimum thresholds, and end-of-library notification banners (#26).
- **Lyrics & Audio Enhancements**: Instrumental track toggle for skipping lyric fetching (#12), proactive seekbar waveform preloading with loading indicator, and mono/stereo audio channel & VBR bitrate badges in the details sidebar.

### 🎨 Visual & UX Improvements
- **Dedicated Smart Playlist Visuals**: Smart playlists feature distinct purple gradient backgrounds and dedicated "Smart" badge styling.
- **Active Playlist Queue Bar**: Re-laid toolbar with "Make Active" button, animated active queue indicator, pinned Queue playlist, and Undo/Redo actions.
- **Relative Date Formatting**: Display relative dates (*Today*, *Yesterday*) on playlist cards and standardized grid card dimensions.
- **Global Modal Handoff**: Elevated Smart Playlist Builder modal to root layout to ensure global handoff accessibility and proper z-index layering above the acrylic PlayerBar.

### 🐛 Stability & Bug Fixes
- **Build Warning Cleanup**: Fixed Svelte 5 `state_referenced_locally` warnings in `SmartPlaylistBuilderModal.svelte` using `untrack()`.
- **Production Build Hook**: Removed `postbuild` hook that automatically executed screenshot generation on every production build (`bun run take-screenshots` remains available manually).
- **Smart Playlist Routing**: Fixed smart playlists being misidentified as exact-match genre auto-playlists and pruned on sync.
- **Active Playlist Decoupling**: Viewing a playlist no longer automatically marks it as the "Active" playback queue target.

### ⚠️ Important Notice
- **Bundle Identifier Update (`org.luminous.app` ➔ `org.luminous.music`)**: Updated bundle identifier to resolve macOS application bundle extension conflicts. Upgrading users should note that local app data/settings will not automatically migrate from previous versions (`v0.75.0` or earlier).
