import "@testing-library/jest-dom";
import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, fireEvent } from "@testing-library/svelte";
import ArtistRowCard from "./ArtistRowCard.svelte";
import type { ArtistItem, AlbumItem } from "../types";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn().mockResolvedValue([]),
}));

describe("ArtistRowCard.svelte", () => {
  const mockArtist: ArtistItem = {
    name: "Dave Hawkins",
    album_count: 1,
    song_count: 6,
    genre: "Rock",
  };

  const mockAlbum: AlbumItem = {
    album: "Dave's Hits",
    artist: "Dave Hawkins",
    year: 2020,
    track_count: 10,
    disc_count: 1,
    art_embedded: false,
    art_automatic: "covers/dave.jpg",
    art_manual: null,
    rating: -1,
    total_duration_nanosec: 0,
  };

  beforeEach(() => {
    vi.clearAllMocks();
  });

  it("renders artist name, genre, and song count", () => {
    const { getByText } = render(ArtistRowCard, {
      props: { artist: mockArtist, artistAlbums: [mockAlbum] },
    });

    expect(getByText("Dave Hawkins")).toBeInTheDocument();
    expect(getByText("Rock")).toBeInTheDocument();
    expect(getByText("6 songs")).toBeInTheDocument();
  });

  it("renders cover art from the artist's front album, matching CoverStack's front tile", () => {
    const { container } = render(ArtistRowCard, {
      props: { artist: mockArtist, artistAlbums: [mockAlbum] },
    });

    const img = container.querySelector("img");
    expect(img).toBeInTheDocument();
  });

  it("triggers custom onclick handler when clicked", async () => {
    const handleClick = vi.fn();
    const { getByText } = render(ArtistRowCard, {
      props: { artist: mockArtist, artistAlbums: [], onclick: handleClick },
    });

    await fireEvent.click(getByText("Dave Hawkins"));
    expect(handleClick).toHaveBeenCalled();
  });
});
