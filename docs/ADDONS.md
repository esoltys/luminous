# Add-on themes

Advanced cosmetic themes (animated overlays above the player bar) are optional add-ons gated by Microsoft Store entitlement (epic #1036). All Luminous functionality stays open source and free; add-ons only change how it looks.

Status: the overlay runtime (#1413), bundle verifier (#1415), key-release client (#1416), Store entitlement (#1414) and the Settings section (#1417) are implemented, and the purchase and key release were verified end to end against the real Store on 2026-10-04. The key cache (#1429) is described below.

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

## Key cache (#1429)

The add-on key is the same for every owner. After the key Worker releases it, the app remembers it for 30 days (`GRACE` in `src-tauri/src/addons/keycache.rs`). Inside that window it is used without calling the Worker, so the Worker is asked about once a month instead of on every launch, and an owner keeps the add-on offline.

The Store is still asked about ownership on every launch, and a remembered key never overrides it: if the Store says the add-on is not owned, or the Worker refuses the account, the key is forgotten. If the Store cannot be asked at all (for example offline), a remembered key within its 30 days stands in for the answer. If the key has run out and nothing can be reached to renew it, the card asks the user to go online once (`reconfirm`). A key the bundle no longer decrypts with is discarded and fetched again. A key stamped in the future is treated as run out, so setting the clock back does not extend the 30 days.

The key is stored in `<app data>/addons/<id>.key`, sealed with Windows DPAPI for the signed-in user, together with the time of the last Worker release. Where no such protection exists, nothing is written and the key stays in memory, as before. The trade-off: the key moves from memory-only to protected on disk. Because it is the same for every owner, this exposes no more than reading it from the running app's memory.
