# Release Notes - Luminous v0.98.0

Luminous v0.98.0 is a feature-heavy release since v0.97.0, headlined by real in-app
auto-update support and a batch of features shaped directly by community requests —
including the first feature request filed by an outside user.

### 📣 Community
- **A second thank-you to [@Crocodile73](https://github.com/Crocodile73)**: after filing
  Luminous's first-ever user-reported bug back in v0.96.0, they're back with its first
  user-requested *feature*: [#219](https://github.com/esoltys/luminous/issues/219), asking
  for album/playlist cards to stop starting playback on a plain click. Fixed below.
- **More requests, shipped**: independent album ratings (#242), grouped batch notifications
  for folder imports (#233), and scroll position preservation across views (#232) all came
  out of user feedback — thank you to everyone filing issues and
  requests; keep them coming.

### 🌟 New Features
- **Real In-App Auto-Update** (#211): Replaces the old "check GitHub, then hand you a link"
  updater with the actual Tauri Updater plugin — signature-verified in-app download,
  install, and restart on Windows and Linux AppImage builds, instead of guessing a download
  asset by file extension. Since this is the first release able to *consume* a signed
  update, there's nothing to self-update to until v0.99.0 ships — auto-update won't
  actually activate until the release after this one.
- **Independent Album Ratings**: Albums can now be rated 0.5–5.0 stars (or favourited),
  separate from any individual song rating — useful when you love an album as a whole but
  have different favourite tracks on it. Sortable in the Collection view (#244).
- **Compact Rows View for Albums & Artists**: A Cards/Rows toggle, remembering its own mode
  per tab, matching the density option Playlists already had (#225).
- **Row/Grid Toggle & Redesigned Glow for Playlists**: Auto and Custom playlist grids get
  the same Cards/Rows toggle as Albums/Artists, plus a translucent tinted-glow redesign
  (replacing the old opaque color blocks) extended across playlist cards, Queue, and Smart
  Playlists (#229).
- **Playlists Refresh-All Button**: A Refresh icon next to the view toggle re-syncs
  genre/decade auto-playlists and force-regenerates every dynamic playlist, bypassing the
  normal 24h staleness gate (#243).
- **Native App Shell**: Right-click no longer falls through to the browser's default
  context menu, and reload/find/print/zoom shortcuts are disabled — Luminous now reads as a
  native app instead of a stock webview (#227).
- **Grouped, Accurate Import Notifications**: Dropping a folder of songs into a watched
  directory now collapses into a single progress toast instead of one-per-file (or, in some
  cases, duplicate/miscounted toasts from the same import) (#235).
- **Scroll Position Preserved Across Views**: Switching tabs, opening/closing a detail view,
  or using Back/Forward no longer resets your scroll position to the top (#237).

### 🎨 Visual & UX Improvements
- **Removed UI Sound Cues**: The synthesized toast/favourite tones felt like clutter on top
  of the music already playing; dropped entirely along with their settings toggle (#215).
- **Shuffle/Repeat Icon Pairing**: Illegible tiny text badges ("IA", "1x") replaced with
  full-size paired mode icons and guide tooltips; the non-functional "One by One" repeat
  mode was removed rather than left as a dead option (#236).
- **Instructive-Voice Copy Pass**: Settings and tooltip hint text rewritten from
  descriptive/third-person to imperative voice, matching the rest of the app's copy, in
  both English and French (#234).
- **Cards Navigate, Not Play**: Clicking an album/playlist card now only navigates to it —
  playback no longer starts unexpectedly from a stray click (#221, fixes #219).
- **Accent-Contrast Text Color Fix**: Switched to a perceived-brightness heuristic for
  choosing black vs. white text on accent-colored buttons, fixing muddy-looking text on
  medium-saturation theme accents; pane-layout toggles now use the same active-state accent
  color as other view toggles (#245).
- **Clear Messaging on Schema Mismatch**: Opening an older build against a database from a
  newer one now explains the version mismatch instead of silently showing an empty library
  (#226).

### 🐛 Stability & Bug Fixes
- **Auto/Smart Playlists Silently Capped**: Genre/decade auto-playlists and rule-based Smart
  Playlists only ever populated their first 25–100 matching songs regardless of how many
  actually matched; now every matching song is included (#241).
- **Duplicate Songs After Case-Only Renames**: Renaming an album folder's casing only (e.g.
  "HERO" → "Hero") on a case-insensitive filesystem produced a duplicate track listing
  instead of updating the existing one; a new Clean Up merge action also remediates
  libraries that already picked up duplicates from this bug (#238).
- **Startup Window Flash**: The window now stays hidden until fully created instead of
  flashing on launch (#239).
- **File Watcher Race Conditions**: Fixed redundant per-subfolder library scans, duplicate
  "songs added" toasts racing the watcher's own batch events, inflated song counts from
  non-audio files, and a watcher/tag-editor write race — all part of the same batch
  notification overhaul (#235).
- **Removed Dead `mood` Column**: Cleaned up a database column that was never populated from
  file tags and had no write path (#218).
