Luminous v0.50.0 is a minor feature and stability update:

### 🌟 New Features
- **File & Playlist Opening**: Directly open supported audio files and M3U playlists using the new folder button in the search bar.
- **Collapsible Sidebar**: Implemented collapsible and snappable sidebar with a compact icon-only state for a cleaner layout.
- **Improved Collection Sorting**: Added header-based column sorting (including track numbers) and restored filter pills to all sub-tabs.
- **Virtualized Tracks List**: Enhanced `CollectionView` performance on large libraries via Svelte track list virtualization.

### ⚙️ Performance Benchmarks & Testing
- **100k Tracks Suite**: Scaled the performance benchmark suite to support 100k tracks to stress-test collection rendering, and added Vitest timeout parameters.

### 🐛 Compatibility & Bug Fixes
- **Linux WebKitGTK 2D Fallback**: Fixed rendering issues and pointer event bugs on Linux by falling back to 2D fade transitions.
- **Artist Fallback**: Resolved issues displaying "Various Artists" when multiple tracks shared the same artist metadata.
- **Icon Tweaks**: Refined the application icon and brand logo SVG viewBox to align better with taskbar proportions.
