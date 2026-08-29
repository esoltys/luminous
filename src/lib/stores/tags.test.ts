import { describe, it, expect, beforeEach, vi } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import type { GenreGroup, Tag, TagGroup } from "../types";

import { tagsStore } from "./tags.svelte";

describe("TagsStore", () => {
  const mockTags: Tag[] = [
    { name: "Metal", song_count: 2 },
    { name: "Symphonic Metal", song_count: 1 },
  ];
  const mockGraph: GenreGroup[] = [
    { main_tag: "Metal", song_count: 1, children: [{ name: "Symphonic Metal", song_count: 1 }] },
  ];
  const mockHierarchy: TagGroup[] = [
    {
      name: "Metal",
      color_index: 0,
      song_count: 1,
      children: [{ name: "Symphonic Metal", song_count: 1 }],
    },
  ];

  beforeEach(() => {
    vi.clearAllMocks();
    vi.mocked(invoke).mockImplementation(async (cmd: string, args?: any) => {
      switch (cmd) {
        case "get_tags_overview":
          return { tags: mockTags, graph: mockGraph, no_genre_count: 3 };
        case "get_songs_by_tag":
          return [{ id: 1, title: "Song A" }];
        case "get_songs_by_curated_tag":
          return [{ id: 2, title: "Song B" }];
        case "get_songs_without_genre":
          return [{ id: 4, title: "Song D" }];
        case "get_tag_hierarchy":
          return mockHierarchy;
        case "set_tag_group_color":
        case "reparent_tag":
        case "promote_tag":
        case "reorder_tag_in_group":
          return null;
        case "merge_tags":
          return 2;
        case "delete_tags":
          return 3;
        default:
          throw new Error(`Unhandled invoke: ${cmd}`);
      }
    });
  });

  it("loads all tags, the genre graph, and the no-genre count", async () => {
    await tagsStore.load();
    expect(tagsStore.allTags).toEqual(mockTags);
    expect(tagsStore.genreGraph).toEqual(mockGraph);
    expect(tagsStore.noGenreCount).toBe(3);
    expect(tagsStore.loaded).toBe(true);
  });

  it("fetches songs without a genre", async () => {
    const songs = await tagsStore.getSongsWithoutGenre(50);
    expect(invoke).toHaveBeenCalledWith("get_songs_without_genre", { limit: 50, mode: undefined });
    expect(songs).toHaveLength(1);
  });

  it("fetches songs by tag (any position)", async () => {
    const songs = await tagsStore.getSongsByTag("Metal", 50);
    expect(invoke).toHaveBeenCalledWith("get_songs_by_tag", { tagName: "Metal", limit: 50, mode: undefined });
    expect(songs).toHaveLength(1);
  });

  it("fetches songs by curated tag (group-vs-child dispatch, #548)", async () => {
    const songs = await tagsStore.getSongsByCuratedTag("Metal", 50);
    expect(invoke).toHaveBeenCalledWith("get_songs_by_curated_tag", { tagName: "Metal", limit: 50, mode: undefined });
    expect(songs).toHaveLength(1);
  });

  it("loads the persisted Genres hierarchy", async () => {
    await tagsStore.loadHierarchy();
    expect(tagsStore.hierarchy).toEqual(mockHierarchy);
  });

  it("sets a group's color and refreshes the hierarchy", async () => {
    await tagsStore.setGroupColor("Metal", 3);
    expect(invoke).toHaveBeenCalledWith("set_tag_group_color", { name: "Metal", colorIndex: 3 });
    expect(invoke).toHaveBeenCalledWith("get_tag_hierarchy");
  });

  it("reparents, promotes, and reorders a tag, refreshing the hierarchy each time", async () => {
    await tagsStore.reparentTag("Symphonic Metal", "Ambient");
    expect(invoke).toHaveBeenCalledWith("reparent_tag", { tagName: "Symphonic Metal", newGroupName: "Ambient" });

    await tagsStore.promoteTag("Symphonic Metal");
    expect(invoke).toHaveBeenCalledWith("promote_tag", { tagName: "Symphonic Metal" });

    await tagsStore.reorderTagInGroup("Symphonic Metal", 0);
    expect(invoke).toHaveBeenCalledWith("reorder_tag_in_group", { tagName: "Symphonic Metal", newIndex: 0 });
  });

  it("merges tags and returns the affected song count", async () => {
    const count = await tagsStore.mergeTags("Prog Metal", "Progressive Metal");
    expect(invoke).toHaveBeenCalledWith("merge_tags", { from: "Prog Metal", into: "Progressive Metal" });
    expect(count).toBe(2);
  });

  it("deletes tags and returns the affected song count", async () => {
    const count = await tagsStore.deleteTags(["Symphonic Metal"]);
    expect(invoke).toHaveBeenCalledWith("delete_tags", { names: ["Symphonic Metal"] });
    expect(count).toBe(3);
  });
});
