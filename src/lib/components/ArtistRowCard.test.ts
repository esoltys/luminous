import "@testing-library/jest-dom";
import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, fireEvent } from "@testing-library/svelte";
import { invoke } from "@tauri-apps/api/core";
import ArtistRowCard from "./ArtistRowCard.svelte";
import type { ArtistItem, AlbumItem } from "../types";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn().mockResolvedValue([]),
}));

vi.mock("@tauri-apps/api/window", () => ({
  getCurrentWindow: vi.fn(() => ({
    onResized: vi.fn(() => Promise.resolve(() => {})),
    onMoved: vi.fn(() => Promise.resolve(() => {})),
  })),
}));

import { collectionStore } from "../stores/collection.svelte";

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
    collectionStore.extendedArtworkByArtist = {};
  });

  it("renders artist name, genre chip, and song count", () => {
    const { getByText, getByRole } = render(ArtistRowCard, {
      props: { artist: mockArtist, artistAlbums: [mockAlbum] },
    });

    expect(getByText("Dave Hawkins")).toBeInTheDocument();
    expect(getByRole("button", { name: "Rock" })).toBeInTheDocument();
    expect(getByText("Rock")).toBeInTheDocument();
    expect(getByText("6 songs")).toBeInTheDocument();
  });

  it("falls back to unknown genre text when artist has no genre", () => {
    const { getByText } = render(ArtistRowCard, {
      props: { artist: { ...mockArtist, genre: undefined }, artistAlbums: [mockAlbum] },
    });

    expect(getByText("Unknown genre")).toBeInTheDocument();
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

  it("renders a discovered artist portrait instead of the album-art front cover (#98/#761)", async () => {
    vi.mocked(invoke).mockImplementation(async (cmd: string) => {
      if (cmd === "get_extended_artwork_for_artist") {
        return {
          count: 1,
          primary_uri: null,
          artist_portrait_uri: "luminous-art://local/C:/Music/Dave Hawkins/artist.jpg",
          band_logo_uri: null,
          fanart_uri: null,
          items: [],
        };
      }
      return [];
    });

    const { findByAltText } = render(ArtistRowCard, {
      props: { artist: mockArtist, artistAlbums: [mockAlbum] },
    });

    const portrait = await findByAltText("Dave Hawkins");
    expect(portrait.tagName).toBe("IMG");
    expect(portrait.getAttribute("src")).toBe("luminous-art://local/C:/Music/Dave Hawkins/artist.jpg");
  });
});
