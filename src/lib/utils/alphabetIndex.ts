/**
 * Alphabetical indexing engine for jumping across sorted lists (#1973).
 *
 * Precomputes letter boundaries for an alphabetically sorted collection in O(N)
 * time, providing O(1) index lookups during user clicks and scrub gestures.
 */

interface LetterSection {
  /** The display label: '#' (for numbers/symbols) or 'A' through 'Z'. */
  letter: string;
  /** Index of the first matching item in the list, or -1 if no items start with this letter. */
  startIndex: number;
  /** Total count of items starting with this letter. */
  count: number;
}

export interface AlphabetIndexResult {
  /** Ordered list of letter sections to render in the scrubber. */
  sections: LetterSection[];
  /**
   * Resolves the target item index to scroll to for a given letter.
   * If the requested letter has no matching items, resolves to the nearest
   * populated letter (subsequent letter preferred; preceding if at the end).
   * Returns -1 if the collection is empty.
   */
  getJumpIndex: (letter: string) => number;
}

const LATIN_LETTERS_ASC = [
  "#", "A", "B", "C", "D", "E", "F", "G", "H", "I", "J", "K", "L", "M",
  "N", "O", "P", "Q", "R", "S", "T", "U", "V", "W", "X", "Y", "Z"
] as const;

const SPANISH_LETTERS_ASC = [
  "#", "A", "B", "C", "D", "E", "F", "G", "H", "I", "J", "K", "L", "M",
  "N", "Ñ", "O", "P", "Q", "R", "S", "T", "U", "V", "W", "X", "Y", "Z"
] as const;

const RUSSIAN_LETTERS_ASC = [
  "#", "А", "Б", "В", "Г", "Д", "Е", "Ж", "З", "И", "К", "Л", "М",
  "Н", "О", "П", "Р", "С", "Т", "У", "Ф", "Х", "Ц", "Ч", "Ш", "Щ",
  "Э", "Ю", "Я"
] as const;

const UKRAINIAN_LETTERS_ASC = [
  "#", "А", "Б", "В", "Г", "Ґ", "Д", "Е", "Є", "Ж", "З", "И", "І",
  "Ї", "К", "Л", "М", "Н", "О", "П", "Р", "С", "Т", "У", "Ф", "Х",
  "Ц", "Ч", "Ш", "Щ", "Ю", "Я"
] as const;

export const BASE_LETTERS_ASC = LATIN_LETTERS_ASC;
export const BASE_LETTERS_DESC = [...LATIN_LETTERS_ASC].reverse();

/**
 * Returns the ascending and descending alphabet character lists for a given locale.
 */
function getAlphabetForLocale(locale = "en"): {
  asc: readonly string[];
  desc: readonly string[];
  lang: string;
} {
  const lang = (locale || "en").toLowerCase().split("-")[0];
  let asc: readonly string[];

  if (lang === "ru") {
    asc = RUSSIAN_LETTERS_ASC;
  } else if (lang === "uk") {
    asc = UKRAINIAN_LETTERS_ASC;
  } else if (lang === "es") {
    asc = SPANISH_LETTERS_ASC;
  } else {
    asc = LATIN_LETTERS_ASC;
  }

  const desc = [...asc].reverse();
  return { asc, desc, lang };
}

/**
 * Normalizes an item key string to determine its alphabet bucket ('#' or alphabet letters).
 *
 * Strips leading quotes/brackets and normalizes Unicode diacritics according to the locale
 * (e.g. Spanish preserves 'Ñ', Russian merges 'Ё' -> 'Е' and 'Й' -> 'И', Latin strips diacritics).
 */
export function extractBucketLetter(str: string | null | undefined, locale = "en"): string {
  if (!str) return "#";
  const trimmed = str.trim();
  if (!trimmed) return "#";

  // Strip leading punctuation/quotes/brackets
  const cleaned = trimmed.replace(/^["'([{‘“«\s]+/, "");
  if (!cleaned) return "#";

  const { lang, asc } = getAlphabetForLocale(locale);
  const firstChar = cleaned.charAt(0);

  if (lang === "ru") {
    const upper = firstChar.toUpperCase();
    if (upper === "Ё") return "Е";
    if (upper === "Й") return "И";
    if (asc.includes(upper)) return upper;
    return "#";
  }

  if (lang === "uk") {
    const upper = firstChar.toUpperCase();
    if (upper === "Й") return "И";
    if (asc.includes(upper)) return upper;
    return "#";
  }

  if (lang === "es") {
    const upper = firstChar.toUpperCase();
    if (upper === "Ñ") return "Ñ";
  }

  // Decompose diacritics and strip combining marks for Latin letters
  const char = firstChar.normalize("NFD").replace(/[\u0300-\u036f]/g, "").toUpperCase();
  if (char >= "A" && char <= "Z") {
    return char;
  }
  return "#";
}

/**
 * Computes alphabet sections and jump lookup for a sorted list of items.
 *
 * @param items The sorted collection of items.
 * @param getKey Extractor function returning the string sort key for each item.
 * @param ascending True if items are sorted ascending; false if descending.
 * @param locale The active BCP 47 UI locale code (e.g. 'en-CA', 'ru', 'es', 'uk').
 */
export function buildAlphabetIndex<T>(
  items: T[],
  getKey: (item: T) => string | null | undefined,
  ascending = true,
  locale = "en"
): AlphabetIndexResult {
  // For Cyrillic locales (ru, uk), if the collection contains zero Cyrillic items but has
  // Latin items, adapt to Latin so the scrubber remains functional for foreign music collections.
  let effectiveLocale = locale;
  const lang = (locale || "en").toLowerCase().split("-")[0];
  if (lang === "ru" || lang === "uk") {
    const hasCyrillic = items.some((item) => {
      const k = getKey(item);
      if (!k) return false;
      const ch = k.trim().replace(/^["'([{‘“«\s]+/, "").charAt(0);
      return /[\u0400-\u04FF]/.test(ch);
    });
    if (!hasCyrillic && items.length > 0) {
      effectiveLocale = "en";
    }
  }

  const { asc, desc } = getAlphabetForLocale(effectiveLocale);
  const baseLetters = ascending ? asc : desc;

  if (items.length === 0) {
    return {
      sections: baseLetters.map((letter) => ({ letter, startIndex: -1, count: 0 })),
      getJumpIndex: () => -1,
    };
  }

  const firstIndexMap = new Map<string, number>();
  const countMap = new Map<string, number>();

  for (let i = 0; i < items.length; i++) {
    const rawKey = getKey(items[i]);
    const letter = extractBucketLetter(rawKey, effectiveLocale);

    if (!firstIndexMap.has(letter)) {
      firstIndexMap.set(letter, i);
    }
    countMap.set(letter, (countMap.get(letter) ?? 0) + 1);
  }

  const sections: LetterSection[] = baseLetters.map((letter) => ({
    letter,
    startIndex: firstIndexMap.has(letter) ? firstIndexMap.get(letter)! : -1,
    count: countMap.get(letter) ?? 0,
  }));

  function getJumpIndex(letter: string): number {
    if (items.length === 0) return -1;
    const target = letter.toUpperCase();
    const idx = sections.findIndex((s) => s.letter === target);

    if (idx !== -1 && sections[idx].startIndex !== -1) {
      return sections[idx].startIndex;
    }

    const startIdx = idx === -1 ? 0 : idx;

    // Search forward for the next populated section
    for (let i = startIdx + 1; i < sections.length; i++) {
      if (sections[i].startIndex !== -1) {
        return sections[i].startIndex;
      }
    }

    // Search backward if no subsequent section has items
    for (let i = startIdx - 1; i >= 0; i--) {
      if (sections[i].startIndex !== -1) {
        return sections[i].startIndex;
      }
    }

    return -1;
  }

  return { sections, getJumpIndex };
}
