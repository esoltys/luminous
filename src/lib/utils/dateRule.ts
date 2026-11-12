/**
 * Smart-playlist rules on date fields (`added`, `lastplayed`).
 *
 * A rule keeps the filter grammar's own shape — `{ op, value }` where the value
 * is a `YYYY-MM-DD` day or a `-<n><unit>` offset back from today — so what the
 * builder edits is exactly what is saved, and reopening a playlist needs no
 * conversion. What a day or an offset *means* is the backend's call
 * (`filter_parser.rs`); this module only maps the grammar onto the choices the
 * builder offers.
 */

export const DATE_FIELDS: ReadonlySet<string> = new Set(["lastplayed", "added"]);

export type RelativeUnit = "d" | "w" | "m" | "y";
export const RELATIVE_UNITS: readonly RelativeUnit[] = ["d", "w", "m", "y"];

/** What the operator picker offers for a date field. */
export type DateOpChoice = "=" | "!=" | "<" | ">" | "<=" | ">=" | "within" | "outside";

export const DATE_OP_CHOICES: readonly DateOpChoice[] = [
  "=",
  "!=",
  "<",
  ">",
  "<=",
  ">=",
  "within",
  "outside",
];

const DEFAULT_RELATIVE = "-30d";
const RELATIVE_RE = /^-(\d+)([dwmy])$/i;

export function parseRelative(value: string): { count: number; unit: RelativeUnit } | null {
  const m = RELATIVE_RE.exec(value.trim());
  if (!m) return null;
  return { count: Number(m[1]), unit: m[2].toLowerCase() as RelativeUnit };
}

export function formatRelative(count: number, unit: RelativeUnit): string {
  return `-${count}${unit}`;
}

/** The picker choice a stored rule corresponds to. "In the last" is `>=` with
 * an offset, "not in the last" is `<` with one; anything else is a plain
 * comparison against a day. */
export function dateOpChoice(op: string, value: string): DateOpChoice {
  const relative = parseRelative(value) !== null;
  if (relative && op === ">=") return "within";
  if (relative && op === "<") return "outside";
  return (DATE_OP_CHOICES as readonly string[]).includes(op) ? (op as DateOpChoice) : "=";
}

/** The `{ op, value }` a rule takes on when the user picks `choice`. Switching
 * between a relative window and a calendar day clears the value — an offset is
 * not a valid day and vice versa — so the rule reads as empty until filled in. */
export function applyDateOpChoice(
  choice: DateOpChoice,
  current: { op: string; value: string },
): { op: string; value: string } {
  const wasRelative = parseRelative(current.value) !== null;
  if (choice === "within")
    return { op: ">=", value: wasRelative ? current.value : DEFAULT_RELATIVE };
  if (choice === "outside")
    return { op: "<", value: wasRelative ? current.value : DEFAULT_RELATIVE };
  return { op: choice, value: wasRelative ? "" : current.value };
}

function pad(n: number): string {
  return String(n).padStart(2, "0");
}

/** A rule saved with a raw epoch-seconds value (the only form before date
 * support) is shown as the local day it falls on, so the date picker can hold it. */
export function legacyTimestampToDay(value: string): string {
  if (!/^\d{5,}$/.test(value.trim())) return value;
  const d = new Date(Number(value) * 1000);
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}`;
}
