import { describe, it, expect } from "vitest";
import { combineWebdavPath, matchWebDavPath } from "./webdavDisplay";

describe("webdavDisplay - combineWebdavPath", () => {
  it("combines URL without trailing slash and remote path", () => {
    expect(combineWebdavPath("http://127.0.0.1:8080", "/Music/BandCamp/")).toBe(
      "http://127.0.0.1:8080/Music/BandCamp/"
    );
  });

  it("combines URL with trailing slash and remote path", () => {
    expect(combineWebdavPath("http://127.0.0.1:8080/", "/Music")).toBe(
      "http://127.0.0.1:8080/Music"
    );
  });

  it("handles root remote path by returning base without trailing slash", () => {
    expect(combineWebdavPath("http://127.0.0.1:8080", "/")).toBe("http://127.0.0.1:8080");
    expect(combineWebdavPath("http://127.0.0.1:8080/", "/")).toBe("http://127.0.0.1:8080");
  });

  it("handles empty remote path", () => {
    expect(combineWebdavPath("http://127.0.0.1:8080", "")).toBe("http://127.0.0.1:8080");
  });
});

describe("webdavDisplay - matchWebDavPath", () => {
  it("matches a song with percent-encoded spaces against an unencoded server remote path (#1804)", () => {
    const result = matchWebDavPath(
      "https://webdav.files.fm/Remote%20Library/01%20Tell%20Me%20(U%20Want%20It).mp3",
      "https://webdav.files.fm/",
      "/Remote Library"
    );

    expect(result.matches).toBe(true);
    expect(result.base).toBe("https://webdav.files.fm/Remote Library");
    expect(result.prefixLength).toBe("/remote library".length);
  });

  it("matches a song with credentialed URL", () => {
    const result = matchWebDavPath(
      "http://user:secret@nas.local:8080/Music/track.flac",
      "http://nas.local:8080",
      "/Music"
    );

    expect(result.matches).toBe(true);
    expect(result.base).toBe("http://nas.local:8080/Music");
  });

  it("matches when remote path is root /", () => {
    const result = matchWebDavPath(
      "http://nas.local:8080/track.flac",
      "http://nas.local:8080",
      "/"
    );

    expect(result.matches).toBe(true);
  });

  it("does not false-match sibling folders with common prefix", () => {
    const result = matchWebDavPath(
      "http://nas.local:8080/Music2/track.flac",
      "http://nas.local:8080",
      "/Music"
    );

    expect(result.matches).toBe(false);
  });

  it("does not match different hosts or protocols", () => {
    expect(
      matchWebDavPath("http://other.host/Music/track.mp3", "http://nas.local:8080", "/Music").matches
    ).toBe(false);

    expect(
      matchWebDavPath("https://nas.local:8080/Music/track.mp3", "http://nas.local:8080", "/Music").matches
    ).toBe(false);

    expect(
      matchWebDavPath("http://nas.local:9000/Music/track.mp3", "http://nas.local:8080", "/Music").matches
    ).toBe(false);
  });

  it("returns matches false when songPath or serverUrl is null or empty", () => {
    expect(matchWebDavPath(null, "http://nas.local", "/").matches).toBe(false);
    expect(matchWebDavPath("http://nas.local/t.mp3", "", "/").matches).toBe(false);
  });
});
