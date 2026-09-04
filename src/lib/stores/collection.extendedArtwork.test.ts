import { describe, it, expect, beforeEach, vi } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type { ExtendedArtworkResponse } from "../types";

vi.mock("@tauri-apps/api/window", () => ({
  getCurrentWindow: vi.fn(() => ({
    onResized: vi.fn(() => Promise.resolve(() => {})),
    onMoved: vi.fn(() => Promise.resolve(() => {})),
  })),
}));

import { collectionStore } from "./collection.svelte";

const SAMPLE_RESPONSE: ExtendedArtworkResponse = {
  count: 2,
  primary_uri: "luminous-art://local/C:/Music/Artist/Album/cover.jpg",
  artist_portrait_uri: null,
  band_logo_uri: null,
  fanart_uri: null,
  items: [
    { category: "primary_cover", uri: "luminous-art://local/C:/Music/Artist/Album/cover.jpg" },
    { category: "back_cover", uri: "luminous-art://local/C:/Music/Artist/Album/back.jpg" },
  ],
};

describe("CollectionStore - extended artwork (#98/#759)", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    // Cached results from earlier tests must not leak between cases.
    collectionStore.extendedArtworkBySong = {};
    collectionStore.extendedArtworkByArtist = {};

    vi.mocked(listen).mockImplementation(async () => () => {});
  });

  it("fetches extended artwork for a song and caches the result", async () => {
    const invokeMock = vi.mocked(invoke).mockImplementation(async (cmd: string) => {
      if (cmd === "get_extended_artwork_for_song") return SAMPLE_RESPONSE;
      return null;
    });

    const first = await collectionStore.getExtendedArtworkForSong(42);
    expect(invokeMock).toHaveBeenCalledWith("get_extended_artwork_for_song", { songId: 42 });
    expect(first).toEqual(SAMPLE_RESPONSE);
    expect(collectionStore.extendedArtworkBySong[42]).toEqual(SAMPLE_RESPONSE);

    // Second call for the same song must hit the cache, not the backend again.
    const second = await collectionStore.getExtendedArtworkForSong(42);
    expect(second).toEqual(SAMPLE_RESPONSE);
    expect(invokeMock).toHaveBeenCalledTimes(1);
  });

  it("dedupes concurrent fetches for the same song into one backend call", async () => {
    let resolveInvoke: (value: ExtendedArtworkResponse) => void;
    const pending = new Promise<ExtendedArtworkResponse>((resolve) => {
      resolveInvoke = resolve;
    });
    const invokeMock = vi.mocked(invoke).mockImplementation(async (cmd: string) => {
      if (cmd === "get_extended_artwork_for_song") return pending;
      return null;
    });

    const call1 = collectionStore.getExtendedArtworkForSong(7);
    const call2 = collectionStore.getExtendedArtworkForSong(7);
    resolveInvoke!(SAMPLE_RESPONSE);

    const [result1, result2] = await Promise.all([call1, call2]);
    expect(result1).toEqual(SAMPLE_RESPONSE);
    expect(result2).toEqual(SAMPLE_RESPONSE);
    expect(invokeMock).toHaveBeenCalledTimes(1);
  });

  it("fetches extended artwork for an artist keyed case-insensitively", async () => {
    const invokeMock = vi.mocked(invoke).mockImplementation(async (cmd: string, args?: any) => {
      if (cmd === "get_extended_artwork_for_artist" && args?.artist === "Shania Twain") {
        return SAMPLE_RESPONSE;
      }
      return null;
    });

    const result = await collectionStore.getExtendedArtworkForArtist("Shania Twain");
    expect(result).toEqual(SAMPLE_RESPONSE);
    expect(collectionStore.extendedArtworkByArtist["shania twain"]).toEqual(SAMPLE_RESPONSE);

    // A different-cased lookup for the same artist should hit the cache.
    const cached = await collectionStore.getExtendedArtworkForArtist("SHANIA TWAIN");
    expect(cached).toEqual(SAMPLE_RESPONSE);
    expect(invokeMock).toHaveBeenCalledTimes(1);
  });

  it("returns an empty result without throwing when the backend call fails", async () => {
    vi.mocked(invoke).mockImplementation(async (cmd: string) => {
      if (cmd === "get_extended_artwork_for_song") throw new Error("scan failed");
      return null;
    });

    const result = await collectionStore.getExtendedArtworkForSong(99);
    expect(result).toEqual({
      count: 0,
      primary_uri: null,
      artist_portrait_uri: null,
      band_logo_uri: null,
      fanart_uri: null,
      items: [],
    });
    // A failed fetch isn't cached — a later call should retry, not stay stuck empty.
    expect(collectionStore.extendedArtworkBySong[99]).toBeUndefined();
  });

  it("returns an empty result for a null/undefined artist name without invoking", async () => {
    const invokeMock = vi.mocked(invoke);
    const result = await collectionStore.getExtendedArtworkForArtist(undefined);
    expect(result.count).toBe(0);
    expect(invokeMock).not.toHaveBeenCalledWith("get_extended_artwork_for_artist", expect.anything());
  });

  it("opens an artwork path via the open_artwork_path command", async () => {
    const invokeMock = vi.mocked(invoke).mockResolvedValue(undefined);
    await collectionStore.openArtworkPath("C:/Music/Artist/Album/cover.jpg");
    expect(invokeMock).toHaveBeenCalledWith("open_artwork_path", { path: "C:/Music/Artist/Album/cover.jpg" });
  });
});
