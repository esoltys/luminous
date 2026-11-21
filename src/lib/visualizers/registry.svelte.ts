import { DEV_TEST_PATTERN_PRESET } from "./devTestPattern";
import type { VisualizerPreset } from "./types";
import { VOXEL_PRESET } from "./voxel";

/** The preset shown when nothing (or something no longer registered) was saved. */
export const DEFAULT_PRESET: VisualizerPreset = VOXEL_PRESET;

// Built-ins in selector order. Add-on packs will register through `registerPreset` later.
// The dev test pattern is left out of unit tests so they see only what ships.
// Reactive, so the selector updates when a preset registers or leaves; `raw` keeps the
// presets themselves unproxied, so identity comparisons hold.
let presets = $state.raw<readonly VisualizerPreset[]>([
  VOXEL_PRESET,
  ...(import.meta.env.DEV && import.meta.env.MODE !== "test" ? [DEV_TEST_PATTERN_PRESET] : []),
]);

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
  presets = [...presets.filter((p) => p.id !== preset.id), preset];
  return () => {
    presets = presets.filter((p) => p !== preset);
  };
}
