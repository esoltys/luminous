import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { createFeedToggle } from "./feedToggle";

/** One `spectrum-data` event: the backend's `luminous_audio::VisualizerFrame`. */
export interface SpectrumFrame {
  /** 32 log-spaced bands, bass to treble, in [0, 1] relative to the frame's own loudest band. */
  bands: number[];
  /** Strength in (0, 1] of the strongest bass onset since the previous frame; 0 means none. */
  onset: number;
}

/**
 * Calls `handler` with each spectrum frame (~30 a second while playing). Data only
 * arrives while someone holds the feed with `acquireSpectrum`.
 */
export function listenSpectrum(handler: (frame: SpectrumFrame) => void): Promise<UnlistenFn> {
  return listen<SpectrumFrame>("spectrum-data", (event) => handler(event.payload));
}

// Shared by every consumer of "spectrum-data": ReactiveLogoBrand, AddonOverlay and
// VisualizerCanvas. See `createFeedToggle` for why it's reference-counted and debounced.
const spectrum = createFeedToggle("set_spectrum_enabled");

export const acquireSpectrum = spectrum.acquire;
export const releaseSpectrum = spectrum.release;
