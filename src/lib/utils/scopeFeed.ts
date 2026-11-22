import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { createFeedToggle } from "./feedToggle";

/**
 * One `scope-data` event: the backend's `luminous_audio::ScopeFrame`. Everything is
 * measured before master volume, so no value moves with the volume slider. Levels never
 * drop below −70 (silence), so every number is finite.
 */
export interface ScopeFrame {
  /** Up to 512 L/R pairs, interleaved `[l0, r0, l1, r1, …]`, from what played since the previous frame. Full scale is ±1. */
  points: number[];
  /** Phase correlation over the last 300 ms, in [-1, 1]; 0 for silence. */
  correlation: number;
  /** BS.1770 loudness over the last 400 ms, in LUFS. */
  momentaryLufs: number;
  /** BS.1770 loudness over the last 3 s, in LUFS. */
  shortTermLufs: number;
  /** Highest 4× oversampled peak in this frame, in dBTP (no hold). */
  truePeakDb: number;
  /** Right-minus-left level over the last 300 ms, in dB: positive leans right. */
  balanceDb: number;
  /** The meters started afresh (scope switched on, seek, new track, device change). */
  restarted: boolean;
}

/**
 * Calls `handler` with each scope frame (~30 a second while playing). Data only arrives
 * while someone holds the feed with `acquireScope`.
 */
export function listenScope(handler: (frame: ScopeFrame) => void): Promise<UnlistenFn> {
  return listen<ScopeFrame>("scope-data", (event) => handler(event.payload));
}

const scope = createFeedToggle("set_scope_enabled");

export const acquireScope = scope.acquire;
export const releaseScope = scope.release;
