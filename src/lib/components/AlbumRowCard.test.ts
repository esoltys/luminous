import "@testing-library/jest-dom";
import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, fireEvent } from "@testing-library/svelte";
import AlbumRowCard from "./AlbumRowCard.svelte";
import { collectionStore } from "../stores/collection.svelte";
import { prefs } from "../stores/prefs.svelte";
import { invoke } from "@tauri-apps/api/core";
import type { AlbumItem } from "../types";

describe("AlbumRowCard.svelte", () => {
  const mockAlbum: AlbumItem = {
    album: "Fake Nudes",
    artist: "Barenaked Ladies",
    year: 2017,
    track_count: 14,
    disc_count: 1,
    art_embedded: false,
    art_automatic: null,
    art_manual: null,
    genre: "Alternative",
    rating: -1,
    total_duration_nanosec: 0,
  };

  beforeEach(() => {
    vi.clearAllMocks();
    prefs.ratingStyle = "stars";
  });

  it("renders album title/year on top and artist/rating on bottom", () => {
    const { getByText } = render(AlbumRowCard, { props: { album: mockAlbum } });

    expect(getByText("Fake Nudes")).toBeInTheDocument();
    expect(getByText("2017")).toBeInTheDocument();
    expect(getByText("Barenaked Ladies")).toBeInTheDocument();
  });

  it("rates the album via the rating widget without navigating to it", async () => {
    vi.mocked(invoke).mockResolvedValueOnce(4);
    const viewAlbumSpy = vi.spyOn(collectionStore, "viewAlbum");
    const { getByLabelText } = render(AlbumRowCard, { props: { album: mockAlbum } });

    await fireEvent.click(getByLabelText("Rate 4 of 5"));

    expect(invoke).toHaveBeenCalledWith("set_album_rating", { album: "Fake Nudes", rating: 4 });
    expect(viewAlbumSpy).not.toHaveBeenCalled();
  });

  it("shows Various Artists when the album has no artist", () => {
    const { getByText } = render(AlbumRowCard, {
      props: { album: { ...mockAlbum, artist: null } },
    });
    expect(getByText("Various Artists")).toBeInTheDocument();
  });

  it("navigates to the album on click by default", async () => {
    const viewAlbumSpy = vi.spyOn(collectionStore, "viewAlbum");
    const { getByText } = render(AlbumRowCard, { props: { album: mockAlbum } });

    await fireEvent.click(getByText("Fake Nudes"));

    expect(viewAlbumSpy).toHaveBeenCalledWith("Fake Nudes");
  });

  it("calls a custom click handler when passed", async () => {
    const handleClick = vi.fn();
    const { getByText } = render(AlbumRowCard, {
      props: { album: mockAlbum, onclick: handleClick },
    });

    await fireEvent.click(getByText("Fake Nudes"));

    expect(handleClick).toHaveBeenCalled();
  });

  it("forwards the context menu event", async () => {
    const handleContextMenu = vi.fn();
    const { getByText } = render(AlbumRowCard, {
      props: { album: mockAlbum, oncontextmenu: handleContextMenu },
    });

    await fireEvent.contextMenu(getByText("Fake Nudes"));

    expect(handleContextMenu).toHaveBeenCalled();
  });

  it("renders favourite corner flag when album rating is 5", () => {
    const favoritedAlbum = { ...mockAlbum, rating: 5 };
    const { getByTestId } = render(AlbumRowCard, { props: { album: favoritedAlbum } });
    expect(getByTestId("favourite-corner-flag")).toBeInTheDocument();
  });

  it("does not render favourite corner flag when album rating is not 5", () => {
    const unratedAlbum = { ...mockAlbum, rating: 2 };
    const { queryByTestId } = render(AlbumRowCard, { props: { album: unratedAlbum } });
    expect(queryByTestId("favourite-corner-flag")).toBeNull();
  });
});
