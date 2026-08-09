Luminous v0.67.0 contains exciting new features, design enhancements, and stability improvements since v0.60.0:

### 🌟 New Features
- **Play Statistics & Ratings**: Play counts and last-played tracking across the library, with hearts or 5-star ratings (configurable single rating style) synced live across every view.
- **Gapless Playback & 20-Band Parametric Equalizer**: Double-buffered decoding removes silence and clicks between tracks, alongside a new DSP chain contract and a 20-band parametric equalizer mode with a live response-curve preview and shared presets.
- **EBU R128 Loudness Analysis**: Automatic loudness analysis with ReplayGain fallback, plus Format and Loudness rows and a live-syncing Lyrics status in the Now Playing panel.
- **Bilingual Interface (English/French)**: Full i18n support with instant language switching across the entire UI.
- **Dedicated Album Detail View**: A standalone album page with song count, plays column, and album-level controls.
- **Playlist Undo/Redo & Import/Export**: A full undo/redo command stack for playlist edits, plus import/export support for M3U, M3U8, PLS, and XSPF playlist files with relative or absolute path options.
- **Moodbar Toggle Mode & Moodmoji**: A contrast-boosted moodbar visualizer, now toggleable directly on the seek bar (waveform ⇄ moodbar), with a color legend tooltip and a 2-emoji "moodmoji" hash next to the now-playing track.

### 🎨 Visual & UX Improvements
- **Home Page Curation**: Album curation grouping on the Home hub, clickable album artists, and renamed Grid views ("Overview").
- **Adaptive Player Bar**: The player bar now hides automatically when nothing is playing, and the rating control moved next to repeat.
- **Consistent Naming**: Standardized "Song" over "Track" throughout the interface.

### 🐛 Stability & Bug Fixes
- **Playlist Drag-and-Drop**: Fixed track reordering races and disabled native window drag interference so HTML5 drag reordering works reliably.
- **Lyrics Fetching**: Synced lyrics are now prioritized over plain-text results when fetching online.
- **Cover Art & Screenshot Tooling**: Corrected cover art extraction, auto-detection of the local database, and mock-config overrides used by the docs screenshot harness.
- **i18n Coverage**: Closed remaining gaps where hardcoded strings bypassed translation.
