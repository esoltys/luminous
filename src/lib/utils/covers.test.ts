import { describe, it, expect, afterEach } from "vitest";
import {
  getArtistCoverStack,
  songsToCoverStack,
  resolveArtistPortraitUrl,
  resolveArtistLogoUrl,
  resolveArtistBackgroundUrl,
  coverRequestUrl,
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

describe("coverRequestUrl sizing (#1528)", () => {
  it("picks the smallest card-sized copy that covers the box, on both URL forms", () => {
    const url = "http://luminous-art.localhost/album-1.jpg";
    expect(coverRequestUrl(url, { devicePixels: 72 })).toBe(`${url}?w=256`);
    expect(coverRequestUrl(url, { devicePixels: 256 })).toBe(`${url}?w=256`);
    expect(coverRequestUrl(url, { devicePixels: 360 })).toBe(`${url}?w=384`);
    expect(coverRequestUrl("luminous-art://album-1.jpg", { devicePixels: 300 })).toBe("luminous-art://album-1.jpg?w=384");
  });

  it("uses the unsized copy for a box bigger than the largest, or one not measured", () => {
    const url = "http://luminous-art.localhost/album-1.jpg";
    expect(coverRequestUrl(url, { devicePixels: 385 })).toBe(url);
    expect(coverRequestUrl(url, { devicePixels: 0 })).toBe(url);
  });

  it("sizes a folder-art thumbnail", () => {
    const url = "http://luminous-art.localhost/thumb/D%3A%2FMusic%2Fcover.jpg";
    expect(coverRequestUrl(url, { devicePixels: 200 })).toBe(`${url}?w=256`);
  });

  it("leaves originals, remote and mock URLs alone", () => {
    for (const url of [
      "http://luminous-art.localhost/local/D%3A%2Fcover.jpg",
      "luminous-art://embedded/album-1.jpg/D%3A%2Fsong.flac",
      "https://is1-ssl.mzstatic.com/image/600x600.jpg",
      "/covers/album-1.jpg",
    ]) {
      expect(coverRequestUrl(url, { devicePixels: 200 })).toBe(url);
    }
  });
});

describe("coverRequestUrl versioning (#1755)", () => {
  it("adds the version alone or after the size", () => {
    expect(coverRequestUrl("luminous-art://album-1.jpg", { version: 3 })).toBe("luminous-art://album-1.jpg?v=3");
    expect(coverRequestUrl("http://luminous-art.localhost/album-1.jpg", { devicePixels: 200, version: 2 })).toBe(
      "http://luminous-art.localhost/album-1.jpg?w=256&v=2"
    );
  });

  it("versions originals it doesn't size", () => {
    expect(coverRequestUrl("luminous-art://local/D%3A%2Fcover.jpg", { devicePixels: 200, version: 1 })).toBe(
      "luminous-art://local/D%3A%2Fcover.jpg?v=1"
    );
  });

  it("starts the query after a folder-art path that holds a literal ?", () => {
    expect(coverRequestUrl("luminous-art://thumb/a?b.jpg", { devicePixels: 200, version: 1 })).toBe(
      "luminous-art://thumb/a?b.jpg?v=1"
    );
  });

  it("leaves URLs alone at version 0, and remote or mock URLs always", () => {
    expect(coverRequestUrl("luminous-art://album-1.jpg", { version: 0 })).toBe("luminous-art://album-1.jpg");
    expect(coverRequestUrl("https://is1-ssl.mzstatic.com/image/600x600.jpg", { version: 4 })).toBe(
      "https://is1-ssl.mzstatic.com/image/600x600.jpg"
    );
    expect(coverRequestUrl("/covers/album-1.jpg", { version: 4 })).toBe("/covers/album-1.jpg");
  });
});
