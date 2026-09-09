import "@testing-library/jest-dom";
import { describe, it, expect, vi } from "vitest";
import { render, fireEvent } from "@testing-library/svelte";
import GenreChips from "./GenreChips.svelte";
import { navigationStore } from "../stores/navigation.svelte";

describe("GenreChips.svelte", () => {
  it("renders nothing when genre is null or empty", () => {
    const { container: container1 } = render(GenreChips, { props: { genre: null } });
    expect(container1.textContent?.trim()).toBe("");

    const { container: container2 } = render(GenreChips, { props: { genre: "" } });
    expect(container2.textContent?.trim()).toBe("");
  });

  describe("compact variant (default)", () => {
    it("renders single genre without overflow count", () => {
      const { getByText, queryByText } = render(GenreChips, {
        props: { genre: "Metal" },
      });

      expect(getByText("Metal")).toBeInTheDocument();
      expect(queryByText("+")).toBeNull();
    });

    it("renders primary genre with +N overflow count when multiple genres exist", () => {
      const { getByText } = render(GenreChips, {
        props: { genre: "Metal; Symphonic Metal; Gothic Metal" },
      });

      expect(getByText("Metal")).toBeInTheDocument();
      expect(getByText("+2")).toBeInTheDocument();
    });

    it("navigates to genre tag on click", async () => {
      const viewSpy = vi.spyOn(navigationStore, "viewGenreTag");
      const { getByRole } = render(GenreChips, {
        props: { genre: "Rock; Pop" },
      });

      const button = getByRole("button");
      await fireEvent.click(button);

      expect(viewSpy).toHaveBeenCalledWith("Rock");
      viewSpy.mockRestore();
    });
  });

  describe("full variant", () => {
    it("renders all genre chips when no limit is specified", () => {
      const { getByText, queryByText } = render(GenreChips, {
        props: {
          genre: "Metal; Symphonic Metal; Gothic Metal; Power Metal; Heavy Metal",
          variant: "full",
        },
      });

      expect(getByText("Metal")).toBeInTheDocument();
      expect(getByText("Symphonic Metal")).toBeInTheDocument();
      expect(getByText("Gothic Metal")).toBeInTheDocument();
      expect(getByText("Power Metal")).toBeInTheDocument();
      expect(getByText("Heavy Metal")).toBeInTheDocument();
      expect(queryByText(/^\+/)).toBeNull();
    });

    it("renders all chips without overflow badge when count <= limit", () => {
      const { getByText, queryByText } = render(GenreChips, {
        props: {
          genre: "Rock; Pop; Jazz",
          variant: "full",
          limit: 4,
        },
      });

      expect(getByText("Rock")).toBeInTheDocument();
      expect(getByText("Pop")).toBeInTheDocument();
      expect(getByText("Jazz")).toBeInTheDocument();
      expect(queryByText(/^\+/)).toBeNull();
    });

    it("limits displayed chips and renders +N badge when count > limit", () => {
      const { getByText, queryByText } = render(GenreChips, {
        props: {
          genre: "Metal; Symphonic Metal; Gothic Metal; Power Metal; Heavy Metal; Pop Rock",
          variant: "full",
          limit: 4,
        },
      });

      expect(getByText("Metal")).toBeInTheDocument();
      expect(getByText("Symphonic Metal")).toBeInTheDocument();
      expect(getByText("Gothic Metal")).toBeInTheDocument();
      expect(getByText("Power Metal")).toBeInTheDocument();

      // Exceeded chips should NOT be individual buttons
      expect(queryByText("Heavy Metal")).toBeNull();
      expect(queryByText("Pop Rock")).toBeNull();

      // Overflow badge with +2 and title showing hidden genres
      const overflowBadge = getByText("+2");
      expect(overflowBadge).toBeInTheDocument();
      expect(overflowBadge.getAttribute("title")).toBe("Heavy Metal, Pop Rock");
    });

    it("navigates to specific genre tag when an individual chip is clicked", async () => {
      const viewSpy = vi.spyOn(navigationStore, "viewGenreTag");
      const { getByTitle } = render(GenreChips, {
        props: {
          genre: "Metal; Symphonic Metal",
          variant: "full",
        },
      });

      const symphonicChip = getByTitle("Browse Symphonic Metal");
      await fireEvent.click(symphonicChip);

      expect(viewSpy).toHaveBeenCalledWith("Symphonic Metal");
      viewSpy.mockRestore();
    });
  });
});
