import "@testing-library/jest-dom";
import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, fireEvent } from "@testing-library/svelte";
import ArtistCard from "./ArtistCard.svelte";
import type { ArtistItem, AlbumItem, Song } from "../types";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn().mockResolvedValue([]),
}));

vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn().mockResolvedValue(() => {}),
}));

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
  });

  it("renders artist name, genre, and song count", () => {
    const { getByText } = render(ArtistCard, {
      props: { artist: mockArtist, artistAlbums: [] },
    });

    expect(getByText("Dave Hawkins")).toBeInTheDocument();
    expect(getByText("Rock")).toBeInTheDocument();
    expect(getByText("6 songs")).toBeInTheDocument();
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
});
