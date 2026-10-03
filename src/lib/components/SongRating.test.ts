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

  it("renders five stars plus a clear button in stars mode", () => {
    prefs.ratingStyle = "stars";
    const { getAllByRole } = render(SongRating, { rating: 3, onRate: vi.fn() });
    expect(getAllByRole("button")).toHaveLength(6);
  });

  it("star clicks pass through the star value", async () => {
    prefs.ratingStyle = "stars";
    const onRate = vi.fn();
    const { getByLabelText } = render(SongRating, { rating: -1, onRate });
    await fireEvent.click(getByLabelText("Rate 3 of 5"));
    expect(onRate).toHaveBeenCalledWith(3);
  });

  it("renders both heart and stars in both mode", () => {
    prefs.ratingStyle = "both";
    const { getAllByRole } = render(SongRating, { rating: 3, loved: 0, onRate: vi.fn() });
    // 1 heart button + 5 star buttons + 1 clear rating button = 7 buttons
    expect(getAllByRole("button")).toHaveLength(7);
  });

  it("calls onSetLoved when heart is clicked in both mode", async () => {
    prefs.ratingStyle = "both";
    const onRate = vi.fn();
    const onSetLoved = vi.fn();
    const { getAllByRole } = render(SongRating, { rating: 3, loved: 0, onRate, onSetLoved });
    const buttons = getAllByRole("button");
    // First button is the heart toggle
    await fireEvent.click(buttons[0]);
    expect(onSetLoved).toHaveBeenCalledWith(1);
    expect(onRate).not.toHaveBeenCalled();
  });

  it("always renders stars for albums even in heart mode", () => {
    prefs.ratingStyle = "heart";
    const { getAllByRole } = render(SongRating, { rating: 4, isAlbum: true, onRate: vi.fn() });
    expect(getAllByRole("button")).toHaveLength(6);
  });

  it("always renders stars for albums even in both mode", () => {
    prefs.ratingStyle = "both";
    const { getAllByRole } = render(SongRating, { rating: 4, isAlbum: true, onRate: vi.fn() });
    expect(getAllByRole("button")).toHaveLength(6);
  });

  it("handles tri-state loved in heart mode", async () => {
    prefs.ratingStyle = "heart";
    const onSetLoved = vi.fn();
    const { getByRole } = render(SongRating, { rating: 0, loved: 0, onSetLoved });
    const btn = getByRole("button");

    // Right-click toggles to -1 (dislike)
    await fireEvent.contextMenu(btn);
    expect(onSetLoved).toHaveBeenCalledWith(-1);
  });

  it("clears dislike to neutral when clicked", async () => {
    prefs.ratingStyle = "heart";
    const onSetLoved = vi.fn();
    const { getByRole } = render(SongRating, { rating: 0, loved: -1, onSetLoved });
    const btn = getByRole("button");

    // Left click on disliked heart clears to 0
    await fireEvent.click(btn);
    expect(onSetLoved).toHaveBeenCalledWith(0);
  });
});
