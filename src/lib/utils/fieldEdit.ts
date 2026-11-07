/**
 * One field of a multi-track tag edit, mirroring the backend's `FieldEdit`
 * (src-tauri/src/commands/tageditor.rs): `keep` leaves every track's own
 * value alone, `set` writes one value to all of them (`null` clears it).
 */
export type FieldEdit<T> = { mode: "keep" } | { mode: "set"; value: T | null };

/**
 * What a multi-track editor shows for one field: the value every track
 * shares, or `mixed` when they differ. Blank strings and missing values all
 * count as "no value", so a set where only some tracks carry it is mixed.
 */
export function sharedValue<T>(values: (T | null | undefined)[]): { value: T | null; mixed: boolean } {
  const normalized = values.map((v) => (v === undefined || v === null || v === "" ? null : v));
  if (normalized.length === 0) return { value: null, mixed: false };
  const first = normalized[0];
  return normalized.every((v) => v === first) ? { value: first, mixed: false } : { value: null, mixed: true };
}

/** The edit to send for a field: `set` once the user changed it, otherwise `keep`. */
export function fieldEdit<T>(edited: boolean, value: T | null): FieldEdit<T> {
  return edited ? { mode: "set", value } : { mode: "keep" };
}
