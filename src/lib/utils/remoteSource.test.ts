import { describe, it, expect } from "vitest";
import { isRemotePath, isRemoteSource, parseSubsonicPath } from "./remoteSource";

describe("remoteSource", () => {
  describe("isRemotePath", () => {
    it("matches WebDAV URLs and subsonic:// paths", () => {
      expect(isRemotePath("https://cloud.example.com/webdav/a.flac")).toBe(true);
      expect(isRemotePath("http://nas.local/a.mp3")).toBe(true);
      expect(isRemotePath("subsonic://1/tr-1")).toBe(true);
      expect(isRemotePath("SUBSONIC://1/tr-1")).toBe(true);
    });

    it("rejects local paths and empty values", () => {
      expect(isRemotePath("/home/me/Music/a.flac")).toBe(false);
      expect(isRemotePath("C:\\Music\\a.flac")).toBe(false);
      expect(isRemotePath("")).toBe(false);
      expect(isRemotePath(null)).toBe(false);
      expect(isRemotePath(undefined)).toBe(false);
    });
  });

  describe("isRemoteSource", () => {
    it("trusts the source when present", () => {
      expect(isRemoteSource({ source: "subsonic", path: "/weird/local-looking" })).toBe(true);
      expect(isRemoteSource({ source: "web_dav" })).toBe(true);
      expect(isRemoteSource({ source: "local_file", path: "/home/me/a.flac" })).toBe(false);
    });

    it("falls back to the path when the source is missing", () => {
      expect(isRemoteSource({ path: "subsonic://2/x" })).toBe(true);
      expect(isRemoteSource({ path: "/home/me/a.flac" })).toBe(false);
      expect(isRemoteSource(null)).toBe(false);
    });
  });

  describe("parseSubsonicPath", () => {
    it("splits server id and track id", () => {
      expect(parseSubsonicPath("subsonic://12/tr-abc/def")).toEqual({ serverId: 12, trackId: "tr-abc/def" });
    });

    it("returns null for anything else", () => {
      expect(parseSubsonicPath("subsonic://abc/tr")).toBeNull();
      expect(parseSubsonicPath("subsonic://1/")).toBeNull();
      expect(parseSubsonicPath("https://x/1/2")).toBeNull();
      expect(parseSubsonicPath(null)).toBeNull();
    });
  });
});
