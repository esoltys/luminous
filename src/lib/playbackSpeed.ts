import { formatNumber } from "./stores/i18n.svelte";

/** The panel's range and step (#2142). The backend clamps and snaps to the same grid, so
 * these shape the controls; they don't guarantee anything. */
export const PLAYBACK_SPEED_MIN = 0.25;
export const PLAYBACK_SPEED_MAX = 3;
export const PLAYBACK_SPEED_STEP = 0.05;
export const PLAYBACK_SPEED_NORMAL = 1;
/** One-tap presets, slowest first. */
export const PLAYBACK_SPEED_PRESETS = [1, 1.25, 1.5, 2, 3] as const;

/** `speed` kept inside the range and on the step grid, as the backend will apply it. */
export function snapPlaybackSpeed(speed: number): number {
  if (!Number.isFinite(speed)) return PLAYBACK_SPEED_NORMAL;
  const clamped = Math.min(Math.max(speed, PLAYBACK_SPEED_MIN), PLAYBACK_SPEED_MAX);
  return Math.round(clamped * 20) / 20;
}

/** "1.25×" in the app's number format; always two decimals so the readout doesn't jump width. */
export function formatPlaybackSpeed(speed: number): string {
  return `${formatNumber(speed, { minimumFractionDigits: 2, maximumFractionDigits: 2 })}×`;
}

/** "1.25×" for a preset chip: no trailing zeros ("1×", "1.5×"). */
export function formatPlaybackSpeedShort(speed: number): string {
  return `${formatNumber(speed, { maximumFractionDigits: 2 })}×`;
}
