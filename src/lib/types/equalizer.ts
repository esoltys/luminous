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

export interface EqPresetPreview {
  key: string;
  response_db: number[];
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

/** Why `import_parametric_profile` / `read_eq_profile_file` refused a profile
 * (#1336). Crosses IPC as a JSON string; preset-store failures reuse the
 * `eq_presets` codes. Lines are 1-based. */
export type EqImportError =
  | { code: "empty" }
  | { code: "too_many_filters"; count: number; max: number }
  | { code: "unsupported_filter"; line: number; kind: string }
  | { code: "unsupported_line"; line: number }
  | { code: "malformed"; line: number }
  | { code: "out_of_range"; line: number; field: "freq" | "gain" | "q"; value: number; min: number; max: number }
  | { code: "preamp_out_of_range"; value: number; min: number; max: number }
  | { code: "file_too_large"; max_bytes: number }
  | { code: "read_failed" }
  | { code: "empty_name" | "duplicate_name" | "not_found" };
