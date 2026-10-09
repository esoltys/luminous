import { de } from "./de";
import { en } from "./en";
import { enGB } from "./en-GB";
import { enUS } from "./en-US";
import { es } from "./es";
import { fr } from "./fr";
import { frFR } from "./fr-FR";
import { it } from "./it";
import { ru } from "./ru";
import { uk } from "./uk";

export type Messages = { [key: string]: string | Messages };

export interface LocaleDef {
  /** BCP 47 tag; passed straight to Intl and `<html lang>`. */
  tag: string;
  messages: Messages;
  /** Tried for a key this locale lacks, before the base locale. */
  fallback?: string;
}

/** Complete catalog every other locale is measured against and falls back to. */
export const BASE_LOCALE = "en-CA";

/**
 * Every shipped UI locale. A new language is one catalog file plus one entry here;
 * `Locale`, the Settings list, and the saved-setting check all derive from it.
 */
export const LOCALES = [
  { tag: "de", messages: de },
  { tag: "en-CA", messages: en },
  { tag: "en-GB", messages: enGB, fallback: "en-CA" },
  { tag: "en-US", messages: enUS, fallback: "en-CA" },
  { tag: "es", messages: es },
  { tag: "fr-CA", messages: fr },
  { tag: "fr", messages: frFR },
  { tag: "it", messages: it },
  { tag: "ru", messages: ru },
  { tag: "uk", messages: uk },
] as const satisfies readonly LocaleDef[];

export type Locale = (typeof LOCALES)[number]["tag"];

/** Value `language` held before locales were BCP 47 tags (builds up to this registry). */
const LEGACY_LANGUAGE_ALIASES: Record<string, Locale> = { en: "en-CA", fr: "fr-CA" };

const byTag = new Map<string, LocaleDef>(LOCALES.map((def) => [def.tag, def]));

export function isLocale(tag: unknown): tag is Locale {
  return typeof tag === "string" && byTag.has(tag);
}

/** Maps a `language` value saved by a pre-registry build to its tag, or null if unknown. */
export function legacyLanguageToLocale(saved: string): Locale | null {
  return LEGACY_LANGUAGE_ALIASES[saved] ?? null;
}

/** Catalogs to search for a key, most specific first, always ending at the base locale. */
export function catalogChain(tag: string): Messages[] {
  const chain: Messages[] = [];
  const seen = new Set<string>();
  let next: string | undefined = tag;
  while (next && !seen.has(next)) {
    seen.add(next);
    const def = byTag.get(next);
    if (!def) break;
    chain.push(def.messages);
    next = def.fallback;
  }
  if (!seen.has(BASE_LOCALE)) chain.push(byTag.get(BASE_LOCALE)!.messages);
  return chain;
}

/** A locale's name in its own language, e.g. "Français (Canada)". */
export function localeLabel(tag: string): string {
  try {
    const name = new Intl.DisplayNames(tag, { type: "language", languageDisplay: "standard" }).of(tag);
    if (name) return name.charAt(0).toLocaleUpperCase(tag) + name.slice(1);
  } catch {
    // Intl.DisplayNames unavailable or tag rejected: fall through to the bare tag.
  }
  return tag;
}

/** Locales listed first in the language picker, in this order. */
const PINNED_LOCALES: readonly Locale[] = ["en-CA", "fr-CA"];

/**
 * The language picker's options: the pinned locales, then everything else sorted by its
 * own-language label, collated with one fixed locale so a non-Latin script does not reorder the list. The picker draws a separator between the groups when `rest` is non-empty.
 */
export function localePickerGroups(): { pinned: Locale[]; rest: Locale[] } {
  const tags = LOCALES.map((def) => def.tag);
  const pinned = PINNED_LOCALES.filter((tag) => tags.includes(tag));
  const rest = tags
    .filter((tag) => !pinned.includes(tag))
    .sort((a, b) => localeLabel(a).localeCompare(localeLabel(b), BASE_LOCALE));
  return { pinned, rest };
}

/** User guide languages shipped in `docs/user-guide/luminous-user-guide-{code}.html`: one per UI language. */
const MANUAL_LANGUAGES = ["EN", "FR", "ES"] as const;

export type ManualLanguage = (typeof MANUAL_LANGUAGES)[number];

/** The manual language implied by a UI locale, by base language (`fr-CA` gives FR); English when no guide matches. */
export function manualLanguageForLocale(tag: string): ManualLanguage {
  const base = tag.toLowerCase().split("-")[0].toUpperCase();
  return MANUAL_LANGUAGES.find((code) => code === base) ?? "EN";
}
