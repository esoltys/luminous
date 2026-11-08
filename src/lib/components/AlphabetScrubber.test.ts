import { render, fireEvent } from "@testing-library/svelte";
import { describe, it, expect, vi } from "vitest";
import AlphabetScrubber from "./AlphabetScrubber.svelte";

describe("AlphabetScrubber.svelte (#1973)", () => {
  const sampleItems = [
    "123 Band",
    "Alice in Chains",
    "Anthrax",
    "Beatles",
    "Coldplay",
    "David Bowie",
    "Eagles",
    "Fleetwood Mac",
    "Gorillaz",
    "Hozier",
    "Iron Maiden",
    "Judas Priest",
    "Kansas",
    "Led Zeppelin",
    "Metallica",
    "Nirvana",
    "Oasis",
    "Pink Floyd",
    "Queen",
    "Radiohead",
  ];

  it("does not render when items count is below minItems", () => {
    const { queryByTestId } = render(AlphabetScrubber, {
      items: ["Alice", "Bob"],
      getKey: (x: unknown) => String(x),
      minItems: 15,
      onjump: vi.fn(),
    });
    expect(queryByTestId("alphabet-scrubber")).toBeNull();
  });

  it("renders when items count meets or exceeds minItems", () => {
    const { getByTestId, getAllByRole } = render(AlphabetScrubber, {
      items: sampleItems,
      getKey: (x: unknown) => String(x),
      minItems: 15,
      onjump: vi.fn(),
    });

    const scrubber = getByTestId("alphabet-scrubber");
    expect(scrubber).toBeInTheDocument();
    expect(scrubber.getAttribute("role")).toBe("navigation");

    // 27 letter buttons: # and A-Z
    const buttons = getAllByRole("button");
    expect(buttons).toHaveLength(27);
  });

  it("invokes onjump with correct index and letter when a letter button is clicked", async () => {
    const handleJump = vi.fn();
    const { getByLabelText } = render(AlphabetScrubber, {
      items: sampleItems,
      getKey: (x: unknown) => String(x),
      minItems: 15,
      onjump: handleJump,
    });

    // Sample items: "Beatles" is at index 3
    const bButton = getByLabelText("Jump to B");
    await fireEvent.click(bButton);

    expect(handleJump).toHaveBeenCalledTimes(1);
    expect(handleJump).toHaveBeenCalledWith(3, "B");
  });

  it("falls forward to next available letter when clicking a letter without items", async () => {
    // List with no 'X' items; 'Queen' is index 18, 'Radiohead' is 19
    const handleJump = vi.fn();
    const { getByLabelText } = render(AlphabetScrubber, {
      items: sampleItems,
      getKey: (x: unknown) => String(x),
      minItems: 15,
      onjump: handleJump,
    });

    // 'X' has no items, falls backward to 'Radiohead' at index 19 (since no items after R)
    const xButton = getByLabelText("Jump to X");
    await fireEvent.click(xButton);

    expect(handleJump).toHaveBeenCalledTimes(1);
    expect(handleJump).toHaveBeenCalledWith(19, "X");
  });

  it("reverses letter order when ascending is false", () => {
    const reversedItems = [...sampleItems].reverse();
    const { getAllByRole } = render(AlphabetScrubber, {
      items: reversedItems,
      getKey: (x: unknown) => String(x),
      ascending: false,
      minItems: 15,
      onjump: vi.fn(),
    });

    const buttons = getAllByRole("button");
    expect(buttons[0].textContent?.trim()).toBe("Z");
    expect(buttons[buttons.length - 1].textContent?.trim()).toBe("#");
  });

  it("renders Russian Cyrillic alphabet when locale is 'ru'", () => {
    const { getAllByRole } = render(AlphabetScrubber, {
      items: sampleItems,
      getKey: (x: unknown) => String(x),
      minItems: 15,
      locale: "ru",
      onjump: vi.fn(),
    });

    const buttons = getAllByRole("button");
    const letters = buttons.map((b) => b.textContent?.trim());
    expect(letters[0]).toBe("#");
    expect(letters[1]).toBe("А");
    expect(letters[letters.length - 1]).toBe("Я");
    expect(letters).toContain("Б");
    expect(letters).toContain("Ж");
  });

  it("renders Spanish alphabet with Ñ when locale is 'es'", () => {
    const { getAllByRole } = render(AlphabetScrubber, {
      items: sampleItems,
      getKey: (x: unknown) => String(x),
      minItems: 15,
      locale: "es",
      onjump: vi.fn(),
    });

    const buttons = getAllByRole("button");
    const letters = buttons.map((b) => b.textContent?.trim());
    expect(letters).toContain("Ñ");
    const nPos = letters.indexOf("N");
    const enyePos = letters.indexOf("Ñ");
    expect(enyePos).toBe(nPos + 1);
  });
});
