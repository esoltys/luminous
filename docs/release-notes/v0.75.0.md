# Release Notes - Luminous v0.75.0

Luminous v0.75.0 contains exciting new features, design enhancements, and stability improvements since v0.67.0:

### 🌟 New Features
- **Decades & Smart Auto-Playlists**: Automatic categorization by release decade (60s through 2020s) with dedicated category color badges (#16) and updated grid shell/card styling (#92, #95).
- **Enhanced Playlist Editor**: Support for multi-selection (#86), drag-and-drop batch reordering (#85), duplicate track detection, inline search, and multi-layer cover art stacks across playlist views.
- **Seamless State & Context Restoration**: Full persistence and seamless restoration of playback position, active queue/context, active subviews, and selected album/artist details upon application restart (#72, #91).
- **Artist & Genre Discovery**: Primary genre display directly on artist cards with optimized backend query performance and expanded sorting options (by genre, release count, or alphabetical).
- **Integrated Library Maintenance**: Configurable auto-scanning, directory watch settings, and database indexing directly from Settings (#19).
- **About & Credits View**: Added About & Credits settings tab with detailed project info and third-party attributions.

### 🎨 Visual & UX Improvements
- **Unified Cover Stack Component**: Standardized multi-layered cover art stacked displays on `AlbumCard`s, Artist detail views, and Playlist hubs.
- **Grid & Layout Alignment**: Unified top bar and card grid scroll containers to guarantee consistent right-margin alignment across all column counts.
- **Carousel Controls**: Refined Home view carousel navigation with inline prev/next buttons (#94).
- **Z-Index Layering**: Clamped floating context menus and modal dialogs to render above the acrylic PlayerBar dock.
- **Table Header Sorting**: Enabled interactive column header sorting in playlist and table views.

### 🐛 Stability & Bug Fixes
- **Seekbar Waveforms**: Fixed dual waveform peak generation and normalization when processing seekbar visualizer data (#93).
- **Artist Metadata**: Corrected artist album count calculation and singular/plural formatting.
- **Canvas Theme Colors**: Fixed dynamic theme resolution for seekbar waveform canvas elements.
- **Screenshot Automation**: Updated screenshot generator to accurately match lyrics with featured tracks (#96).
