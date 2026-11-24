import { clampReferenceDbfs } from "../visualizers/vuMeterEngine";

// Saved in localStorage, like the visualizer choice (visualizer_preset): it is read
// synchronously, so the meters are calibrated from the first frame.
const STORAGE_KEY = "vu_meter_reference_dbfs";

function readSaved(): number | null {
  try {
    const raw = typeof localStorage !== "undefined" ? localStorage.getItem(STORAGE_KEY) : null;
    if (raw === null) return null;
    const value = Number(raw);
    return Number.isFinite(value) ? clampReferenceDbfs(value) : null;
  } catch {
    return null;
  }
}

/**
 * The VU meter's calibration. `referenceDbfs` is the per-channel RMS level that reads 0 VU
 * when the user set one by hand, and null while the meter calibrates itself (from the
 * normalization target, or a typical master when normalization is off). Exported for
 * tests, where a fresh instance stands in for a relaunch.
 */
export class VuMeterSettingsStore {
  referenceDbfs = $state<number | null>(readSaved());

  /** Sets the 0 VU level by hand; it is held within the allowed range. */
  setManual(dbfs: number): void {
    const level = clampReferenceDbfs(dbfs);
    this.referenceDbfs = level;
    try {
      localStorage.setItem(STORAGE_KEY, String(level));
    } catch {
      // Storage unavailable: the level still holds for this session.
    }
  }

  /** Hands calibration back to the meter. */
  setAuto(): void {
    this.referenceDbfs = null;
    try {
      localStorage.removeItem(STORAGE_KEY);
    } catch {
      // Storage unavailable: the choice still holds for this session.
    }
  }
}

export const vuMeterSettings = new VuMeterSettingsStore();
