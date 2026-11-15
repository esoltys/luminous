/**
 * Artist country helpers (#2095). The stored value is an uppercase ISO 3166-1
 * alpha-2 code; everything shown or typed goes through here, so the localized
 * names come from `Intl.DisplayNames` rather than a per-language table.
 */

export interface CountryOption {
  code: string;
  name: string;
}

/** Retired or non-country codes `Intl` still names (e.g. DD "Allemagne"), which would shadow a real country. */
const NOT_SELECTABLE = new Set([
  "AN",
  "BU",
  "CS",
  "DD",
  "FX",
  "NT",
  "SU",
  "TP",
  "YD",
  "YU",
  "ZR",
  "EU",
  "UN",
  "EZ",
  "QO",
  "ZZ",
]);

const LETTERS = "ABCDEFGHIJKLMNOPQRSTUVWXYZ";

function regionNames(locale: string): Intl.DisplayNames | null {
  try {
    return new Intl.DisplayNames([locale], { type: "region", fallback: "none" });
  } catch {
    return null;
  }
}

/** The localized name of an alpha-2 code, or "" when it isn't a real region. */
export function countryDisplayName(code: string | null | undefined, locale = "en"): string {
  const clean = code?.trim().toUpperCase();
  if (!clean || !/^[A-Z]{2}$/.test(clean) || NOT_SELECTABLE.has(clean)) return "";
  try {
    return regionNames(locale)?.of(clean) ?? "";
  } catch {
    return "";
  }
}

/** Every assignable region, named in `locale` and sorted by that name. */
export function countryOptions(locale = "en"): CountryOption[] {
  const names = regionNames(locale);
  if (!names) return [];
  const options: CountryOption[] = [];
  for (const a of LETTERS) {
    for (const b of LETTERS) {
      const code = a + b;
      const name = names.of(code);
      // `fallback: "none"` yields undefined for unassigned codes.
      if (name && name !== code && !NOT_SELECTABLE.has(code)) options.push({ code, name });
    }
  }
  return options.sort((x, y) => x.name.localeCompare(y.name, locale));
}

/**
 * Reads what the user typed into the country field: a code ("ca"), or a name
 * in the UI language or English ("Canada"). `null` means unrecognised, and
 * "" (blank) means "no country", so the caller can tell clearing from a typo.
 */
export function parseCountryInput(input: string, locale = "en"): string | null | "" {
  const text = input.trim();
  if (!text) return "";
  if (/^[A-Za-z]{2}$/.test(text)) {
    const code = text.toUpperCase();
    return countryDisplayName(code, "en") ? code : null;
  }
  const wanted = text.toLocaleLowerCase(locale);
  for (const loc of new Set([locale, "en"])) {
    const hit = countryOptions(loc).find((o) => o.name.toLocaleLowerCase(loc) === wanted);
    if (hit) return hit.code;
  }
  return null;
}
