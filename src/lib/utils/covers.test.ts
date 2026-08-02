import { describe, it, expect } from "vitest";
import { getArtistCoverStack, songsToCoverStack } from "./covers";

describe("getArtistCoverStack", () => {
  it("prefers real album art, front cover first", () => {
    const result = getArtistCoverStack(
      [
        { sample_song_id: 1, art_manual: null, art_automatic: null, art_embedded: false },
        { sample_song_id: 2, art_manual: "covers/b.jpg", art_automatic: null, art_embedded: false },
      ],
      []
    );

    expect(result).toEqual([{ songId: 2, artEmbedded: false, artAutomatic: null, artManual: "covers/b.jpg" }]);
  });

  it("falls back to song covers when no album has art", () => {
    const result = getArtistCoverStack(
      [{ sample_song_id: 1, art_manual: null, art_automatic: null, art_embedded: false }],
      [{ id: 10, art_manual: "covers/song.jpg", art_automatic: null, art_embedded: false }]
    );

    expect(result).toEqual(songsToCoverStack([{ id: 10, art_manual: "covers/song.jpg", art_automatic: null, art_embedded: false }]));
  });

  it("returns an empty array when the artist has no art anywhere", () => {
    expect(getArtistCoverStack([], [])).toEqual([]);
  });
});
