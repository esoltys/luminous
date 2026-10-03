import { i18n, formatNumber } from "../stores/i18n.svelte";
import type { EqImportError } from "../types/equalizer";

/** Preset name for a picked profile file: the file name without its
 * extension and AutoEq's trailing "ParametricEQ" (any case), e.g.
 * `…/Sennheiser HD 600 ParametricEQ.txt` → `Sennheiser HD 600`. */
export function profileNameFromPath(path: string): string {
  const file = path.split(/[\\/]/).pop() ?? "";
  return file
    .replace(/\.[^.]+$/, "")
    .replace(/\s*parametric\s*eq$/i, "")
    .trim();
}

function parseImportError(raw: unknown): EqImportError | null {
  try {
    const value = JSON.parse(String(raw));
    return value && typeof value.code === "string" ? (value as EqImportError) : null;
  } catch {
    return null;
  }
}

const num = (n: number) => formatNumber(n, { maximumFractionDigits: 2 });

/** The localized message for a failed import; unknown errors get a generic one. */
export function importErrorMessage(raw: unknown): string {
  const err = parseImportError(raw);
  switch (err?.code) {
    case "empty":
      return i18n.t("equalizer.importErrorEmpty");
    case "too_many_filters":
      return i18n.t("equalizer.importErrorTooManyFilters", { count: err.count, max: err.max });
    case "unsupported_filter":
      return i18n.t("equalizer.importErrorUnsupportedFilter", { line: err.line, kind: err.kind });
    case "unsupported_line":
      return i18n.t("equalizer.importErrorUnsupportedLine", { line: err.line });
    case "malformed":
      return i18n.t("equalizer.importErrorMalformed", { line: err.line });
    case "out_of_range": {
      const field = { freq: "equalizer.frequency", gain: "equalizer.gain", q: "equalizer.qFactor" }[err.field];
      return i18n.t("equalizer.importErrorOutOfRange", {
        line: err.line,
        field: i18n.t(field),
        value: num(err.value),
        min: num(err.min),
        max: num(err.max),
      });
    }
    case "preamp_out_of_range":
      return i18n.t("equalizer.importErrorPreampOutOfRange", {
        value: num(err.value),
        min: num(err.min),
        max: num(err.max),
      });
    case "file_too_large":
      return i18n.t("equalizer.importErrorFileTooLarge", { kb: Math.round(err.max_bytes / 1024) });
    case "read_failed":
      return i18n.t("equalizer.importErrorReadFailed");
    case "empty_name":
      return i18n.t("equalizer.presetErrorEmptyName");
    case "duplicate_name":
      return i18n.t("equalizer.presetErrorDuplicateName");
    default:
      return i18n.t("equalizer.importErrorGeneric");
  }
}
