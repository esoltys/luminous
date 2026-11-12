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

    // 19 distinct letters derived from sampleItems: # and A through R
    const buttons = getAllByRole("button");
    expect(buttons).toHaveLength(19);
    expect(buttons[0].textContent?.trim()).toBe("#");
    expect(buttons[1].textContent?.trim()).toBe("A");
    expect(buttons[buttons.length - 1].textContent?.trim()).toBe("R");
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
    expect(buttons[0].textContent?.trim()).toBe("R");
    expect(buttons[buttons.length - 1].textContent?.trim()).toBe("#");
  });

  it("derives Cyrillic alphabet when collection has Cyrillic items", () => {
    const cyrillicItems = [
      "Аквариум",
      "Би-2",
      "Воскресение",
      "Гражданская Оборона",
      "ДДТ",
      "Земфира",
      "Кино",
      "Ленинград",
      "Мумий Тролль",
      "Наутилус Помпилиус",
      "Океан Ельзи",
      "Пикник",
      "Руки Вверх",
      "Сплин",
      "Чайф",
    ];
    const { getAllByRole } = render(AlphabetScrubber, {
      items: cyrillicItems,
      getKey: (x: unknown) => String(x),
      minItems: 15,
      locale: "ru",
      onjump: vi.fn(),
    });

    const buttons = getAllByRole("button");
    const letters = buttons.map((b) => b.textContent?.trim());
    expect(letters[0]).toBe("А");
    expect(letters).toContain("К");
    expect(letters).toContain("Ч");
  });

  it("derives Spanish alphabet with Ñ when collection has Spanish items", () => {
    const spanishItems = [
      "Alejandro",
      "Barón Rojo",
      "Camela",
      "Duncan Dhu",
      "Estopa",
      "Fito",
      "Heroes del Silencio",
      "Ivan Ferreiro",
      "Jarabe de Palo",
      "Kiko Veneno",
      "Los Secretos",
      "M Clan",
      "Nacho Vegas",
      "Ñu",
      "Zahara",
    ];
    const { getAllByRole } = render(AlphabetScrubber, {
      items: spanishItems,
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
