import "@testing-library/jest-dom";
import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, fireEvent, waitFor } from "@testing-library/svelte";
import PlaylistsCollectionView from "./PlaylistsCollectionView.svelte";
import { collectionStore } from "../stores/collection.svelte";
import { navigationStore } from "../stores/navigation.svelte";
import { playlistsStore } from "../stores/playlists.svelte";
import { pinnedStore } from "../stores/pinned.svelte";
import { invoke } from "@tauri-apps/api/core";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn().mockImplementation((cmd: string) => {
    if (cmd === "get_playlists") {
      return Promise.resolve([
        {
          id: 1,
          name: "1980s",
          dynamic_enabled: true,
          is_queue: false,
          dynamic_spec: "decade:1980s",
          track_count: 12,
          created: 1700000000,
          updated: 1700000000,
        },
        {
          id: 2,
          name: "Rock",
          dynamic_enabled: true,
          is_queue: false,
          dynamic_spec: "tag:Rock",
          track_count: 8,
          created: 1700000000,
          updated: 1700000000,
        },
      ]);
    }
    if (cmd === "get_favourite_songs") return Promise.resolve([]);
    if (cmd === "get_recently_added_songs") return Promise.resolve([]);
    if (cmd === "sync_all_auto_playlists") return Promise.resolve(null);
    if (cmd === "refresh_all_auto_playlists") return Promise.resolve(null);
    return Promise.resolve([]);
  }),
}));

vi.mock("@tauri-apps/plugin-dialog", () => ({
  open: vi.fn().mockResolvedValue(null),
}));

describe("PlaylistsCollectionView.svelte - Decades Auto Playlists", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    navigationStore.selectedPlaylistId = null;
    navigationStore.selectedAutoPlaylist = null;
    navigationStore.playlistsSubTab = "auto";
  });

  it("renders decade auto-playlist cards in the auto playlist grid", async () => {
    playlistsStore.playlists = [
      {
        id: 1,
        name: "1980s",
        dynamic_enabled: true,
        is_queue: false,
        dynamic_spec: "decade:1980s",
        track_count: 12,
        created: 1700000000,
        updated: 1700000000,
      },
      {
        id: 2,
        name: "Rock",
        dynamic_enabled: true,
        is_queue: false,
        dynamic_spec: "tag:Rock",
        track_count: 8,
        created: 1700000000,
        updated: 1700000000,
      },
    ];

    const { getByText } = render(PlaylistsCollectionView);
    expect(getByText("1980s")).toBeInTheDocument();
    expect(getByText("Rock")).toBeInTheDocument();
  });

  it("refreshes auto-playlists and Smart Playlists when the refresh-all button is clicked", async () => {
    playlistsStore.playlists = [
      {
        id: 1,
        name: "1980s",
        dynamic_enabled: true,
        is_queue: false,
        dynamic_spec: "decade:1980s",
        track_count: 12,
        created: 1700000000,
        updated: 1700000000,
      },
      {
        id: 2,
        name: "Rock",
        dynamic_enabled: true,
        is_queue: false,
        dynamic_spec: "tag:Rock",
        track_count: 8,
        created: 1700000000,
        updated: 1700000000,
      },
    ];

    const { getByTitle } = render(PlaylistsCollectionView);
    const refreshButton = getByTitle("Refresh all auto-playlists and Smart Playlists with the latest songs from your library");

    await fireEvent.click(refreshButton);

    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith("sync_all_auto_playlists");
      expect(invoke).toHaveBeenCalledWith("refresh_all_auto_playlists");
    });
  });

  it("places Moment Mix (daypart) at the front before Favourite Songs in auto grid", async () => {
    playlistsStore.favouritesCount = 10;
    playlistsStore.playlists = [
      {
        id: 99,
        name: "Morning Mix",
        dynamic_enabled: true,
        is_queue: false,
        dynamic_spec: "daypart:morning:2026-09-23:",
        track_count: 20,
        created: 1700000000,
        updated: 1700000000,
      },
    ];

    const { getByText } = render(PlaylistsCollectionView);
    const morningMixEl = getByText("Morning Mix");
    const favSongsEl = getByText("Favourite Songs");

    expect(morningMixEl).toBeInTheDocument();
    expect(favSongsEl).toBeInTheDocument();
    expect(
      morningMixEl.compareDocumentPosition(favSongsEl) & Node.DOCUMENT_POSITION_FOLLOWING
    ).toBeTruthy();
  });

  it("opens a pin-able context menu when an auto-playlist card is right-clicked", async () => {
    playlistsStore.favouritesCount = 10;
    playlistsStore.playlists = [];
    const toggleSpy = vi.spyOn(pinnedStore, "toggle").mockResolvedValue();

    const { getByText } = render(PlaylistsCollectionView);
    await fireEvent.contextMenu(getByText("Favourite Songs"));
    await fireEvent.click(getByText("Pin to Home"));

    expect(toggleSpy).toHaveBeenCalledWith("auto_playlist", "favourites");
    toggleSpy.mockRestore();
  });
});

describe("PlaylistsCollectionView.svelte - Custom Playlists", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    navigationStore.selectedPlaylistId = null;
    navigationStore.selectedAutoPlaylist = null;
    navigationStore.playlistsSubTab = "custom";
  });

  it("opens a pin-able context menu when a playlist card is right-clicked", async () => {
    playlistsStore.playlists = [
      {
        id: 7,
        name: "Road Trip",
        dynamic_enabled: false,
        is_queue: false,
        track_count: 3,
        created: 1700000000,
        updated: 1700000000,
      },
    ];
    const toggleSpy = vi.spyOn(pinnedStore, "toggle").mockResolvedValue();

    const { getAllByText, getByText } = render(PlaylistsCollectionView);
    await fireEvent.contextMenu(getAllByText("Road Trip")[0]);
    await fireEvent.click(getByText("Pin to Home"));

    expect(toggleSpy).toHaveBeenCalledWith("playlist", "7");
    toggleSpy.mockRestore();
  });
});
