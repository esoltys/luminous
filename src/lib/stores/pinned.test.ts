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
    expect(pinnedStore.items).toHaveLength(4);
    expect(pinnedStore.isPinned("song", "10")).toBe(true);
    expect(pinnedStore.isPinned("album", "Album A")).toBe(true);
    expect(pinnedStore.isPinned("artist", "Artist A")).toBe(true);
    expect(pinnedStore.isPinned("playlist", "5")).toBe(true);
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
});
