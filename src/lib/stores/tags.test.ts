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
        case "get_song_tags":
          return ["Metal", "Symphonic Metal"];
        case "set_song_tags":
          return undefined;
        case "get_songs_by_tag":
          return [{ id: 1, title: "Song A" }];
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

  it("fetches a song's tags in order", async () => {
    const tags = await tagsStore.getSongTags(42);
    expect(invoke).toHaveBeenCalledWith("get_song_tags", { songId: 42 });
    expect(tags).toEqual(["Metal", "Symphonic Metal"]);
  });

  it("sets a song's tags and refreshes the cached lists", async () => {
    await tagsStore.setSongTags(42, ["Metal", "Symphonic Metal"]);
    expect(invoke).toHaveBeenCalledWith("set_song_tags", { songId: 42, tagNames: ["Metal", "Symphonic Metal"] });
    expect(invoke).toHaveBeenCalledWith("list_all_tags");
    expect(invoke).toHaveBeenCalledWith("get_genre_graph");
  });

  it("fetches songs by tag", async () => {
    const songs = await tagsStore.getSongsByTag("Metal", 50);
    expect(invoke).toHaveBeenCalledWith("get_songs_by_tag", { tagName: "Metal", limit: 50, mode: undefined });
    expect(songs).toHaveLength(1);
  });
});
