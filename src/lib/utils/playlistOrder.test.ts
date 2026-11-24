import { beforeEach, describe, expect, it } from "vitest";
import { orderPlaylists, loadPlaylistSortPref, savePlaylistSortPref } from "./playlistOrder";
import type { Playlist } from "../types";

let nextId = 1;
function pl(name: string, over: Partial<Playlist> = {}): Playlist {
  return {
    id: nextId++,
    name,
    created: 0,
    updated: 0,
    track_count: 0,
    dynamic_enabled: false,
    is_queue: false,
    kind: "custom",
    ...over,
  };
}
const auto = (name: string, spec: string, over: Partial<Playlist> = {}) =>
  pl(name, {
    dynamic_enabled: true,
    dynamic_spec: spec,
    is_system: true,
    kind: "system_auto",
    ...over,
  });
const names = (list: Playlist[]) => list.map((p) => p.name);

describe("orderPlaylists", () => {
  beforeEach(() => localStorage.clear());

  it("puts the user's own playlists before auto-playlists", () => {
    const out = orderPlaylists([
      auto("Rock", "tag:Rock"),
      pl("Zed"),
      pl("Smart", {
        dynamic_enabled: true,
        dynamic_spec: "rating>=4",
        is_system: false,
        kind: "smart",
      }),
      pl("Alpha"),
    ]);
    expect(names(out)).toEqual(["Alpha", "Smart", "Zed", "Rock"]);
  });

  it("pins singletons first and BPM buckets last, in intensity order", () => {
    const out = orderPlaylists([
      auto("Extreme BPM", "bpmrange:extreme"),
      auto("Jazz", "tag:Jazz"),
      auto("Down-Tempo BPM", "bpmrange:down"),
      auto("Missing Metadata", "missingmeta"),
      auto("Blues", "tag:Blues"),
    ]);
    expect(names(out)).toEqual([
      "Missing Metadata",
      "Blues",
      "Jazz",
      "Down-Tempo BPM",
      "Extreme BPM",
    ]);
  });

  it("follows the saved sort of each tab", () => {
    savePlaylistSortPref("custom", { field: "track_count", asc: false });
    savePlaylistSortPref("auto", { field: "updated", asc: true });
    const out = orderPlaylists([
      pl("Small", { track_count: 1 }),
      pl("Big", { track_count: 9 }),
      auto("New", "tag:New", { updated: 20 }),
      auto("Old", "tag:Old", { updated: 10 }),
    ]);
    expect(names(out)).toEqual(["Big", "Small", "Old", "New"]);
  });

  it("defaults to name ascending when nothing is saved", () => {
    expect(loadPlaylistSortPref("custom")).toEqual({ field: "name", asc: true });
  });
});
