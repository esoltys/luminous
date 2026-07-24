# Release Notes - Luminous v1.0.0

> **Status:** Draft, prepared ahead of the v1.0.0 release. Not yet published.

Luminous v1.0.0 marks the completion of **Milestone 1.0 — Core Desktop Player & Local Collection**. It contains exciting new features, design enhancements, and stability improvements since v0.90.0:

### 🌟 New Features
- **Tag-Based File Organizer**: A new dry-run preview modal for reorganizing your library into a custom folder/filename template, with conditional blocks (optional disc/track segments), syntax-highlighted variable editing, resizable preview columns, and the option to relocate companion files (artwork, lyrics, logs, cue sheets) alongside moved tracks (#81).
- **Bulk Tag Editor for Albums**: Edit metadata tags across an entire album at once instead of one song at a time (#70).
- **Playback Fades & Auto-Crossfade**: Optional fade-in/fade-out on play, pause, and seek, plus an auto-crossfade mode for gap-free transitions between tracks (#79).
- **Detached Picture-in-Picture Miniplayer Window**: A floating, always-on-top miniplayer with a hover transport control mask, drag-to-move, and drag-to-resize, with size and position now remembered across sessions (#46).
- **Queue Population Modes**: Auto-Refill playlists (genre/decade auto-playlists and custom Smart Playlists) can now be biased All / Favourites / Familiar / Discover / Deep Cuts, replacing the old fixed top-N ordering (#120).
- **Home Redesign — Top Artists & Row Layouts**: A full-width Top Artists carousel plus Most Played and Recently Added row layouts, with album genre now shown under the category label (#118).
- **Update & Format Settings**: New in-app update checking and automatic format detection, plus further i18n polish; all supported audio formats are now enabled by default (#107).
- **Miniplayer Song Rating**: Rate the current track directly from the miniplayer's hover controls.

### 🎨 Visual & UX Improvements
- **Artist Discography Overhaul**: Sets/Albums/EPs/Singles tabs now only appear when populated, loose singles-only artists get a proper grouped view, and cover art falls back to song artwork when there's no album to draw from.
- **Multi-Disc Track Numbering**: Track numbers now show as "{disc}-{track}" across releases with more than one disc.
- **Clickable Carousel Cards**: Song titles and artists in Home/Artist carousels now navigate to their album/artist views, matching the rest of the app.
- **Keyboard Shortcuts Everywhere**: Escape-to-cancel and Enter-to-confirm now work consistently across the Tag Editor, Album Tag Editor, Organize Files, and confirmation dialogs.
- **Themed Range Sliders**: Preamp, Target Level, Fallback Gain, Fade, and Crossfade sliders now show an accent-filled track instead of a flat, low-contrast bar.
- **Small Polish**: Sun/Moon indicator on the System theme card, Repeat mode shown as a text label like Shuffle, an optional Skip Count column in the songs table, and a click-to-copy version number with a GitHub icon in About.
- **Accessibility Audit**: Removed low-contrast, opacity-diluted text across every view.

### 🐛 Stability & Bug Fixes
- **Cover Art Reliability**: Fixed loose singles sharing one cached cover image, the background file watcher wiping cached artwork on tag edits, and embedded cover art being lost when saving tag changes.
- **Miniplayer Fixes**: Resolved the window growing on repeated enter/exit, a cropped album cover at default size, Ctrl+M double-firing, native/manual resize fighting each other, and a corner/border mismatch with the OS window frame.
- **Auto-Playlist Freshness**: Genre/decade auto-playlists now rebuild immediately after a scan instead of waiting up to 24 hours.
- **Organizer Correctness**: Excluded already-pruned songs from the preview, fixed companion-art paths after a relocation, and normalized UNC paths on Windows.
- **Shuffle Mode**: Fixed a bug where the previous track could replay itself in shuffle mode.
- **Theme Sync**: Dynamic Artwork theme colors now apply immediately when selected, instead of waiting for the next track change.
