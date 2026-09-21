import { describe, it, expect, vi } from "vitest";
import { render, screen, fireEvent } from "@testing-library/svelte";
import ArtistTagCards from "./ArtistTagCards.svelte";
import type { TagGroup } from "../types";

describe("ArtistTagCards", () => {
  const sampleHierarchy: TagGroup[] = [
    {
      name: "Award-Winning",
      color_index: 2,
      song_count: 5,
      children: [
        { name: "Juno Award", song_count: 3 },
        { name: "Grammy Award", song_count: 2 },
        { name: "Brit Award", song_count: 1 }
      ]
    },
    {
      name: "Canadian",
      color_index: 4,
      song_count: 10,
      children: []
    }
  ];

  it("renders artist tag groups and children with artist counts", () => {
    render(ArtistTagCards, {
      props: {
        hierarchy: sampleHierarchy,
        onOpenTag: vi.fn(),
        selectMode: false,
        selected: new Set<string>(),
        onToggleSelect: vi.fn()
      }
    });

    expect(screen.getByText("Award-Winning")).toBeTruthy();
    expect(screen.getByText("5 artists")).toBeTruthy();
    expect(screen.getByText("Juno Award")).toBeTruthy();
    expect(screen.getByText("Grammy Award")).toBeTruthy();
    expect(screen.getByText("Brit Award")).toBeTruthy();
    expect(screen.getByText("Canadian")).toBeTruthy();
    expect(screen.getByText("10 artists")).toBeTruthy();
  });

  it("renders empty state message for group without children", () => {
    render(ArtistTagCards, {
      props: {
        hierarchy: sampleHierarchy,
        onOpenTag: vi.fn(),
        selectMode: false,
        selected: new Set<string>(),
        onToggleSelect: vi.fn()
      }
    });

    expect(screen.getByText(/No sub-tags yet/i)).toBeTruthy();
  });

  it("invokes onOpenTag when card name is clicked", async () => {
    const onOpenTag = vi.fn();
    render(ArtistTagCards, {
      props: {
        hierarchy: sampleHierarchy,
        onOpenTag,
        selectMode: false,
        selected: new Set<string>(),
        onToggleSelect: vi.fn()
      }
    });

    const cardBtn = screen.getByText("Award-Winning");
    await fireEvent.click(cardBtn);
    expect(onOpenTag).toHaveBeenCalledWith("Award-Winning");
  });

  it("renders checkboxes in select mode and calls onToggleSelect", async () => {
    const onToggleSelect = vi.fn();
    render(ArtistTagCards, {
      props: {
        hierarchy: sampleHierarchy,
        onOpenTag: vi.fn(),
        selectMode: true,
        selected: new Set(["Juno Award"]),
        onToggleSelect
      }
    });

    const junoChip = screen.getByText("Juno Award").closest("span")!;
    await fireEvent.click(junoChip);
    expect(onToggleSelect).toHaveBeenCalledWith("Juno Award");
  });
});
