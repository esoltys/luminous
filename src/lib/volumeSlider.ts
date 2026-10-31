import { playerStore } from "./stores/player.svelte";
import type { VolumeSafety } from "./types";

/**
 * The track background for a 0–1 volume slider: filled up to `volume`, and,
 * while the safety ceiling (#1890) is on, a notch at the ceiling with the
 * range above it dimmed as locked.
 */
export function volumeSliderBackground(volume: number, safety: VolumeSafety | null): string {
  const filled = `var(--color-accent) 0%, var(--color-accent) ${volume * 100}%`;
  if (!safety?.enabled) {
    return `background: linear-gradient(to right, ${filled}, var(--color-border) ${volume * 100}%, var(--color-border) 100%)`;
  }
  const ceiling = safety.ceiling * 100;
  const locked = "color-mix(in srgb, var(--color-border) 40%, transparent)";
  return (
    `background: linear-gradient(to right, ${filled}, ` +
    `var(--color-border) ${volume * 100}%, var(--color-border) calc(${ceiling}% - 1px), ` +
    `var(--color-text-secondary) calc(${ceiling}% - 1px), var(--color-text-secondary) calc(${ceiling}% + 1px), ` +
    `${locked} calc(${ceiling}% + 1px), ${locked} 100%)`
  );
}

/**
 * Apply a volume slider's position, then move the thumb to the level the
 * backend actually applied: dragged past the safety ceiling, it snaps back
 * to the ceiling (the store's value may not change, so Svelte wouldn't
 * move it).
 */
export async function setVolumeFromSlider(input: HTMLInputElement): Promise<void> {
  await playerStore.setVolume(parseFloat(input.value));
  input.value = String(playerStore.volume);
}
