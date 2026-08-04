import "@testing-library/jest-dom";
import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, fireEvent } from "@testing-library/svelte";
import AlbumCard from "./AlbumCard.svelte";
import { collectionStore } from "../stores/collection.svelte";
import { prefs } from "../stores/prefs.svelte";
import { invoke } from "@tauri-apps/api/core";
import type { AlbumItem } from "../types";

vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn().mockResolvedValue(() => {}),
}));

describe("AlbumCard.svelte", () => {
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
  };

  beforeEach(() => {
    vi.clearAllMocks();
    prefs.ratingStyle = "stars";
  });

  it("renders album title, artist, year, and genre", () => {
    const { getByText } = render(AlbumCard, { props: { album: mockAlbum } });

    expect(getByText("Fake Nudes")).toBeInTheDocument();
    expect(getByText("Barenaked Ladies")).toBeInTheDocument();
    expect(getByText("2017")).toBeInTheDocument();
    expect(getByText("Alternative")).toBeInTheDocument();
  });

  it("rates the album via the rating widget without navigating to it", async () => {
    vi.mocked(invoke).mockResolvedValueOnce(4);
    const viewAlbumSpy = vi.spyOn(collectionStore, "viewAlbum");
    const { getByLabelText } = render(AlbumCard, { props: { album: mockAlbum } });

    await fireEvent.click(getByLabelText("Rate 4 of 5"));

    expect(invoke).toHaveBeenCalledWith("set_album_rating", { album: "Fake Nudes", rating: 4 });
    expect(viewAlbumSpy).not.toHaveBeenCalled();
  });

  it("navigates to album when album title is clicked", async () => {
    const viewAlbumSpy = vi.spyOn(collectionStore, "viewAlbum");
    const { getByText } = render(AlbumCard, { props: { album: mockAlbum } });

    const albumTitleBtn = getByText("Fake Nudes");
    await fireEvent.click(albumTitleBtn);

    expect(viewAlbumSpy).toHaveBeenCalledWith("Fake Nudes");
  });

  it("navigates to artist when artist name is clicked", async () => {
    const viewArtistSpy = vi.spyOn(collectionStore, "viewArtist");
    const { getByText } = render(AlbumCard, { props: { album: mockAlbum } });

    const artistBtn = getByText("Barenaked Ladies");
    await fireEvent.click(artistBtn);

    expect(viewArtistSpy).toHaveBeenCalledWith("Barenaked Ladies");
  });

  it("calls custom click handler when passed", async () => {
    const handleClick = vi.fn();
    const { getByText } = render(AlbumCard, {
      props: { album: mockAlbum, onclick: handleClick },
    });

    const card = getByText("Fake Nudes").closest("div.bg-brand-sidebar")!;
    await fireEvent.click(card);

    expect(handleClick).toHaveBeenCalled();
  });
});
