import { describe, it, expect, beforeEach } from "vitest";
import { matchesSongSearch } from "./songSearch";
import { collectionStore } from "../stores/collection.svelte";
import type { Song } from "../types";

describe("matchesSongSearch", () => {
  const sampleSong: Song = {
    id: 1,
    title: "Creep",
    artist: "Radiohead",
    album: "Pablo Honey",
    genre: "Alternative Rock",
    year: 1993,
    path: "/music/radiohead/creep.mp3",
  } as Song;

  beforeEach(() => {
    collectionStore.artistProfiles = {};
  });

  it("returns true for empty or whitespace-only queries", () => {
    expect(matchesSongSearch(sampleSong, "")).toBe(true);
    expect(matchesSongSearch(sampleSong, "   ")).toBe(true);
  });

  it("returns false if song is undefined", () => {
    expect(matchesSongSearch(undefined, "creep")).toBe(false);
  });

  it("matches title, artist, album, genre, year, path", () => {
    expect(matchesSongSearch(sampleSong, "Creep")).toBe(true);
    expect(matchesSongSearch(sampleSong, "radiohead")).toBe(true);
    expect(matchesSongSearch(sampleSong, "pablo")).toBe(true);
    expect(matchesSongSearch(sampleSong, "alternative")).toBe(true);
    expect(matchesSongSearch(sampleSong, "1993")).toBe(true);
    expect(matchesSongSearch(sampleSong, "creep.mp3")).toBe(true);
    expect(matchesSongSearch(sampleSong, "nonexistent")).toBe(false);
  });

  it("matches multi-term queries when all terms match any field", () => {
    expect(matchesSongSearch(sampleSong, "radiohead creep")).toBe(true);
    expect(matchesSongSearch(sampleSong, "pablo 1993")).toBe(true);
    expect(matchesSongSearch(sampleSong, "radiohead jazz")).toBe(false);
  });

  it("matches artist tags via collectionStore.artistProfiles", () => {
    collectionStore.artistProfiles = {
      radiohead: {
        artist_key: "radiohead",
        tags: ["90s", "british", "rock"],
      } as any,
    };

    expect(matchesSongSearch(sampleSong, "british")).toBe(true);
    expect(matchesSongSearch(sampleSong, "tag:rock")).toBe(true);
    expect(matchesSongSearch(sampleSong, "artist-tag:90s")).toBe(true);
    expect(matchesSongSearch(sampleSong, "artist-tag:electronic")).toBe(false);
  });
});
