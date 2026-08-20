import { describe, it, expect, beforeEach, vi } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import type { GenreGroup, Tag } from "../types";

import { tagsStore } from "./tags.svelte";

describe("TagsStore", () => {
  const mockTags: Tag[] = [
    { name: "Metal", song_count: 2 },
    { name: "Symphonic Metal", song_count: 1 },
  ];
  const mockGraph: GenreGroup[] = [
    { main_tag: "Metal", song_count: 1, children: [{ name: "Symphonic Metal", song_count: 1 }] },
  ];

  beforeEach(() => {
    vi.clearAllMocks();
    vi.mocked(invoke).mockImplementation(async (cmd: string, args?: any) => {
      switch (cmd) {
        case "list_all_tags":
          return mockTags;
        case "get_genre_graph":
          return mockGraph;
        case "get_songs_by_tag":
          return [{ id: 1, title: "Song A" }];
        case "get_songs_by_main_tag":
          return [{ id: 2, title: "Song B" }];
        case "get_songs_by_genre_edge":
          return [{ id: 3, title: "Song C" }];
        default:
          throw new Error(`Unhandled invoke: ${cmd}`);
      }
    });
  });

  it("loads all tags and the genre graph", async () => {
    await tagsStore.load();
    expect(tagsStore.allTags).toEqual(mockTags);
    expect(tagsStore.genreGraph).toEqual(mockGraph);
    expect(tagsStore.loaded).toBe(true);
  });

  it("fetches songs by tag (any position)", async () => {
    const songs = await tagsStore.getSongsByTag("Metal", 50);
    expect(invoke).toHaveBeenCalledWith("get_songs_by_tag", { tagName: "Metal", limit: 50, mode: undefined });
    expect(songs).toHaveLength(1);
  });

  it("fetches songs by main tag (strict, for Genre-view roots)", async () => {
    const songs = await tagsStore.getSongsByMainTag("Metal", 50);
    expect(invoke).toHaveBeenCalledWith("get_songs_by_main_tag", { tagName: "Metal", limit: 50, mode: undefined });
    expect(songs).toHaveLength(1);
  });

  it("fetches songs by genre edge (for Genre-view children)", async () => {
    const songs = await tagsStore.getSongsByGenreEdge("Metal", "Symphonic Metal", 50);
    expect(invoke).toHaveBeenCalledWith("get_songs_by_genre_edge", {
      rootTag: "Metal",
      childTag: "Symphonic Metal",
      limit: 50,
      mode: undefined,
    });
    expect(songs).toHaveLength(1);
  });
});
