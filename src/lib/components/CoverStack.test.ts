import "@testing-library/jest-dom";
import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, waitFor, fireEvent } from "@testing-library/svelte";
import { invoke } from "@tauri-apps/api/core";
import type { ExtendedArtworkResponse } from "../types";
import CoverStack from "./CoverStack.svelte";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn().mockResolvedValue(""),
}));

vi.mock("@tauri-apps/api/window", () => ({
  getCurrentWindow: vi.fn(() => ({
    onResized: vi.fn(() => Promise.resolve(() => {})),
    onMoved: vi.fn(() => Promise.resolve(() => {})),
  })),
}));

import { collectionStore } from "../stores/collection.svelte";

describe("CoverStack.svelte", () => {
  const mockCovers = [
    { songId: 1, artAutomatic: "cover1.jpg" },
    { songId: 2, artAutomatic: "cover2.jpg" },
  ];

  it("centers content by default when direction is right", () => {
    const { container } = render(CoverStack, {
      props: { covers: mockCovers, direction: "right" },
    });

    const rootDiv = container.firstElementChild as HTMLElement;
    expect(rootDiv.className).toContain("justify-center");
    expect(rootDiv.className).not.toContain("justify-end");
  });

  it("right-aligns content when direction is left", () => {
    const { container } = render(CoverStack, {
      props: { covers: mockCovers, direction: "left" },
    });

    const rootDiv = container.firstElementChild as HTMLElement;
    expect(rootDiv.className).toContain("justify-end");
    expect(rootDiv.className).not.toContain("justify-center");
  });

  it("renders single cover correctly with direction left", () => {
    const { container } = render(CoverStack, {
      props: { covers: [mockCovers[0]], direction: "left" },
    });

    const rootDiv = container.firstElementChild as HTMLElement;
    expect(rootDiv.className).toContain("justify-end");
  });

  describe("extended artwork (#98/#760)", () => {
    const RESPONSE_WITH_EXTRAS: ExtendedArtworkResponse = {
      count: 3,
      primary_uri: "luminous-art://local/C:/Music/Artist/Album/cover.jpg",
      artist_portrait_uri: null,
      band_logo_uri: null,
      fanart_uri: null,
      items: [
        { category: "primary_cover", uri: "luminous-art://local/C:/Music/Artist/Album/cover.jpg" },
        { category: "back_cover", uri: "luminous-art://local/C:/Music/Artist/Album/back.jpg" },
        { category: "booklet", uri: "luminous-art://local/C:/Music/Artist/Album/booklet.jpg" },
      ],
    };

    const RESPONSE_SINGLE: ExtendedArtworkResponse = {
      count: 1,
      primary_uri: "luminous-art://local/C:/Music/Artist/Album/cover.jpg",
      artist_portrait_uri: null,
      band_logo_uri: null,
      fanart_uri: null,
      items: [{ category: "primary_cover", uri: "luminous-art://local/C:/Music/Artist/Album/cover.jpg" }],
    };

    beforeEach(() => {
      vi.clearAllMocks();
      collectionStore.extendedArtworkBySong = {};
    });

    it("shows a count badge when more than one artwork file was discovered", async () => {
      vi.mocked(invoke).mockImplementation(async (cmd: string) => {
        if (cmd === "get_extended_artwork_for_song") return RESPONSE_WITH_EXTRAS;
        return "";
      });

      const { getByText } = render(CoverStack, {
        props: { covers: [mockCovers[0]], extendedArtworkSongId: 1 },
      });

      await waitFor(() => expect(getByText("3")).toBeInTheDocument());
    });

    it("does not show a count badge when only one artwork file was discovered", async () => {
      vi.mocked(invoke).mockImplementation(async (cmd: string) => {
        if (cmd === "get_extended_artwork_for_song") return RESPONSE_SINGLE;
        return "";
      });

      const { container, getByTitle } = render(CoverStack, {
        props: { covers: [mockCovers[0]], extendedArtworkSongId: 1 },
      });

      // The hover "Open Images" control still appears — there's a real file
      // to open — just no count badge, since "1" would be noise.
      await waitFor(() => expect(getByTitle("Open images")).toBeInTheDocument());
      expect(container.textContent).not.toContain("1");
    });

    it("does not show any extended-artwork UI when no extendedArtworkSongId is given", async () => {
      const { queryByTitle } = render(CoverStack, {
        props: { covers: [mockCovers[0]] },
      });

      // No fetch was even attempted.
      expect(invoke).not.toHaveBeenCalledWith("get_extended_artwork_for_song", expect.anything());
      expect(queryByTitle("Open images")).not.toBeInTheDocument();
    });

    it("opens the primary artwork path when the Open Images control is clicked", async () => {
      const invokeMock = vi.mocked(invoke).mockImplementation(async (cmd: string) => {
        if (cmd === "get_extended_artwork_for_song") return RESPONSE_WITH_EXTRAS;
        if (cmd === "open_artwork_path") return undefined;
        return "";
      });

      const { getByTitle } = render(CoverStack, {
        props: { covers: [mockCovers[0]], extendedArtworkSongId: 1 },
      });

      const openControl = await waitFor(() => getByTitle("Open 3 images"));
      await fireEvent.click(openControl);

      await waitFor(() =>
        expect(invokeMock).toHaveBeenCalledWith("open_artwork_path", {
          path: "C:/Music/Artist/Album/cover.jpg",
        })
      );
    });

    it("shows no extended-artwork UI when rendering a multi-song box-set collage", async () => {
      vi.mocked(invoke).mockImplementation(async (cmd: string) => {
        if (cmd === "get_extended_artwork_for_song") return RESPONSE_WITH_EXTRAS;
        return "";
      });

      const { queryByTitle } = render(CoverStack, {
        // More than one distinct-song cover — the box-set collage case,
        // where a single song's extended artwork isn't meaningful.
        props: { covers: mockCovers, extendedArtworkSongId: 1 },
      });

      await new Promise((r) => setTimeout(r, 0));
      expect(queryByTitle("Open images")).not.toBeInTheDocument();
      expect(queryByTitle("Open 3 images")).not.toBeInTheDocument();
    });
  });
});
