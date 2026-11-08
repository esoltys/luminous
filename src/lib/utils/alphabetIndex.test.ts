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
  it("returns all base letters with startIndex -1 for an empty list", () => {
    const { sections, getJumpIndex } = buildAlphabetIndex([], (x: string) => x);
    expect(sections).toHaveLength(BASE_LETTERS_ASC.length);
    expect(sections[0].letter).toBe("#");
    expect(sections[1].letter).toBe("A");
    expect(sections.every((s) => s.startIndex === -1 && s.count === 0)).toBe(true);
    expect(getJumpIndex("A")).toBe(-1);
  });

  it("computes accurate start indices and counts for populated sections in ascending order", () => {
    const items = [
      "123 Band",
      "Alice in Chains",
      "Anthrax",
      "Beatles",
      "David Bowie",
      "Eagles",
    ];

    const { sections, getJumpIndex } = buildAlphabetIndex(items, (x) => x, true);

    const hashSec = sections.find((s) => s.letter === "#");
    const aSec = sections.find((s) => s.letter === "A");
    const bSec = sections.find((s) => s.letter === "B");
    const cSec = sections.find((s) => s.letter === "C");
    const dSec = sections.find((s) => s.letter === "D");
    const eSec = sections.find((s) => s.letter === "E");

    expect(hashSec?.startIndex).toBe(0);
    expect(hashSec?.count).toBe(1);

    expect(aSec?.startIndex).toBe(1);
    expect(aSec?.count).toBe(2);

    expect(bSec?.startIndex).toBe(3);
    expect(bSec?.count).toBe(1);

    expect(cSec?.startIndex).toBe(-1);
    expect(cSec?.count).toBe(0);

    expect(dSec?.startIndex).toBe(4);
    expect(dSec?.count).toBe(1);

    expect(eSec?.startIndex).toBe(5);
    expect(eSec?.count).toBe(1);

    // Exact matches
    expect(getJumpIndex("A")).toBe(1);
    expect(getJumpIndex("B")).toBe(3);
    expect(getJumpIndex("D")).toBe(4);

    // Unpopulated letter 'C' falls forward to 'D' (index 4)
    expect(getJumpIndex("C")).toBe(4);

    // Letters after 'E' fall backward to 'E' (index 5)
    expect(getJumpIndex("Z")).toBe(5);
  });

  it("inverts sections order when ascending is false", () => {
    const items = ["ZZ Top", "Yes", "Anthrax", "1999"];
    const { sections, getJumpIndex } = buildAlphabetIndex(items, (x) => x, false);

    expect(sections[0].letter).toBe("Z");
    expect(sections[sections.length - 1].letter).toBe("#");
    expect(sections).toHaveLength(BASE_LETTERS_DESC.length);

    expect(getJumpIndex("Z")).toBe(0);
    expect(getJumpIndex("Y")).toBe(1);
    expect(getJumpIndex("A")).toBe(2);
    expect(getJumpIndex("#")).toBe(3);
  });
});
