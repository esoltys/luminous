import "@testing-library/jest-dom";
import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, fireEvent } from "@testing-library/svelte";
import AutoPlaylistContextMenu from "./AutoPlaylistContextMenu.svelte";
import { pinnedStore } from "../stores/pinned.svelte";
import type { AutoPlaylistItem } from "../types";

describe("AutoPlaylistContextMenu.svelte", () => {
  const mockAutoPlaylist: AutoPlaylistItem = {
    kind: "favourites",
    trackCount: 25,
  };

  beforeEach(() => {
    vi.clearAllMocks();
  });

  it("renders auto-playlist label and actions", () => {
    const { getByText } = render(AutoPlaylistContextMenu, {
      props: {
        x: 100,
        y: 100,
        autoPlaylist: mockAutoPlaylist,
        label: "Favourites",
        onClose: vi.fn(),
      },
    });

    expect(getByText("Favourites")).toBeInTheDocument();
    expect(getByText("Play Playlist")).toBeInTheDocument();
    expect(getByText("Add to Queue")).toBeInTheDocument();
    expect(getByText("Pin to Home")).toBeInTheDocument();
  });

  it("calls onPlay and onClose when Play Playlist is clicked", async () => {
    const onPlay = vi.fn();
    const onClose = vi.fn();
    const { getByText } = render(AutoPlaylistContextMenu, {
      props: {
        x: 100,
        y: 100,
        autoPlaylist: mockAutoPlaylist,
        label: "Favourites",
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
    const { getByText } = render(AutoPlaylistContextMenu, {
      props: {
        x: 100,
        y: 100,
        autoPlaylist: mockAutoPlaylist,
        label: "Favourites",
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

    const { getByText } = render(AutoPlaylistContextMenu, {
      props: {
        x: 100,
        y: 100,
        autoPlaylist: mockAutoPlaylist,
        label: "Favourites",
        onClose,
      },
    });

    const unpinBtn = getByText("Unpin from Home");
    expect(unpinBtn).toBeInTheDocument();

    await fireEvent.click(unpinBtn);
    expect(toggleSpy).toHaveBeenCalledWith("auto_playlist", "favourites");
    expect(onClose).toHaveBeenCalled();
  });
});
