import { DEV_TEST_PATTERN_PRESET } from "./devTestPattern";
import type { VisualizerPreset } from "./types";
import { VOXEL_PRESET } from "./voxel";

/** The preset shown when nothing (or something no longer registered) was saved. */
export const DEFAULT_PRESET: VisualizerPreset = VOXEL_PRESET;

// Built-ins in selector order. Add-on packs will register through `registerPreset` later.
// The dev test pattern is left out of unit tests so they see only what ships.
const presets: VisualizerPreset[] = [
  VOXEL_PRESET,
  ...(import.meta.env.DEV && import.meta.env.MODE !== "test" ? [DEV_TEST_PATTERN_PRESET] : []),
];

/** Every registered preset, in selector order. */
export function listPresets(): readonly VisualizerPreset[] {
  return presets;
}

/** The preset with `id`, or the default one if no such preset is registered. */
export function getPreset(id: string | null | undefined): VisualizerPreset {
  return presets.find((p) => p.id === id) ?? DEFAULT_PRESET;
}

/**
 * Adds `preset` to the end of the list (replacing one with the same id) and returns a
 * function that removes it again.
 */
export function registerPreset(preset: VisualizerPreset): () => void {
  const existing = presets.findIndex((p) => p.id === preset.id);
  if (existing >= 0) presets.splice(existing, 1);
  presets.push(preset);
  return () => {
    const i = presets.indexOf(preset);
    if (i >= 0) presets.splice(i, 1);
  };
}
