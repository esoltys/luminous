import "@testing-library/jest-dom";
import { describe, it, expect, vi, beforeEach } from "vitest";
import { render } from "@testing-library/svelte";
import PlaylistsCollectionView from "./PlaylistsCollectionView.svelte";
import { collectionStore } from "../stores/collection.svelte";
import { playlistsStore } from "../stores/playlists.svelte";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn().mockImplementation((cmd: string) => {
    if (cmd === "get_playlists") {
      return Promise.resolve([
        {
          id: 1,
          name: "1980s",
          dynamic_enabled: true,
          dynamic_spec: "decade:1980s",
          track_count: 12,
          created: 1700000000,
          updated: 1700000000,
        },
        {
          id: 2,
          name: "Rock",
          dynamic_enabled: true,
          dynamic_spec: "genre:Rock",
          track_count: 8,
          created: 1700000000,
          updated: 1700000000,
        },
      ]);
    }
    if (cmd === "get_favourite_songs") return Promise.resolve([]);
    if (cmd === "get_recently_added_songs") return Promise.resolve([]);
    if (cmd === "sync_genre_auto_playlists" || cmd === "sync_decade_auto_playlists") {
      return Promise.resolve(null);
    }
    return Promise.resolve([]);
  }),
}));

vi.mock("@tauri-apps/plugin-dialog", () => ({
  open: vi.fn().mockResolvedValue(null),
}));

describe("PlaylistsCollectionView.svelte - Decades Auto Playlists", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    collectionStore.selectedPlaylistId = null;
    collectionStore.selectedAutoPlaylist = null;
    collectionStore.playlistsSubTab = "auto";
  });

  it("renders decade auto-playlist cards in the auto playlist grid", async () => {
    playlistsStore.playlists = [
      {
        id: 1,
        name: "1980s",
        dynamic_enabled: true,
        dynamic_spec: "decade:1980s",
        track_count: 12,
        created: 1700000000,
        updated: 1700000000,
      },
      {
        id: 2,
        name: "Rock",
        dynamic_enabled: true,
        dynamic_spec: "genre:Rock",
        track_count: 8,
        created: 1700000000,
        updated: 1700000000,
      },
    ];

    const { getByText } = render(PlaylistsCollectionView);
    expect(getByText("1980s")).toBeInTheDocument();
    expect(getByText("Rock")).toBeInTheDocument();
  });
});
