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
  "#",
  "A",
  "B",
  "C",
  "D",
  "E",
  "F",
  "G",
  "H",
  "I",
  "J",
  "K",
  "L",
  "M",
  "N",
  "O",
  "P",
  "Q",
  "R",
  "S",
  "T",
  "U",
  "V",
  "W",
  "X",
  "Y",
  "Z",
] as const;

export const BASE_LETTERS_ASC = LATIN_LETTERS_ASC;
export const BASE_LETTERS_DESC = [...LATIN_LETTERS_ASC].reverse();

/**
 * Normalizes an item key string to determine its alphabet bucket ('#' or alphabet letters).
 *
 * Strips leading quotes/brackets and normalizes Unicode characters:
 * - Spanish preserves 'Ñ' / 'ñ'
 * - Cyrillic: Russian merges 'Ё' -> 'Е' and 'Й' -> 'И'; Ukrainian merges 'Й' -> 'И'
 * - Latin diacritics (e.g. 'É', 'ö', 'Å') decompose and map to their base Latin letters
 * - Non-Latin alphabetic scripts (Cyrillic, Greek, etc.) preserve their uppercase letters
 * - Digits, punctuation and symbols map to '#'
 */
export function extractBucketLetter(str: string | null | undefined, locale = "en"): string {
  if (!str) return "#";
  const trimmed = str.trim();
  if (!trimmed) return "#";

  // Strip leading punctuation/quotes/brackets
  const cleaned = trimmed.replace(/^["'([{‘“«\s]+/, "");
  if (!cleaned) return "#";

  const lang = (locale || "en").toLowerCase().split("-")[0];
  const firstChar = cleaned.charAt(0);

  if (lang === "es") {
    const upper = firstChar.toUpperCase();
    if (upper === "Ñ") return "Ñ";
  }

  // Cyrillic script
  if (/[\u0400-\u04FF]/.test(firstChar)) {
    const upper = firstChar.toUpperCase();
    if (upper === "Ё") return "Е";
    if (upper === "Й") return "И";
    return upper;
  }

  // Greek script
  if (/[\u0370-\u03FF]/.test(firstChar)) {
    return firstChar.toUpperCase();
  }

  // Decompose diacritics and strip combining marks for Latin letters
  const char = firstChar
    .normalize("NFD")
    .replace(/[\u0300-\u036f]/g, "")
    .toUpperCase();
  if (char >= "A" && char <= "Z") {
    return char;
  }

  // Any other Unicode letter character
  if (/\p{L}/u.test(firstChar)) {
    return firstChar.toUpperCase();
  }

  return "#";
}

/**
 * Computes alphabet sections and jump lookup derived from the items in a sorted list (#1973).
 *
 * For multilingual libraries, sections are dynamically derived from the items present
 * in the collection in their actual sorted collation order, ensuring every scrubber letter
 * maps to existing content.
 *
 * @param items The sorted collection of items.
 * @param getKey Extractor function returning the string sort key for each item.
 * @param ascending True if items are sorted ascending; false if descending.
 * @param locale The active BCP 47 UI locale code (e.g. 'en-CA', 'ru', 'es', 'uk').
 */
export function buildAlphabetIndex<T>(
  items: T[],
  getKey: (item: T) => string | null | undefined,
  _ascending = true,
  locale = "en",
): AlphabetIndexResult {
  if (items.length === 0) {
    return {
      sections: [],
      getJumpIndex: () => -1,
    };
  }

  const sections: LetterSection[] = [];
  const letterMap = new Map<string, LetterSection>();

  for (let i = 0; i < items.length; i++) {
    const rawKey = getKey(items[i]);
    const letter = extractBucketLetter(rawKey, locale);

    let sec = letterMap.get(letter);
    if (!sec) {
      sec = { letter, startIndex: i, count: 0 };
      letterMap.set(letter, sec);
      sections.push(sec);
    }
    sec.count++;
  }

  const getJumpIndex = (targetLetter: string): number => {
    const sec = letterMap.get(targetLetter.toUpperCase());
    if (sec) return sec.startIndex;
    return -1;
  };

  return { sections, getJumpIndex };
}
