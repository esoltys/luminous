import { describe, it, expect, vi } from "vitest";

vi.mock("../platform", () => ({ isWindows: true }));

import { getCoverArtUrl, resolveArtUrl } from "./index";

describe("getCoverArtUrl on Windows", () => {
  it("rewrites cached art to the http luminous-art host unchanged", () => {
    expect(getCoverArtUrl("luminous-art://album-123.jpg")).toBe(
      "http://luminous-art.localhost/album-123.jpg"
    );
  });

  it("percent-encodes a local path so '#', '?' and '%' survive as http", () => {
    const path = "Z:\\Music Library\\ERRA [2024] #1? 100%\\folder.jpg";
    const url = getCoverArtUrl(`luminous-art://local/${path}`)!;
    const parsed = new URL(url);
    expect(parsed.hash).toBe("");
    expect(parsed.search).toBe("");
    expect(decodeURIComponent(parsed.pathname.slice("/local/".length))).toBe(path);
  });

  it("keeps a UNC share path's leading backslashes intact", () => {
    const path = "\\\\nas\\music\\Artist\\Album\\folder.jpg";
    const url = resolveArtUrl(path)!;
    expect(decodeURIComponent(new URL(url).pathname.slice("/local/".length))).toBe(path);
  });
});
