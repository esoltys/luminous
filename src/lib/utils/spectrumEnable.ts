import { invoke } from "@tauri-apps/api/core";

// The backend's `set_spectrum_enabled` command is a single global on/off
// flag (see src-tauri/src/audio.rs's `spectrum_enabled: AtomicBool`) shared
// by every consumer of "spectrum-data" events — currently ReactiveLogoBrand
// and SpectrumVisualizer. Each one calling invoke("set_spectrum_enabled", ...)
// directly from its own onMount/onDestroy is unsafe: it's not reference-
// counted, so one consumer's disable-on-unmount can silently kill spectrum
// data for another consumer that's still mounted and listening. Worse, a
// Svelte branch switch (e.g. crossing the playbar-only height breakpoint,
// or toggling Miniplayer) can destroy every consumer and recreate fresh ones
// within the same tick — even with reference counting, the count still dips
// through zero, and the resulting disable/enable pair of async IPC calls has
// no guaranteed resolve order, so a stale disable can land after the fresh
// enable and leave the backend stuck off.
//
// Debouncing the disable absorbs exactly that destroy-then-immediately-
// recreate pattern: a re-acquire within the window cancels the pending
// disable outright, so only a request that's genuinely going away for a
// while actually disables the backend flag.
const DISABLE_DEBOUNCE_MS = 250;

let refCount = 0;
let disableTimer: ReturnType<typeof setTimeout> | null = null;

export function acquireSpectrum(): void {
  refCount++;
  if (disableTimer) {
    clearTimeout(disableTimer);
    disableTimer = null;
  }
  if (refCount === 1) {
    void invoke("set_spectrum_enabled", { enabled: true }).catch((e) =>
      console.error("Failed to enable spectrum:", e)
    );
  }
}

export function releaseSpectrum(): void {
  refCount = Math.max(0, refCount - 1);
  if (refCount === 0) {
    disableTimer = setTimeout(() => {
      disableTimer = null;
      void invoke("set_spectrum_enabled", { enabled: false }).catch((e) =>
        console.error("Failed to disable spectrum:", e)
      );
    }, DISABLE_DEBOUNCE_MS);
  }
}
