import "@testing-library/jest-dom";
import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, fireEvent } from "@testing-library/svelte";
import PlaylistCardContextMenu from "./PlaylistCardContextMenu.svelte";
import { pinnedStore } from "../stores/pinned.svelte";
import type { Playlist } from "../types";

describe("PlaylistCardContextMenu.svelte", () => {
  const mockPlaylist: Playlist = {
    id: 12,
    name: "Summer Vibes",
    dynamic_enabled: false,
    is_queue: false,
    created: 0,
    updated: 0,
    track_count: 8,
  };

  beforeEach(() => {
    vi.clearAllMocks();
  });

  it("renders playlist title and actions", () => {
    const { getByText } = render(PlaylistCardContextMenu, {
      props: {
        x: 100,
        y: 100,
        playlist: mockPlaylist,
        onClose: vi.fn(),
      },
    });

    expect(getByText("Summer Vibes")).toBeInTheDocument();
    expect(getByText("Play Playlist")).toBeInTheDocument();
    expect(getByText("Add to Queue")).toBeInTheDocument();
    expect(getByText("Pin to Home")).toBeInTheDocument();
  });

  it("calls onPlay and onClose when Play Playlist is clicked", async () => {
    const onPlay = vi.fn();
    const onClose = vi.fn();
    const { getByText } = render(PlaylistCardContextMenu, {
      props: {
        x: 100,
        y: 100,
        playlist: mockPlaylist,
        onPlay,
        onClose,
      },
    });

    await fireEvent.click(getByText("Play Playlist"));
    expect(onPlay).toHaveBeenCalled();
    expect(onClose).toHaveBeenCalled();
  });

  it("calls onAddToQueue and onClose when Add to Queue is clicked", async () => {
    const onAddToQueue = vi.fn();
    const onClose = vi.fn();
    const { getByText } = render(PlaylistCardContextMenu, {
      props: {
        x: 100,
        y: 100,
        playlist: mockPlaylist,
        onAddToQueue,
        onClose,
      },
    });

    await fireEvent.click(getByText("Add to Queue"));
    expect(onAddToQueue).toHaveBeenCalled();
    expect(onClose).toHaveBeenCalled();
  });

  it("shows Unpin from Home when already pinned and calls pinnedStore.toggle", async () => {
    vi.spyOn(pinnedStore, "isPinned").mockReturnValue(true);
    const toggleSpy = vi.spyOn(pinnedStore, "toggle").mockResolvedValue();
    const onClose = vi.fn();

    const { getByText } = render(PlaylistCardContextMenu, {
      props: {
        x: 100,
        y: 100,
        playlist: mockPlaylist,
        onClose,
      },
    });

    const unpinBtn = getByText("Unpin from Home");
    expect(unpinBtn).toBeInTheDocument();

    await fireEvent.click(unpinBtn);
    expect(toggleSpy).toHaveBeenCalledWith("playlist", "12");
    expect(onClose).toHaveBeenCalled();
  });

  it("does not render pin button when playlist is the queue", () => {
    const queuePlaylist: Playlist = { ...mockPlaylist, is_queue: true };
    const { queryByText } = render(PlaylistCardContextMenu, {
      props: {
        x: 100,
        y: 100,
        playlist: queuePlaylist,
        onClose: vi.fn(),
      },
    });

    expect(queryByText("Pin to Home")).toBeNull();
    expect(queryByText("Unpin from Home")).toBeNull();
  });
});
