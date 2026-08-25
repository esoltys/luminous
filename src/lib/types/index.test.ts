import { describe, it, expect } from "vitest";
import { resolveArtUrl, getCoverArtUrl } from "./index";

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
