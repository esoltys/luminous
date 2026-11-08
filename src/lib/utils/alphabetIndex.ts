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

export const BASE_LETTERS_ASC = [
  "#", "A", "B", "C", "D", "E", "F", "G", "H", "I", "J", "K", "L", "M",
  "N", "O", "P", "Q", "R", "S", "T", "U", "V", "W", "X", "Y", "Z"
] as const;

export const BASE_LETTERS_DESC = [
  "Z", "Y", "X", "W", "V", "U", "T", "S", "R", "Q", "P", "O", "N", "M",
  "L", "K", "J", "I", "H", "G", "F", "E", "D", "C", "B", "A", "#"
] as const;

/**
 * Normalizes an item key string to determine its alphabet bucket ('#' or 'A'–'Z').
 *
 * Strips leading quotes/brackets and normalizes Unicode diacritics so accented
 * characters (e.g. 'É', 'ö', 'Å') map to their base Latin letters.
 */
export function extractBucketLetter(str: string | null | undefined): string {
  if (!str) return "#";
  const trimmed = str.trim();
  if (!trimmed) return "#";

  // Strip leading punctuation/quotes/brackets
  const cleaned = trimmed.replace(/^["'([{‘“«\s]+/, "");
  if (!cleaned) return "#";

  // Decompose diacritics and strip combining marks
  const char = cleaned.charAt(0).normalize("NFD").replace(/[\u0300-\u036f]/g, "").toUpperCase();
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
 * @param ascending True if items are sorted A-Z / #-Z; false if sorted Z-A / Z-#.
 */
export function buildAlphabetIndex<T>(
  items: T[],
  getKey: (item: T) => string | null | undefined,
  ascending = true
): AlphabetIndexResult {
  if (items.length === 0) {
    const baseLetters = ascending ? BASE_LETTERS_ASC : BASE_LETTERS_DESC;
    return {
      sections: baseLetters.map((letter) => ({ letter, startIndex: -1, count: 0 })),
      getJumpIndex: () => -1,
    };
  }

  const firstIndexMap = new Map<string, number>();
  const countMap = new Map<string, number>();

  for (let i = 0; i < items.length; i++) {
    const rawKey = getKey(items[i]);
    const letter = extractBucketLetter(rawKey);

    if (!firstIndexMap.has(letter)) {
      firstIndexMap.set(letter, i);
    }
    countMap.set(letter, (countMap.get(letter) ?? 0) + 1);
  }

  const baseLetters = ascending ? BASE_LETTERS_ASC : BASE_LETTERS_DESC;
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
