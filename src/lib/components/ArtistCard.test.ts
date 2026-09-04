import "@testing-library/jest-dom";
import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, fireEvent } from "@testing-library/svelte";
import { invoke } from "@tauri-apps/api/core";
import ArtistCard from "./ArtistCard.svelte";
import type { ArtistItem, AlbumItem, Song } from "../types";

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

describe("ArtistCard.svelte", () => {
  const mockArtist: ArtistItem = {
    name: "Dave Hawkins",
    album_count: 0,
    song_count: 6,
    genre: "Rock",
  };

  const mockAlbum: AlbumItem = {
    album: "Dave's Hits",
    artist: "Dave Hawkins",
    year: 2020,
    track_count: 10,
    disc_count: 1,
    art_embedded: true,
    art_automatic: "covers/dave.jpg",
    art_manual: null,
    rating: -1,
    total_duration_nanosec: 0,
  };

  const mockSongWithArt: Song = {
    id: 101,
    source: "local_file",
    filetype: "MP3",
    title: "Single 1",
    artist: "Dave Hawkins",
    art_embedded: true,
    art_automatic: "covers/single1.jpg",
    art_manual: undefined,
    art_unset: false,
    compilation: false,
    beginning_nanosec: 0,
    end_nanosec: 0,
    rating: 0,
    playcount: 0,
    skipcount: 0,
    unavailable: false,
  };

  beforeEach(() => {
    vi.clearAllMocks();
    collectionStore.extendedArtworkByArtist = {};
  });

  it("renders artist name, genre chip, and song count", () => {
    const { getByText, getByRole } = render(ArtistCard, {
      props: { artist: mockArtist, artistAlbums: [] },
    });

    expect(getByText("Dave Hawkins")).toBeInTheDocument();
    const genreChip = getByRole("button", { name: "Rock" });
    expect(genreChip).toBeInTheDocument();
    expect(getByText("Rock")).toBeInTheDocument();
    expect(getByText("6 songs")).toBeInTheDocument();
  });

  it("falls back to unknown genre text when artist has no genre", () => {
    const { getByText } = render(ArtistCard, {
      props: { artist: { ...mockArtist, genre: undefined }, artistAlbums: [] },
    });

    expect(getByText("Unknown genre")).toBeInTheDocument();
  });

  it("renders cover artwork from artistAlbums when albums exist", () => {
    const { container } = render(ArtistCard, {
      props: { artist: mockArtist, artistAlbums: [mockAlbum] },
    });

    const coverArt = container.querySelector("img");
    expect(coverArt).toBeInTheDocument();
  });

  it("falls back to artistSongs artwork when artist has no album releases", () => {
    const { container } = render(ArtistCard, {
      props: {
        artist: mockArtist,
        artistAlbums: [],
        artistSongs: [mockSongWithArt],
      },
    });

    const coverArt = container.querySelector("img");
    expect(coverArt).toBeInTheDocument();
  });

  it("renders initial letter avatar fallback when no artwork exists anywhere", () => {
    const { getByText, container } = render(ArtistCard, {
      props: { artist: mockArtist, artistAlbums: [], artistSongs: [] },
    });

    expect(getByText("D")).toBeInTheDocument();
    expect(container.querySelector("img")).not.toBeInTheDocument();
  });

  it("triggers custom onclick handler when clicked", async () => {
    const handleClick = vi.fn();
    const { getByText } = render(ArtistCard, {
      props: { artist: mockArtist, artistAlbums: [], onclick: handleClick },
    });

    await fireEvent.click(getByText("Dave Hawkins"));
    expect(handleClick).toHaveBeenCalled();
  });

  it("renders a discovered artist portrait instead of the album-art composite (#98/#761)", async () => {
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

    const { findByAltText } = render(ArtistCard, {
      props: { artist: mockArtist, artistAlbums: [mockAlbum] },
    });

    const portrait = await findByAltText("Dave Hawkins");
    expect(portrait.tagName).toBe("IMG");
    expect(portrait.getAttribute("src")).toBe("luminous-art://local/C:/Music/Dave Hawkins/artist.jpg");
  });
});
