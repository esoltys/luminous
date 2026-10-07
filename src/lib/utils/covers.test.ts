import { describe, it, expect, afterEach } from "vitest";
import {
  getArtistCoverStack,
  songsToCoverStack,
  resolveArtistPortraitUrl,
  resolveArtistLogoUrl,
  resolveArtistBackgroundUrl,
  sizedCoverSrcset,
} from "./covers";
import { prefs } from "../stores/prefs.svelte";

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

describe("artist image resolvers (#1276)", () => {
  afterEach(() => {
    prefs.fanartFetchPhoto = true;
    prefs.fanartFetchLogo = true;
    prefs.fanartFetchBackground = true;
  });

  it("shows a fetched photo, logo and background by default", () => {
    expect(resolveArtistPortraitUrl(null, "abc.jpg")).toContain("abc.jpg");
    expect(resolveArtistLogoUrl(null, "abc.jpg")).toContain("abc.jpg");
    expect(resolveArtistBackgroundUrl(null, "abc.jpg")).toContain("abc.jpg");
  });

  const cases = [
    { name: "photo", resolve: resolveArtistPortraitUrl, pref: "fanartFetchPhoto" },
    { name: "logo", resolve: resolveArtistLogoUrl, pref: "fanartFetchLogo" },
    { name: "background", resolve: resolveArtistBackgroundUrl, pref: "fanartFetchBackground" },
  ] as const;

  for (const { name, resolve, pref } of cases) {
    it(`shows a fetched ${name} when no local file exists and its type is on`, () => {
      prefs[pref] = true;
      expect(resolve(null, "abc.jpg")).toContain("abc.jpg");
    });

    it(`prefers a local ${name} file over a fetched one`, () => {
      prefs[pref] = true;
      const url = resolve("C:/Music/Artist/local.jpg", "abc.jpg");
      expect(url).toContain("local.jpg");
      expect(url).not.toContain("abc.jpg");
    });

    it(`hides a fetched ${name} but keeps a local one when its type is turned off`, () => {
      prefs[pref] = false;
      expect(resolve(null, "abc.jpg")).toBeNull();
      expect(resolve("C:/Music/Artist/local.jpg", "abc.jpg")).toContain("local.jpg");
    });
  }
});

describe("sizedCoverSrcset (#1528)", () => {
  it("offers card-sized copies of a cached cover, on both URL forms", () => {
    expect(sizedCoverSrcset("http://luminous-art.localhost/album-1.jpg")).toBe(
      "http://luminous-art.localhost/album-1.jpg?w=256 256w, " +
        "http://luminous-art.localhost/album-1.jpg?w=384 384w, " +
        "http://luminous-art.localhost/album-1.jpg 600w"
    );
    expect(sizedCoverSrcset("luminous-art://album-1.jpg")).toBe(
      "luminous-art://album-1.jpg?w=256 256w, luminous-art://album-1.jpg?w=384 384w, luminous-art://album-1.jpg 600w"
    );
  });

  it("offers them for a folder-art thumbnail", () => {
    const url = "http://luminous-art.localhost/thumb/D%3A%2FMusic%2Fcover.jpg";
    expect(sizedCoverSrcset(url)).toContain(`${url}?w=256 256w`);
  });

  it("leaves originals, remote and mock URLs alone", () => {
    expect(sizedCoverSrcset("http://luminous-art.localhost/local/D%3A%2Fcover.jpg")).toBeNull();
    expect(sizedCoverSrcset("luminous-art://embedded/album-1.jpg/D%3A%2Fsong.flac")).toBeNull();
    expect(sizedCoverSrcset("https://is1-ssl.mzstatic.com/image/600x600.jpg")).toBeNull();
    expect(sizedCoverSrcset("/covers/album-1.jpg")).toBeNull();
    expect(sizedCoverSrcset(null)).toBeNull();
  });
});
