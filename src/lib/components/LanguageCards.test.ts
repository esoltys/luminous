import "@testing-library/jest-dom";
import { describe, it, expect, vi } from "vitest";
import { render, fireEvent } from "@testing-library/svelte";
import LanguageCards from "./LanguageCards.svelte";
import type { LanguageSummary } from "../types";

describe("LanguageCards.svelte", () => {
  const mockLanguages: LanguageSummary[] = [
    { code: "da", song_count: 11, album_count: 3 },
    { code: "ja", song_count: 42, album_count: 5 },
  ];

  it("renders language cards with localized names, codes, song count, and album count", () => {
    const onOpenLanguage = vi.fn();
    const { getByText, queryByText } = render(LanguageCards, {
      props: {
        languages: mockLanguages,
        onOpenLanguage,
      },
    });

    expect(getByText("Danish")).toBeInTheDocument();
    expect(getByText("da")).toBeInTheDocument();
    expect(getByText(/11 songs/i)).toBeInTheDocument();
    expect(getByText(/3 albums/i)).toBeInTheDocument();
    // Ensure raw unlocalized i18n keys are never shown
    expect(queryByText("collection.albumCount")).toBeNull();
    expect(queryByText("collection.albumsCount")).toBeNull();
  });

  it("calls onOpenLanguage when clicked", async () => {
    const onOpenLanguage = vi.fn();
    const { getByText } = render(LanguageCards, {
      props: {
        languages: mockLanguages,
        onOpenLanguage,
      },
    });

    await fireEvent.click(getByText("Danish"));
    expect(onOpenLanguage).toHaveBeenCalledWith("da");
  });
});
