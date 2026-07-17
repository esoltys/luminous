import { describe, it, expect, vi } from "vitest";
import { render, fireEvent } from "@testing-library/svelte";
import StarRating from "./StarRating.svelte";

// In jsdom, elements have zero-width bounding rects, so clicks always resolve
// to the full-star value (the left-half/half-star branch requires real layout).

describe("StarRating", () => {
  it("renders five stars", () => {
    const { getAllByRole } = render(StarRating, { rating: 3 });
    expect(getAllByRole("button")).toHaveLength(5);
  });

  it("calls onRate with the clicked star value", async () => {
    const onRate = vi.fn();
    const { getAllByRole } = render(StarRating, { rating: -1, onRate });
    await fireEvent.click(getAllByRole("button")[3]);
    expect(onRate).toHaveBeenCalledWith(4);
  });

  it("clears the rating when the current value is clicked again", async () => {
    const onRate = vi.fn();
    const { getAllByRole } = render(StarRating, { rating: 2, onRate });
    await fireEvent.click(getAllByRole("button")[1]);
    expect(onRate).toHaveBeenCalledWith(-1);
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
    const widths = Array.from(overlays).map(
      (el) => (el as HTMLElement).style.width
    );
    expect(widths).toEqual(["100%", "100%", "50%"]);
  });

  it("shows no fill when unrated", () => {
    const { container } = render(StarRating, { rating: -1 });
    expect(container.querySelectorAll("span.absolute")).toHaveLength(0);
  });
});
