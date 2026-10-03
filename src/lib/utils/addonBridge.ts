import { isWindows } from "../platform";

/**
 * Host→overlay message API for add-on overlays (#1413). One-way: the overlay
 * runs in a sandboxed iframe (`sandbox="allow-scripts"`, opaque origin, no
 * Tauri IPC) and receives these via `window.postMessage`. Documented in
 * docs/ADDONS.md. The version is `OVERLAY_API_VERSION` in
 * src-tauri/src/addons/mod.rs; bump it when a shape changes incompatibly.
 */

interface OverlayTrack {
  title: string;
  artist: string;
  album: string;
}

export type OverlayMessage =
  | {
      luminousAddon: 1;
      kind: "state";
      isPlaying: boolean;
      positionMs: number;
      track: OverlayTrack | null;
      accent: string;
      reducedMotion: boolean;
    }
  | { luminousAddon: 1; kind: "size"; width: number; height: number }
  | {
      luminousAddon: 1;
      kind: "spectrum";
      bass: number;
      mid: number;
      treble: number;
      bars: number[];
    };

/**
 * URL the overlay iframe loads. Windows WebView2 only intercepts
 * `http://<scheme>.localhost`; elsewhere the custom scheme is used directly
 * (same split as `getCoverArtUrl`, #715). Path segments are percent-encoded.
 */
export function addonFrameUrl(id: string, entry: string, windows: boolean = isWindows): string {
  const path = [id, ...entry.split("/")].map(encodeURIComponent).join("/");
  return windows ? `http://luminous-addon.localhost/${path}` : `luminous-addon://localhost/${path}`;
}

const clamp01 = (v: number) => (Number.isFinite(v) ? Math.min(1, Math.max(0, v)) : 0);

function average(bins: number[], from: number, to: number): number {
  const slice = bins.slice(from, to);
  return slice.length ? slice.reduce((sum, v) => sum + v, 0) / slice.length : 0;
}

/**
 * Collapse the backend's 32 spectrum bins into bass/mid/treble plus the raw
 * bars, using the same bin ranges as the logo (`ReactiveLogoBrand`).
 */
export function spectrumMessage(bins: number[]): OverlayMessage {
  return {
    luminousAddon: 1,
    kind: "spectrum",
    bass: clamp01(average(bins, 0, 8)),
    mid: clamp01(average(bins, 8, 20)),
    treble: clamp01(average(bins, 20, 32)),
    bars: bins.map(clamp01)
  };
}
