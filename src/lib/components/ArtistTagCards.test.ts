import "@testing-library/jest-dom";
import { describe, it, expect, vi, afterEach } from "vitest";
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
        onOpenTag: vi.fn()
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
        onOpenTag: vi.fn()
      }
    });

    expect(screen.getByText(/No sub-tags yet/i)).toBeTruthy();
  });

  it("invokes onOpenTag when card name is clicked", async () => {
    const onOpenTag = vi.fn();
    render(ArtistTagCards, {
      props: {
        hierarchy: sampleHierarchy,
        onOpenTag
      }
    });

    const cardBtn = screen.getByText("Award-Winning");
    await fireEvent.click(cardBtn);
    expect(onOpenTag).toHaveBeenCalledWith("Award-Winning");
  });

  describe("drag ghost", () => {
    // jsdom has no elementFromPoint; each test points it at the element under the cursor.
    const originalElementFromPoint = document.elementFromPoint;
    afterEach(() => {
      document.elementFromPoint = originalElementFromPoint;
    });

    function chip(name: string): HTMLElement {
      return document.querySelector<HTMLElement>(`[data-artist-chip-key="${name}"]`)!;
    }
    function card(name: string): HTMLElement {
      return document.querySelector<HTMLElement>(`[data-artist-card-name="${name}"]`)!;
    }

    it("does not appear for a plain click on a chip", async () => {
      const onOpenTag = vi.fn();
      render(ArtistTagCards, { props: { hierarchy: sampleHierarchy, onOpenTag } });

      await fireEvent.pointerDown(chip("Juno Award"), { button: 0, clientX: 10, clientY: 10 });
      expect(screen.queryByTestId("artist-tag-drag-ghost")).toBeNull();
      await fireEvent.pointerUp(window);

      expect(onOpenTag).toHaveBeenCalledWith("Juno Award");
      expect(screen.queryByTestId("artist-tag-drag-ghost")).toBeNull();
    });

    it("lights up over another card, where the drop would move the chip", async () => {
      render(ArtistTagCards, { props: { hierarchy: sampleHierarchy, onOpenTag: vi.fn() } });
      await fireEvent.pointerDown(chip("Juno Award"), { button: 0, clientX: 10, clientY: 10 });

      document.elementFromPoint = () => card("Canadian");
      await fireEvent.pointerMove(window, { clientX: 60, clientY: 60 });

      const ghost = screen.getByTestId("artist-tag-drag-ghost");
      expect(ghost).toHaveTextContent("Juno Award");
      expect(ghost).toHaveAttribute("data-over-target");

      // Leave every target before releasing, so the drag ends without reparenting anything.
      document.elementFromPoint = () => null;
      await fireEvent.pointerMove(window, { clientX: 500, clientY: 500 });
      expect(ghost).not.toHaveAttribute("data-over-target");
      await fireEvent.pointerUp(window);
    });

    it("stays unlit over the chip's own card, where dropping does nothing", async () => {
      render(ArtistTagCards, { props: { hierarchy: sampleHierarchy, onOpenTag: vi.fn() } });
      await fireEvent.pointerDown(chip("Juno Award"), { button: 0, clientX: 10, clientY: 10 });

      document.elementFromPoint = () => card("Award-Winning");
      await fireEvent.pointerMove(window, { clientX: 60, clientY: 60 });

      expect(screen.getByTestId("artist-tag-drag-ghost")).not.toHaveAttribute("data-over-target");
      await fireEvent.pointerUp(window);
    });
  });
});
