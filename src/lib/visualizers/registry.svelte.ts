import type { VisualizerPreset } from "./types";
import { VECTORSCOPE_PRESET } from "./vectorscope";
import { VOXEL_PRESET } from "./voxel";
import { VOXEL_LOGO_PRESET } from "./voxelLogo";

/** The preset shown when nothing (or something no longer registered) was saved. */
export const DEFAULT_PRESET: VisualizerPreset = VOXEL_PRESET;

// Built-ins in selector order. Add-on packs will register through `registerPreset` later.
// Reactive, so the selector updates when a preset registers or leaves; `raw` keeps the
// presets themselves unproxied, so identity comparisons hold.
let presets = $state.raw<readonly VisualizerPreset[]>([
  VOXEL_PRESET,
  VOXEL_LOGO_PRESET,
  VECTORSCOPE_PRESET,
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
