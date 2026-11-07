import { describe, it, expect } from "vitest";
import { SharedTagFields } from "./sharedTagFields.svelte";
import type { Song } from "../types";

const KEEP = { mode: "keep" };

function track(overrides: Partial<Song>): Song {
  return {
    album: "Night Visions",
    albumsort: null,
    album_artist: "Imagine Dragons",
    album_artist_sort: null,
    artist: "Imagine Dragons",
    genre: "Pop",
    genresort: null,
    year: 2012,
    disc: 1,
    disc_subtitle: null,
    compilation: false,
    ...overrides,
  } as Song;
}

describe("SharedTagFields", () => {
  it("shows shared values, marks differing ones mixed, and sends only edits", () => {
    const fields = new SharedTagFields();
    fields.reset([track({ disc: 1 }), track({ disc: 3, disc_subtitle: "Remixes" })]);

    expect(fields.album.value).toBe("Night Visions");
    expect(fields.disc.showsMixed).toBe(true);
    expect(fields.discSubtitle.showsMixed).toBe(true);

    fields.year.set(2013);
    expect(fields.edit).toEqual({
      album: KEEP,
      albumsort: KEEP,
      albumArtist: KEEP,
      albumArtistSort: KEEP,
      genre: KEEP,
      genresort: KEEP,
      year: { mode: "set", value: 2013 },
      disc: KEEP,
      discSubtitle: KEEP,
      compilation: KEEP,
    });
  });

  it("shows a track without an album artist under its track artist", () => {
    const fields = new SharedTagFields();
    fields.reset([track({ album_artist: "" }), track({})]);
    expect(fields.albumArtist.value).toBe("Imagine Dragons");
    expect(fields.albumArtist.showsMixed).toBe(false);
  });

  it("treats compilation flags that differ as mixed", () => {
    const fields = new SharedTagFields();
    fields.reset([track({ compilation: true }), track({ compilation: false })]);
    expect(fields.compilation.showsMixed).toBe(true);
  });
});
