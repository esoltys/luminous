// Plot geometry shared by the parametric graph and band strip. These map
// values onto the screen only; the bounds themselves come from the backend's
// `EqRanges` (#1249), and the filter response is always backend-evaluated
// (#1248) — nothing here models a filter.

import type { SettingRange } from "../types/equalizer";

/** Height of the plot's SVG viewBox; y runs 0 (top) to PLOT_HEIGHT. */
export const PLOT_HEIGHT = 40;
const PLOT_MID = PLOT_HEIGHT / 2;
/** Headroom so a curve at full gain doesn't touch the plot edge. */
const PLOT_HALF_SPAN = 17;

/** Position of `freq` on a log axis over `range`, as 0..1. */
export function freqToUnit(freq: number, range: SettingRange): number {
  return Math.log(freq / range.min) / Math.log(range.max / range.min);
}

/** Inverse of `freqToUnit`. */
export function unitToFreq(unit: number, range: SettingRange): number {
  return range.min * Math.exp(unit * Math.log(range.max / range.min));
}

/** `count` log-spaced frequencies spanning `range` inclusive. */
export function logSpacedFreqs(count: number, range: SettingRange): number[] {
  return Array.from({ length: count }, (_, i) => unitToFreq(i / (count - 1), range));
}

/** Short axis label: 20, 200, 2k, 20k. */
export function formatFreq(freq: number): string {
  if (freq >= 10000) return `${(freq / 1000).toFixed(0)}k`;
  if (freq >= 1000) return `${(freq / 1000).toFixed(1).replace(/\.0$/, "")}k`;
  return `${Math.round(freq)}`;
}

/** Display precision for an edited frequency: 0.1 Hz in the bass, whole Hz above. */
export function roundFreq(freq: number): number {
  return freq < 100 ? Math.round(freq * 10) / 10 : Math.round(freq);
}

/** SVG y for a gain, symmetric about 0 dB and clamped to `gain`. */
export function dbToY(db: number, gain: SettingRange): number {
  const clamped = Math.max(gain.min, Math.min(gain.max, db));
  return PLOT_MID - (clamped / gain.max) * PLOT_HALF_SPAN;
}

/** Inverse of `dbToY` (unclamped). */
export function yToDb(y: number, gain: SettingRange): number {
  return ((PLOT_MID - y) / PLOT_HALF_SPAN) * gain.max;
}

/** Geometric midpoint of the widest gap on the log axis between the given
 * frequencies, with the range's own edges as boundaries — where a new band
 * lands without crowding an existing one. */
export function widestGapFreq(freqs: number[], range: SettingRange): number {
  const points = [range.min, ...freqs, range.max].sort((a, b) => a - b);
  let best = { ratio: 0, freq: Math.sqrt(range.min * range.max) };
  for (let i = 0; i < points.length - 1; i++) {
    const ratio = points[i + 1] / points[i];
    if (ratio > best.ratio) best = { ratio, freq: Math.sqrt(points[i] * points[i + 1]) };
  }
  return roundFreq(best.freq);
}
