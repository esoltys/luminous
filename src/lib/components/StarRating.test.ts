import "@testing-library/jest-dom";
import { describe, it, expect, vi } from "vitest";
import { render, fireEvent } from "@testing-library/svelte";
import StarRating from "./StarRating.svelte";

// In jsdom, elements have zero-width bounding rects, so clicks always resolve
// to the full-star value (the left-half/half-star branch requires real layout).

describe("StarRating", () => {
  it("renders five stars when read-only (no clear button without onRate)", () => {
    const { getAllByRole } = render(StarRating, { rating: 3 });
    expect(getAllByRole("button")).toHaveLength(5);
  });

  it("renders five stars plus a clear button when interactive", () => {
    const { getAllByRole } = render(StarRating, { rating: 3, onRate: vi.fn() });
    expect(getAllByRole("button")).toHaveLength(6);
  });

  it("calls onRate with the clicked star value", async () => {
    const onRate = vi.fn();
    const { getByLabelText } = render(StarRating, { rating: -1, onRate });
    await fireEvent.click(getByLabelText("Rate 4 of 5"));
    expect(onRate).toHaveBeenCalledWith(4);
  });

  it("clears the rating when the current value is clicked again", async () => {
    const onRate = vi.fn();
    const { getByLabelText } = render(StarRating, { rating: 2, onRate });
    await fireEvent.click(getByLabelText("Rate 2 of 5"));
    expect(onRate).toHaveBeenCalledWith(-1);
  });

  it("clears the rating via the clear button", async () => {
    const onRate = vi.fn();
    const { getByLabelText } = render(StarRating, { rating: 2, onRate });
    await fireEvent.click(getByLabelText("Clear rating"));
    expect(onRate).toHaveBeenCalledWith(-1);
  });

  it("disables the clear button when already unrated", () => {
    const { getByLabelText } = render(StarRating, { rating: -1, onRate: vi.fn() });
    expect(getByLabelText("Clear rating")).toBeDisabled();
  });

  it("is inert without an onRate handler", async () => {
    const { getAllByRole } = render(StarRating, { rating: 4 });
    for (const button of getAllByRole("button")) {
      expect(button.hasAttribute("disabled")).toBe(true);
    }
  });

  it("fills stars up to the rating", () => {
    const { container } = render(StarRating, { rating: 2.5 });
    const overlays = container.querySelectorAll("span.absolute");
    expect(overlays).toHaveLength(3);
    const widths = Array.from(overlays).map((el) => (el as HTMLElement).style.width);
    expect(widths).toEqual(["100%", "100%", "50%"]);
  });

  it("shows no fill when unrated", () => {
    const { container } = render(StarRating, { rating: -1 });
    expect(container.querySelectorAll("span.absolute")).toHaveLength(0);
  });

  describe("revealOnInteract", () => {
    // jsdom applies no Tailwind CSS, so assert on the classes that hide and reveal the control.
    const group = (container: HTMLElement) => container.querySelector('[role="group"]')!;

    it("hides an unrated control until the surrounding group is hovered or focused", () => {
      const { container } = render(StarRating, {
        rating: -1,
        onRate: vi.fn(),
        revealOnInteract: true,
      });
      expect(group(container)).toHaveClass(
        "opacity-0",
        "group-hover:opacity-40",
        "group-focus-within:opacity-40",
        "focus-within:opacity-40",
      );
    });

    it("keeps the stars in the tab order while hidden", () => {
      const { getAllByRole } = render(StarRating, {
        rating: -1,
        onRate: vi.fn(),
        revealOnInteract: true,
      });
      const stars = getAllByRole("button").filter((b) => !b.hasAttribute("disabled"));
      expect(stars).toHaveLength(5);
    });

    it("always shows a rated control", () => {
      const { container } = render(StarRating, {
        rating: 3,
        onRate: vi.fn(),
        revealOnInteract: true,
      });
      expect(group(container)).not.toHaveClass("opacity-0");
    });

    it("is unchanged without the prop", () => {
      const { container } = render(StarRating, { rating: -1, onRate: vi.fn() });
      expect(group(container)).not.toHaveClass("opacity-0");
      expect(group(container)).toHaveClass("opacity-40");
    });
  });
});
