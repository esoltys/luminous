// Mirrors `src-tauri/src/equalizer.rs`. The backend clamps every value and
// echoes the canonical config; `EqRanges` carries the bounds it clamps to.

export type EqMode = "graphic10" | "parametric";

export type ParametricKind = "peak" | "low_shelf" | "high_shelf";

export interface ParametricBand {
  kind: ParametricKind;
  freq: number;
  gain_db: number;
  q: number;
  enabled: boolean;
}

export interface EqConfig {
  enabled: boolean;
  mode: EqMode;
  preamp: number;
  gains: number[];
  parametric: ParametricBand[];
  /** Backend-owned: a built-in preset name, `user:<id>`, or null (Custom). */
  active_preset?: string | null;
}

/** A saved user preset (parametric filter list + preamp), by id. */
interface UserPreset {
  id: number;
  name: string;
}

export interface EqPresetList {
  builtin: string[];
  user: UserPreset[];
}

/** The picker key for a user preset — mirrors `equalizer::user_preset_key`. */
export const userPresetKey = (id: number): string => `user:${id}`;

export interface SettingRange {
  min: number;
  max: number;
}

export interface EqRanges {
  freq: SettingRange;
  gain_db: SettingRange;
  q: SettingRange;
  preamp: SettingRange;
  max_bands: number;
  min_bands: number;
}
