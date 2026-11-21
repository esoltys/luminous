import { getPreset, listPresets } from "../visualizers/registry";
import type { VisualizerPreset } from "../visualizers/types";

// Saved in localStorage, like the immersive view next to it (layout_immersiveView): it is
// read synchronously, so the saved preset is drawn from the first frame.
const STORAGE_KEY = "visualizer_preset";

function readSaved(): string | null {
  try {
    return typeof localStorage !== "undefined" ? localStorage.getItem(STORAGE_KEY) : null;
  } catch {
    return null;
  }
}

/**
 * The selected visualizer preset, shared by the playbar mini-view and the immersive view.
 * Exported for tests, where a fresh instance stands in for a relaunch.
 */
export class VisualizerStore {
  // The id as saved, even if no such preset is registered right now (e.g. a pack that is
  // disabled): `preset` falls back to the default, and the choice comes back with the pack.
  private selectedId = $state<string | null>(readSaved());

  /** Bumped on every user switch, so the selector can flash the new name. */
  switchCount = $state(0);

  get preset(): VisualizerPreset {
    return getPreset(this.selectedId);
  }

  get presets(): readonly VisualizerPreset[] {
    return listPresets();
  }

  /** True when there is more than one preset to choose between. */
  get canSwitch(): boolean {
    return listPresets().length > 1;
  }

  /** The preset `step` places away from the current one, wrapping around the list. */
  neighbour(step: 1 | -1): VisualizerPreset {
    const list = listPresets();
    const i = list.indexOf(this.preset);
    return list[(i + step + list.length) % list.length];
  }

  select(id: string): void {
    const preset = getPreset(id);
    if (preset.id !== id || preset === this.preset) return;
    this.selectedId = id;
    this.switchCount++;
    try {
      localStorage.setItem(STORAGE_KEY, id);
    } catch {
      // Storage unavailable: the choice still holds for this session.
    }
  }

  /** Selects the next (`1`) or previous (`-1`) preset. */
  cycle(step: 1 | -1): void {
    this.select(this.neighbour(step).id);
  }
}

export const visualizerStore = new VisualizerStore();
