/**
 * One field of a multi-track tag edit, mirroring the backend's `FieldEdit`
 * (src-tauri/src/commands/tageditor.rs): `keep` leaves every track's own
 * value alone, `set` writes one value to all of them (`null` clears it).
 */
export type FieldEdit<T> = { mode: "keep" } | { mode: "set"; value: T | null };

/**
 * A field of a multi-track editor whose tracks may disagree, such as the disc
 * number or disc subtitle of a multi-disc album. It shows the value every
 * track shares, or reads as mixed when they differ, and is only written once
 * the user edits it — so an untouched field never overwrites each track's own
 * value with one of them.
 */
export class MixedField<T> {
  value = $state<T | null>(null);
  /** The tracks disagree and the user hasn't typed a value for all of them. */
  showsMixed = $state(false);
  #edited = false;

  /** Loads the edited tracks' current values. Blank strings and missing
      values count as "no value", so a set where only some tracks carry one
      is mixed. */
  reset(values: (T | null | undefined)[]) {
    const normalized = values.map((v) => (v === undefined || v === null || v === "" ? null : v));
    const first = normalized[0] ?? null;
    const shared = normalized.every((v) => v === first);
    this.value = shared ? first : null;
    this.showsMixed = !shared;
    this.#edited = false;
  }

  /** Records the user's edit, which applies to every track. */
  set(value: T | null) {
    this.value = value;
    this.showsMixed = false;
    this.#edited = true;
  }

  /** Captures the field as it is now; calling the result puts it back,
      edited or not — for undoing a change the editor made on its own. */
  snapshot(): () => void {
    const { value, showsMixed } = this;
    const edited = this.#edited;
    return () => {
      this.value = value;
      this.showsMixed = showsMixed;
      this.#edited = edited;
    };
  }

  /** What to send to the save command. */
  get edit(): FieldEdit<T> {
    return this.#edited ? { mode: "set", value: this.value } : { mode: "keep" };
  }
}
