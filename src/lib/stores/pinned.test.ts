import { describe, it, expect, beforeEach, vi } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type { PinnedItem } from "../types";

import { pinnedStore } from "./pinned.svelte";

describe("PinnedStore", () => {
  let eventCallbacks: Record<string, Function> = {};

  const mockItems: PinnedItem[] = [
    { type: "song", song: { id: 10, title: "Song A" } as any },
    { type: "album", album: { album: "Album A", artist: "Artist A" } as any },
    { type: "artist", artist: { name: "Artist A" } as any },
    { type: "playlist", playlist: { id: 5, name: "Playlist A" } as any },
    { type: "auto_playlist", autoPlaylist: { kind: "favourites", trackCount: 12 } as any },
  ];

  beforeEach(() => {
    vi.clearAllMocks();
    eventCallbacks = {};

    vi.mocked(listen).mockImplementation(async (event: string, callback: any) => {
      eventCallbacks[event] = callback;
      return () => {};
    });

    vi.mocked(invoke).mockImplementation(async (cmd: string) => {
      switch (cmd) {
        case "get_pinned_items":
          return mockItems;
        default:
          return null;
      }
    });
  });

  it("loads pinned items on refresh and reports isPinned per type/refKey", async () => {
    await pinnedStore.refresh();
    expect(pinnedStore.items).toHaveLength(5);
    expect(pinnedStore.isPinned("song", "10")).toBe(true);
    expect(pinnedStore.isPinned("album", "Album A")).toBe(true);
    expect(pinnedStore.isPinned("artist", "Artist A")).toBe(true);
    expect(pinnedStore.isPinned("playlist", "5")).toBe(true);
    expect(pinnedStore.isPinned("auto_playlist", "favourites")).toBe(true);
    expect(pinnedStore.isPinned("song", "999")).toBe(false);
  });

  it("pin() invokes pin_item with itemType/refKey and refreshes", async () => {
    await pinnedStore.pin("song", "42");
    expect(invoke).toHaveBeenCalledWith("pin_item", { itemType: "song", refKey: "42" });
    expect(invoke).toHaveBeenCalledWith("get_pinned_items");
  });

  it("unpin() invokes unpin_item with itemType/refKey and refreshes", async () => {
    await pinnedStore.unpin("song", "42");
    expect(invoke).toHaveBeenCalledWith("unpin_item", { itemType: "song", refKey: "42" });
    expect(invoke).toHaveBeenCalledWith("get_pinned_items");
  });

  it("toggle() unpins an already-pinned item and pins one that isn't", async () => {
    await pinnedStore.refresh();

    await pinnedStore.toggle("song", "10");
    expect(invoke).toHaveBeenCalledWith("unpin_item", { itemType: "song", refKey: "10" });

    await pinnedStore.toggle("song", "999");
    expect(invoke).toHaveBeenCalledWith("pin_item", { itemType: "song", refKey: "999" });
  });

  it("reorder() invokes reorder_pinned_items with the given order and refreshes", async () => {
    const order: Array<["song", string]> = [["song", "2"], ["song", "1"]];
    await pinnedStore.reorder(order);
    expect(invoke).toHaveBeenCalledWith("reorder_pinned_items", { order });
    expect(invoke).toHaveBeenCalledWith("get_pinned_items");
  });

  it("filters visible items using hasPinnedContent and preserves hidden items on reorderVisible", async () => {
    const itemsWithEmpty: PinnedItem[] = [
      { type: "song", song: { id: 1, title: "Song 1" } as any },
      { type: "playlist", playlist: { id: 10, name: "Empty Playlist", track_count: 0 } as any },
      { type: "song", song: { id: 2, title: "Song 2" } as any },
      { type: "song", song: { id: 3, title: "Song 3" } as any },
    ];
    vi.mocked(invoke).mockImplementation(async (cmd: string) => {
      if (cmd === "get_pinned_items") return itemsWithEmpty;
      return null;
    });

    await pinnedStore.refresh();
    expect(pinnedStore.items).toHaveLength(4);
    // The empty playlist (track_count = 0) is excluded from visibleItems
    expect(pinnedStore.visibleItems).toHaveLength(3);
    expect(pinnedStore.visibleItems.map((i) => i.type === "song" && i.song.id)).toEqual([1, 2, 3]);

    // Move visible item 0 ("Song 1") to visible index 2 ("Song 3")
    await pinnedStore.reorderVisible(0, 2);
    // Order in persistent storage should have Song 2, Song 3, Song 1 with the empty playlist preserved
    expect(invoke).toHaveBeenCalledWith("reorder_pinned_items", {
      order: [
        ["playlist", "10"],
        ["song", "2"],
        ["song", "3"],
        ["song", "1"],
      ],
    });
  });
});

