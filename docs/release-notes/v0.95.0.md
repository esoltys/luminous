# Release Notes - Luminous v0.95.0

Luminous v0.95.0 contains exciting new features, design enhancements, and stability improvements since v0.90.0:

### 🌟 New Features
- **Tag-Based File Organizer**: A dry-run preview modal for reorganizing your library into a custom folder/filename template, with conditional blocks (optional disc/track segments), syntax-highlighted variable editing, resizable preview columns, and the option to relocate companion files (artwork, lyrics, logs, cue sheets) alongside moved tracks — now embedded directly inline in Settings (#81).
- **Bulk Tag Editor for Albums**: Edit metadata tags across an entire album at once instead of one song at a time (#70).
- **Playback Fades & Auto-Crossfade**: Optional fade-in/fade-out on play, pause, and seek, plus an auto-crossfade mode for gap-free transitions between tracks (#79).
- **Detached Picture-in-Picture Miniplayer Window**: A floating, always-on-top miniplayer with a hover transport control mask, drag-to-move, and drag-to-resize, with size and position now remembered across sessions (#46).
- **Queue Population Modes**: Auto-Refill playlists (genre/decade auto-playlists and custom Smart Playlists) can now be biased All / Favourites / Familiar / Discover / Deep Cuts, replacing the old fixed top-N ordering (#120).
- **Home Redesign — Top Artists & Row Layouts**: A full-width Top Artists carousel plus Most Played and Recently Added row layouts, with album genre now shown under the category label (#118).
- **Update & Format Settings**: New in-app update checking and automatic format detection, plus further i18n polish; all supported audio formats are now enabled by default (#107).
- **Miniplayer Song Rating**: Rate the current track directly from the miniplayer's hover controls.
- **Keyboard Shortcuts Overlay**: A new `Ctrl+/` shortcuts help modal, remapped playback keys, and new global hotkeys for layout and history navigation (#138).
- **In-App User Guide**: A full user guide is now available directly from the Help sidebar item (#142).

### 🎨 Visual & UX Improvements
- **New Design System**: App-wide sync to a refreshed design system with a new logo (split into crisp static and lush reactive artifacts) and updated playlist card gradients (#133, #134).
- **Artist Discography Overhaul**: Sets/Albums/EPs/Singles tabs now only appear when populated, loose singles-only artists get a proper grouped view, and cover art falls back to song artwork when there's no album to draw from.
- **Multi-Disc Track Numbering**: Track numbers now show as "{disc}-{track}" across releases with more than one disc.
- **Clickable Carousel Cards**: Song titles and artists in Home/Artist carousels now navigate to their album/artist views, matching the rest of the app.
- **Keyboard Shortcuts Everywhere**: Escape-to-cancel and Enter-to-confirm now work consistently across the Tag Editor, Album Tag Editor, Organize Files, and confirmation dialogs.
- **Themed Range Sliders**: Preamp, Target Level, Fallback Gain, Fade, and Crossfade sliders now show an accent-filled track instead of a flat, low-contrast bar.
- **Streamlined Overview**: Removed the redundant per-view overview infoboxes and added a direct Help link to the side nav (#140).
- **Settings & Organizer Polish**: Split Settings into dedicated Folders/Tools tabs, readable theme labels with Canadian spelling, a visible border on the organize-pattern input, and highlighted fields for values changed by an AcoustID lookup in the tag editor (#135, #137).
- **Playlist & Home Polish**: Refined toggles, album cards, Top 5 rows, and the playlist infobox (#129).
- **Small Polish**: Sun/Moon indicator on the System theme card, Repeat mode shown as a text label like Shuffle, an optional Skip Count column in the songs table, and a click-to-copy version number with a GitHub icon in About.
- **Rescaled Visualizers**: Spectrum analyzer and reactive logo now scale to peak-relative bins for a more consistent look across tracks.
- **Accessibility Audit**: Removed low-contrast, opacity-diluted text across every view.

### 🐛 Stability & Bug Fixes
- **Library Watcher Reconciliation**: Files moved across watched folders are now reconciled instead of silently breaking playback (#132).
- **Cover Art Reliability**: Fixed loose singles sharing one cached cover image, the background file watcher wiping cached artwork on tag edits, and embedded cover art being lost when saving tag changes.
- **Miniplayer Fixes**: Resolved the window growing on repeated enter/exit, a cropped album cover at default size, Ctrl+M double-firing, native/manual resize fighting each other, and a corner/border mismatch with the OS window frame.
- **Auto-Playlist Freshness**: Genre/decade auto-playlists now rebuild immediately after a scan instead of waiting up to 24 hours.
- **Organizer Correctness**: Excluded already-pruned songs from the preview, fixed companion-art paths after a relocation, normalized UNC paths on Windows, named colliding files clearly in the preview, and let Apply proceed past collisions by skipping only the affected files.
- **Empty-Folder Cleanup**: Clean Up now sweeps the whole library for empty folders (not just newly scanned ones), recognizes OneDrive placeholder folders, and shows the empty-folder count in its toast for longer. Ratings are now preserved when a watched-folder move relocates a track.
- **Shuffle Mode**: Fixed a bug where the previous track could replay itself in shuffle mode.
- **Theme Sync**: Dynamic Artwork theme colors now apply immediately when selected, instead of waiting for the next track change.
- **Immersion Mode**: Fixed the immersion toggle button showing an inverted on/off state.
- **Scan Reliability**: `scan_all` now always emits a completion signal, even when there are zero watched folders.
