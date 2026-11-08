import "@testing-library/jest-dom";
import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, fireEvent } from "@testing-library/svelte";
import AddToPlaylistSubmenu from "./AddToPlaylistSubmenu.svelte";
import { playlistsStore } from "../stores/playlists.svelte";
import type { Playlist } from "../types";

const playlistA: Playlist = {
  id: 10,
  name: "Metal Favourites",
  is_queue: false,
  dynamic_enabled: false,
  track_count: 5,
  created: 0,
  updated: 0,
};

const playlistB: Playlist = {
  id: 20,
  name: "Acoustic Mix",
  is_queue: false,
  dynamic_enabled: false,
  track_count: 3,
  created: 0,
  updated: 0,
};

describe("AddToPlaylistSubmenu.svelte", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    playlistsStore.playlists = [playlistA, playlistB];
  });

  it("renders trigger and opens submenu on hover", async () => {
    const onSelectPlaylist = vi.fn();
    const onCreatePlaylist = vi.fn();
    const onClose = vi.fn();

    const { getByText } = render(AddToPlaylistSubmenu, {
      props: {
        onSelectPlaylist,
        onCreatePlaylist,
        onClose,
      },
    });

    const trigger = getByText("Add to Playlist");
    expect(trigger).toBeInTheDocument();

    await fireEvent.click(trigger);
    expect(getByText("Metal Favourites")).toBeInTheDocument();
    expect(getByText("Acoustic Mix")).toBeInTheDocument();
    expect(getByText("New Playlist...")).toBeInTheDocument();
  });

  it("calls onSelectPlaylist and onClose when a playlist is clicked", async () => {
    const onSelectPlaylist = vi.fn();
    const onCreatePlaylist = vi.fn();
    const onClose = vi.fn();

    const { getByText } = render(AddToPlaylistSubmenu, {
      props: {
        onSelectPlaylist,
        onCreatePlaylist,
        onClose,
      },
    });

    const trigger = getByText("Add to Playlist");
    await fireEvent.click(trigger);

    await fireEvent.click(getByText("Metal Favourites"));
    expect(onSelectPlaylist).toHaveBeenCalledWith(playlistA);
    expect(onClose).toHaveBeenCalled();
  });

  it("opens modal and creates a new playlist when confirmed", async () => {
    const onSelectPlaylist = vi.fn();
    const onCreatePlaylist = vi.fn();
    const onClose = vi.fn();

    const { getByText, getByPlaceholderText } = render(AddToPlaylistSubmenu, {
      props: {
        onSelectPlaylist,
        onCreatePlaylist,
        onClose,
      },
    });

    const trigger = getByText("Add to Playlist");
    await fireEvent.click(trigger);

    await fireEvent.click(getByText("New Playlist..."));

    expect(getByText("New Playlist")).toBeInTheDocument();
    const input = getByPlaceholderText("Untitled Playlist");
    await fireEvent.input(input, { target: { value: "Roadtrip Tunes" } });

    await fireEvent.click(getByText("Create"));
    expect(onCreatePlaylist).toHaveBeenCalledWith("Roadtrip Tunes");
    expect(onClose).toHaveBeenCalled();
  });
});
