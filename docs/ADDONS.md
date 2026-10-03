# Add-on themes

Advanced cosmetic themes (animated overlays above the player bar) are optional add-ons gated by Microsoft Store entitlement (epic #1036). All Luminous functionality stays open source and free; add-ons only change how it looks.

Status: the overlay runtime (#1413) is implemented. The bundle verifier (#1415), key-release client (#1416) and Store entitlement (#1414) are not, and the Partner Center behaviours they depend on are unverified.

## Overlay runtime

An overlay is an HTML page from the decrypted bundle, loaded in an `<iframe sandbox="allow-scripts">` above the player bar. The frame has an opaque origin and cannot reach Tauri IPC, the network or storage. It is served from memory by the `luminous-addon` URI scheme (`src-tauri/src/addons/mod.rs`) with a restrictive CSP (`default-src 'none'`, scripts, styles, images, fonts and media from `'self'` only, `connect-src 'none'`). Only assets registered by the loader are served; anything else, and any path with `..`, an empty segment or a backslash, is a 404.

The frame is 240 CSS px tall, spans the bar's width, sits on the bar's top edge, and is `pointer-events: none` and `aria-hidden`. The overlay positions its own content inside it.

### API version

`OVERLAY_API_VERSION = 1` (`src-tauri/src/addons/mod.rs`). Manifests declare the minimum they need as `minApiVersion`. Unknown optional manifest fields must be tolerated, so a new optional field never breaks an older app.

### Messages (host to overlay)

One-way `window.postMessage`. Every message is `{ luminousAddon: 1, kind, ... }`.

| `kind` | Fields | When |
| --- | --- | --- |
| `state` | `isPlaying`, `positionMs`, `track` (`{title, artist, album}` or `null`), `accent` (`#rrggbb`), `reducedMotion` | after load and on any change |
| `size` | `width`, `height` (CSS px) | after load and on resize |
| `spectrum` | `bass`, `mid`, `treble` (0..1), `bars` (0..1 each) | about 30 Hz, only with `capabilities: ["spectrum"]` |

An overlay must honour `reducedMotion` by showing a static frame. The host acquires spectrum data only for add-ons that opt in, through the ref-counted `acquireSpectrum()` / `releaseSpectrum()`.

### Bundle conventions

- `overlayEntry` is a bundle path to an HTML file, normally `overlay.html`. Relative URLs inside it resolve within the bundle and must be declared in `manifest.assets`.
- `colors` has exactly the eight theme keys, each `#rrggbb`.
- The decrypted archive is capped at 25 MB (enforced by the packer, and by the verifier in #1415).
