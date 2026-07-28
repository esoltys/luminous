# Release Notes - Luminous v0.96.0

Luminous v0.96.0 is a polish-and-stability release since v0.95.0, and marks a small
milestone: the first bug reported by someone other than the developer, fixed.

### 📣 Community
- **Luminous now has a home on the web**: a new landing page at
  [esoltys.dev/luminous](https://esoltys.dev/luminous/).
- **GitHub Discussions are open**: share ideas, ask questions, and talk shop over at
  [github.com/esoltys/luminous/discussions](https://github.com/esoltys/luminous/discussions).
- **Bug reports & feature requests**: file them at
  [github.com/esoltys/luminous/issues](https://github.com/esoltys/luminous/issues).
- **First user-reported bug, fixed**: [#166](https://github.com/esoltys/luminous/issues/166)
  — background disk activity being mistaken for a scan that never finishes — was the very
  first bug reported by an actual user rather than found in-house, filed by
  [@Crocodile73](https://github.com/Crocodile73). Thank you for the report; it's fixed
  below, and it won't be the last one we chase down together. 🎉

### 🎨 Visual & UX Improvements
- **Miniplayer Controls Rework**: Added a volume slider and mute button, extended the
  drag grabber into a full-width textured strip across the top edge, removed the manual
  resize handle and "Luminous" label, and moved the restore button to the bottom-right
  with a corrected `Ctrl+M` tooltip (#158).
- **Unified Empty-Library Welcome**: Home, Songs, Albums, and Artists now share a single
  welcome card (Add Folder / Help) when no watched folder has been added yet, and the
  Collection, Playlists, and Lyrics sidebar items stay hidden until the library actually
  has songs.
- **Shared Component Migration**: Hand-rolled buttons and text inputs across the app now
  use the shared Button/Input components for a more consistent look (#159).
- **Dynamic Artwork Text Color**: Fixed a mismatch between the live theme and a saved
  Custom Theme's text color under Dynamic Artwork.
- **Empty-State Icons**: Matched empty-state icons to their corresponding sidebar nav
  icons.
- **i18n Fixes**: Closed translation gaps in the Smart Playlist Builder, top navigation,
  and elsewhere (#146).
- **French Screenshots**: Documentation screenshots are now generated in French
  automatically alongside English (#154).

### 🐛 Stability & Bug Fixes
- **Background Disk Activity Mistaken for a Stuck Scan** ([#166](https://github.com/esoltys/luminous/issues/166),
  #167): The background loudness analyzer decoded the entire library unconditionally,
  even with Loudness Normalization turned off (its default), which looked like a scan
  that never finished on large or slow external drives. It's now paused whenever the
  feature is disabled, the Watched Folders list correctly reflects whether Real-Time
  Watching is on, and Albums/Artists cover art thumbnails now lazy-load instead of all
  fetching from disk at once.
- **Mass Data Loss on Unreachable Drives**: Automatic scans could hard-delete every song
  on a watched drive that was temporarily disconnected or unmounted, wiping ratings, play
  counts, tags, and playlist memberships. Scans now soft-delete (mark unavailable) songs
  under an unreachable root instead, reserving hard deletion for the explicit "Clean Up
  Missing Songs" action (#161).
- **Dependency Cleanup**: Unified on a single version of `reqwest` across the backend,
  removing a duplicated dependency from the build (#160).
