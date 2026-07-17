import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, fireEvent } from "@testing-library/svelte";
import SongRating from "./SongRating.svelte";
import { prefs } from "../stores/prefs.svelte";

describe("SongRating", () => {
  beforeEach(() => {
    prefs.ratingStyle = "heart";
  });

  it("renders a single heart in heart mode", () => {
    const { getAllByRole } = render(SongRating, { rating: -1, onRate: vi.fn() });
    expect(getAllByRole("button")).toHaveLength(1);
  });

  it("heart click favorites an unrated song at 5.0", async () => {
    const onRate = vi.fn();
    const { getByRole } = render(SongRating, { rating: -1, onRate });
    await fireEvent.click(getByRole("button"));
    expect(onRate).toHaveBeenCalledWith(5);
  });

  it("heart click on a favorited song clears the rating", async () => {
    const onRate = vi.fn();
    const { getByRole } = render(SongRating, { rating: 5, onRate });
    expect(getByRole("button").getAttribute("aria-pressed")).toBe("true");
    await fireEvent.click(getByRole("button"));
    expect(onRate).toHaveBeenCalledWith(-1);
  });

  it("renders five stars in stars mode", () => {
    prefs.ratingStyle = "stars";
    const { getAllByRole } = render(SongRating, { rating: 3, onRate: vi.fn() });
    expect(getAllByRole("button")).toHaveLength(5);
  });

  it("star clicks pass through the star value", async () => {
    prefs.ratingStyle = "stars";
    const onRate = vi.fn();
    const { getAllByRole } = render(SongRating, { rating: -1, onRate });
    await fireEvent.click(getAllByRole("button")[2]);
    expect(onRate).toHaveBeenCalledWith(3);
  });
});
