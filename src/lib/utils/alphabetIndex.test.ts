import { describe, it, expect } from "vitest";
import {
  extractBucketLetter,
  buildAlphabetIndex,
  BASE_LETTERS_ASC,
  BASE_LETTERS_DESC,
} from "./alphabetIndex";

describe("alphabetIndex.ts — letter extraction (#1973)", () => {
  it("extracts standard Latin uppercase letters", () => {
    expect(extractBucketLetter("Abba")).toBe("A");
    expect(extractBucketLetter("Beatles")).toBe("B");
    expect(extractBucketLetter("ZZ Top")).toBe("Z");
  });

  it("converts lowercase letters to uppercase buckets", () => {
    expect(extractBucketLetter("coldplay")).toBe("C");
    expect(extractBucketLetter("radiohead")).toBe("R");
  });

  it("normalizes diacritics / accented characters to Latin base letters", () => {
    expect(extractBucketLetter("Édith Piaf")).toBe("E");
    expect(extractBucketLetter("Åge Aleksandersen")).toBe("A");
    expect(extractBucketLetter("Björk")).toBe("B");
    expect(extractBucketLetter("Sigur Rós")).toBe("S");
  });

  it("maps numbers, symbols, and non-letter characters to '#'", () => {
    expect(extractBucketLetter("1999")).toBe("#");
    expect(extractBucketLetter("10,000 Maniacs")).toBe("#");
    expect(extractBucketLetter("+44")).toBe("#");
    expect(extractBucketLetter("---")).toBe("#");
  });

  it("strips leading punctuation and quotes before extracting the letter", () => {
    expect(extractBucketLetter('"Heroes"')).toBe("H");
    expect(extractBucketLetter("'Round Midnight")).toBe("R");
    expect(extractBucketLetter("(Don't Fear) The Reaper")).toBe("D");
    expect(extractBucketLetter("«Le Festin»")).toBe("L");
  });

  it("handles null, undefined, empty, and whitespace strings gracefully", () => {
    expect(extractBucketLetter(null)).toBe("#");
    expect(extractBucketLetter(undefined)).toBe("#");
    expect(extractBucketLetter("")).toBe("#");
    expect(extractBucketLetter("   ")).toBe("#");
  });
});

describe("alphabetIndex.ts — buildAlphabetIndex (#1973)", () => {
  it("returns empty sections for an empty list", () => {
    const { sections, getJumpIndex } = buildAlphabetIndex([], (x: string) => x);
    expect(sections).toHaveLength(0);
    expect(getJumpIndex("A")).toBe(-1);
  });

  it("derives sections dynamically from the items present in the collection", () => {
    const items = [
      "123 Band",
      "Alice in Chains",
      "Anthrax",
      "Beatles",
      "David Bowie",
      "Eagles",
    ];

    const { sections, getJumpIndex } = buildAlphabetIndex(items, (x) => x, true);

    const letters = sections.map((s) => s.letter);
    expect(letters).toEqual(["#", "A", "B", "D", "E"]);

    expect(sections[0]).toEqual({ letter: "#", startIndex: 0, count: 1 });
    expect(sections[1]).toEqual({ letter: "A", startIndex: 1, count: 2 });
    expect(sections[2]).toEqual({ letter: "B", startIndex: 3, count: 1 });
    expect(sections[3]).toEqual({ letter: "D", startIndex: 4, count: 1 });
    expect(sections[4]).toEqual({ letter: "E", startIndex: 5, count: 1 });

    // Exact matches
    expect(getJumpIndex("A")).toBe(1);
    expect(getJumpIndex("B")).toBe(3);
    expect(getJumpIndex("D")).toBe(4);
    expect(getJumpIndex("E")).toBe(5);
    expect(getJumpIndex("Z")).toBe(-1);
  });

  it("derives sections in reverse order when items are sorted descending", () => {
    const items = ["ZZ Top", "Yes", "Anthrax", "1999"];
    const { sections, getJumpIndex } = buildAlphabetIndex(items, (x) => x, false);

    const letters = sections.map((s) => s.letter);
    expect(letters).toEqual(["Z", "Y", "A", "#"]);

    expect(getJumpIndex("Z")).toBe(0);
    expect(getJumpIndex("Y")).toBe(1);
    expect(getJumpIndex("A")).toBe(2);
    expect(getJumpIndex("#")).toBe(3);
  });

  it("derives Spanish 'Ñ' section for Spanish titles", () => {
    const items = ["Alejandro", "Niño de Elche", "Ñu", "Zahara"];
    const { sections, getJumpIndex } = buildAlphabetIndex(items, (x) => x, true, "es");

    const letters = sections.map((s) => s.letter);
    expect(letters).toEqual(["A", "N", "Ñ", "Z"]);
    expect(getJumpIndex("Ñ")).toBe(2);
  });

  it("derives Cyrillic sections for Russian titles", () => {
    const items = [
      "123",
      "Аквариум",
      "Би-2",
      "Ёлка",
      "Земфира",
      "Кино",
      "Ялла"
    ];
    const { sections, getJumpIndex } = buildAlphabetIndex(items, (x) => x, true, "ru");

    const letters = sections.map((s) => s.letter);
    // 'Ёлка' maps to 'Е' bucket
    expect(letters).toEqual(["#", "А", "Б", "Е", "З", "Кино".charAt(0), "Я"]);

    expect(getJumpIndex("А")).toBe(1);
    expect(getJumpIndex("Б")).toBe(2);
    expect(getJumpIndex("Е")).toBe(3);
    expect(getJumpIndex("К")).toBe(5);
    expect(getJumpIndex("Я")).toBe(6);
  });

  it("derives Ukrainian Cyrillic sections with Ґ, Є, І, Ї", () => {
    const items = ["123", "Антитіла", "Ґринджоли", "Євген", "Іван", "Їжак", "Океан Ельзи"];
    const { sections, getJumpIndex } = buildAlphabetIndex(items, (x) => x, true, "uk");

    const letters = sections.map((s) => s.letter);
    expect(letters).toEqual(["#", "А", "Ґ", "Є", "І", "Ї", "О"]);

    expect(getJumpIndex("Ґ")).toBe(2);
    expect(getJumpIndex("Є")).toBe(3);
    expect(getJumpIndex("І")).toBe(4);
    expect(getJumpIndex("Ї")).toBe(5);
  });

  it("seamlessly handles multilingual libraries with both Cyrillic and Latin items", () => {
    const mixedItems = [
      "123 Band",
      "Аквариум",
      "Кино",
      "Beatles",
      "Queen",
    ];
    const { sections, getJumpIndex } = buildAlphabetIndex(mixedItems, (x) => x, true, "ru");

    const letters = sections.map((s) => s.letter);
    expect(letters).toEqual(["#", "А", "К", "B", "Q"]);

    expect(getJumpIndex("А")).toBe(1);
    expect(getJumpIndex("К")).toBe(2);
    expect(getJumpIndex("B")).toBe(3);
    expect(getJumpIndex("Q")).toBe(4);
  });
});
