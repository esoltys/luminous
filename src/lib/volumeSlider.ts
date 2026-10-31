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
 * Apply a volume slider's position; dragged past the safety ceiling, the
 * thumb snaps back to the level the backend applied (the store's value may
 * not change, so Svelte wouldn't move it).
 */
export async function setVolumeFromSlider(input: HTMLInputElement): Promise<void> {
  const requested = parseFloat(input.value);
  await playerStore.setVolume(requested);
  // Only on a cut: rewriting an unchanged level mid-drag would pull the
  // thumb back to a position the pointer has already left.
  if (playerStore.volume < requested) input.value = String(playerStore.volume);
}
