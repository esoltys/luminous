import { describe, it, expect } from "vitest";
import { getArtistAlbums, getArtistSongs } from "./artist";
import type { AlbumItem, Song } from "../types";

describe("artist utils", () => {
  // Regression coverage for #295: tag casing can drift across files/albums for
  // the same real-world artist (e.g. "The War On Drugs" vs "The War on Drugs"),
  // and the artist detail page must pull in every casing variant.
  describe("getArtistSongs", () => {
    it("matches artist/album_artist case-insensitively", () => {
      const songs = [
        { artist: "The War on Drugs" } as Song,
        { album_artist: "The War On Drugs" } as Song,
        { artist: "Someone Else" } as Song,
      ];
      expect(getArtistSongs(songs, "The War On Drugs")).toHaveLength(2);
    });

    it("returns an empty array when name is null", () => {
      expect(getArtistSongs([{ artist: "Anyone" } as Song], null)).toEqual([]);
    });
  });

  describe("getArtistAlbums", () => {
    it("matches artist case-insensitively and sorts newest first", () => {
      const albums = [
        { artist: "the war on drugs", year: 2014 } as AlbumItem,
        { artist: "The War On Drugs", year: 2017 } as AlbumItem,
        { artist: "Other Artist", year: 2020 } as AlbumItem,
      ];
      const result = getArtistAlbums(albums, "The War on Drugs");
      expect(result.map((a) => a.year)).toEqual([2017, 2014]);
    });
  });
});
