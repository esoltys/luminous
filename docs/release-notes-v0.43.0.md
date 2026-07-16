Luminous v0.43.0 introduces a major visual redesign, performance tuning, and tag metadata updates:

### 🌟 New Features & Redesign
- **Three-Column Grid Shell**: Fully redesigned the main user interface layout with a top navigation ribbon for a more modern appearance.
- **Immersive 3D Album Theater**: Pivots to a full-screen immersive album artwork view when bottom panel is toggled, including a premium 3D card flip transition.
- **Audio Visualizer Header Logo**: Implemented multi-band pulsing mapped directly to Bass (glow), Mids (outer ring), and Treble (inner burst) frequencies.
- **AcoustID Tag Lookups**: Added automatic AcoustID metadata matching and fingerprint lookup, with error-handling, logger masking, and setup guidelines in the TagEditor UI.
- **Global Keyboard Media Keys**: Integrated global media keys support for play, pause, next, and previous.

### 🎨 Customization & Style Tweaks
- **Theme Adjustments**: Nordic Blue is now the default app theme when no preference is saved. Theme custom properties are fully reactive on initial load.
- **Custom Volume Slider**: Replaced default input range with a styled, theme-matching custom volume bar.

### 🐛 Stability & Backend Improvements
- **ALSA Buffer Fix**: Resolved Linux ALSA PCM underruns using a lock-free Single Producer Single Consumer (SPSC) ring buffer.
- **Soft-Delete Mode**: Preserves playlist metadata when referenced audio files are missing from the filesystem.
- **Consolidated Albums**: Properly groups "Various Artists" compilations.
