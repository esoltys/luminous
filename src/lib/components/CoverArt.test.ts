import "@testing-library/jest-dom";
import { describe, it, expect, vi, beforeEach } from "vitest";
import { render } from "@testing-library/svelte";
import { invoke } from "@tauri-apps/api/core";
import CoverArt from "./CoverArt.svelte";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
  convertFileSrc: (path: string) => `asset://${path}`,
}));

vi.mock("@tauri-apps/api/window", () => ({
  getCurrentWindow: () => ({
    onResized: vi.fn().mockResolvedValue(() => {}),
    onMoved: vi.fn().mockResolvedValue(() => {}),
  }),
}));

describe("CoverArt.svelte - fanart.tv cover fallback (#1277)", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it("shows the fanart.tv cover once the iTunes lookup misses", async () => {
    let coverLookups = 0;
    vi.mocked(invoke).mockImplementation(async (cmd: string) => {
      if (cmd === "get_cover_art_uri") {
        // First lookup: nothing yet. After iTunes misses, the backend
        // resolves the song's fanart.tv cover as its fallback.
        return coverLookups++ === 0 ? null : "luminous-art://abc_fanart_cover.jpg";
      }
      return null;
    });

    const { container } = render(CoverArt, { props: { songId: 7 } });

    await vi.waitFor(() => {
      expect(container.querySelector("img")?.getAttribute("src")).toContain("abc_fanart_cover.jpg");
    });
    expect(invoke).toHaveBeenCalledWith("fetch_remote_cover", { songId: 7 });
  });

  it("shows the placeholder when neither iTunes nor fanart.tv has a cover", async () => {
    vi.mocked(invoke).mockResolvedValue(null);

    const { container } = render(CoverArt, { props: { songId: 7 } });

    await vi.waitFor(() => {
      expect(vi.mocked(invoke).mock.calls.filter(([c]) => c === "get_cover_art_uri")).toHaveLength(2);
    });
    expect(container.querySelector("img")).toBeNull();
  });
});
