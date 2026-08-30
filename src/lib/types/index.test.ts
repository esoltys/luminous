import { describe, it, expect } from "vitest";
import { resolveArtUrl, getCoverArtUrl, pinnedRefKeyFor, autoPlaylistRefKeyFor, type PinnedItem } from "./index";

describe("resolveArtUrl", () => {
  it("returns null for null, undefined, or empty string", () => {
    expect(resolveArtUrl(null)).toBeNull();
    expect(resolveArtUrl(undefined)).toBeNull();
    expect(resolveArtUrl("")).toBeNull();
  });

  it("preserves remote http and https URLs", () => {
    expect(resolveArtUrl("http://example.com/cover.jpg")).toBe("http://example.com/cover.jpg");
    expect(resolveArtUrl("https://example.com/cover.jpg")).toBe("https://example.com/cover.jpg");
  });

  it("handles luminous-art:// URIs directly", () => {
    expect(resolveArtUrl("luminous-art://album-123.jpg")).toBe("luminous-art://album-123.jpg");
  });

  it("wraps cached album art filenames in luminous-art://", () => {
    expect(resolveArtUrl("album-12345.jpg")).toBe("luminous-art://album-12345.jpg");
  });

  it("wraps Unix absolute local filesystem paths in luminous-art://local/", () => {
    const unixPath = "/home/user/Music/Album/folder.jpg";
    expect(resolveArtUrl(unixPath)).toBe(`luminous-art://local/${unixPath}`);
  });

  it("wraps Windows absolute local filesystem paths in luminous-art://local/", () => {
    const winPath = "C:\\Users\\User\\Music\\Album\\folder.jpg";
    expect(resolveArtUrl(winPath)).toBe(`luminous-art://local/${winPath}`);
  });
});

describe("autoPlaylistRefKeyFor", () => {
  it("uses the bare kind for auto-playlists with no backing playlist row", () => {
    expect(autoPlaylistRefKeyFor({ kind: "favourites" })).toBe("favourites");
    expect(autoPlaylistRefKeyFor({ kind: "recently_added" })).toBe("recently_added");
    expect(autoPlaylistRefKeyFor({ kind: "most_played" })).toBe("most_played");
    expect(autoPlaylistRefKeyFor({ kind: "history" })).toBe("history");
  });

  it("keys materialized kinds by their selector, not a playlist id", () => {
    expect(autoPlaylistRefKeyFor({ kind: "genre", genre: "Rock" })).toBe("genre:Rock");
    expect(autoPlaylistRefKeyFor({ kind: "decade", decade: "1980s" })).toBe("decade:1980s");
    expect(autoPlaylistRefKeyFor({ kind: "bpm", bpm: "60-90" })).toBe("bpm:60-90");
    expect(autoPlaylistRefKeyFor({ kind: "artist_tag", artistTag: "Progressive Metal" })).toBe(
      "artist_tag:Progressive Metal"
    );
  });
});

describe("pinnedRefKeyFor", () => {
  it("dispatches each PinnedItem type to its identifying key", () => {
    expect(pinnedRefKeyFor({ type: "song", song: { id: 7 } as any })).toBe("7");
    expect(pinnedRefKeyFor({ type: "album", album: { album: "Album A" } as any })).toBe("Album A");
    expect(pinnedRefKeyFor({ type: "artist", artist: { name: "Artist A" } as any })).toBe("Artist A");
    expect(pinnedRefKeyFor({ type: "playlist", playlist: { id: 3 } as any })).toBe("3");
    const autoPlaylistItem: PinnedItem = {
      type: "auto_playlist",
      autoPlaylist: { kind: "genre", genre: "Jazz", trackCount: 5 } as any,
    };
    expect(pinnedRefKeyFor(autoPlaylistItem)).toBe("genre:Jazz");
  });
});
