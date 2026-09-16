import "@testing-library/jest-dom";
import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, fireEvent } from "@testing-library/svelte";
import ArtistContextMenu from "./ArtistContextMenu.svelte";
import { pinnedStore } from "../stores/pinned.svelte";

describe("ArtistContextMenu.svelte", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it("renders artist name header and actions", () => {
    const { getByText } = render(ArtistContextMenu, {
      props: {
        x: 100,
        y: 100,
        artistName: "Radiohead",
        onClose: vi.fn(),
      },
    });

    expect(getByText("Radiohead")).toBeInTheDocument();
    expect(getByText("Play Artist")).toBeInTheDocument();
    expect(getByText("Add to Queue")).toBeInTheDocument();
    expect(getByText("Pin to Home")).toBeInTheDocument();
  });

  it("calls onPlay and onClose when Play Artist is clicked", async () => {
    const onPlay = vi.fn();
    const onClose = vi.fn();
    const { getByText } = render(ArtistContextMenu, {
      props: {
        x: 100,
        y: 100,
        artistName: "Radiohead",
        onPlay,
        onClose,
      },
    });

    await fireEvent.click(getByText("Play Artist"));
    expect(onPlay).toHaveBeenCalled();
    expect(onClose).toHaveBeenCalled();
  });

  it("calls onAddToQueue and onClose when Add to Queue is clicked", async () => {
    const onAddToQueue = vi.fn();
    const onClose = vi.fn();
    const { getByText } = render(ArtistContextMenu, {
      props: {
        x: 100,
        y: 100,
        artistName: "Radiohead",
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

    const { getByText } = render(ArtistContextMenu, {
      props: {
        x: 100,
        y: 100,
        artistName: "Radiohead",
        onClose,
      },
    });

    const unpinBtn = getByText("Unpin from Home");
    expect(unpinBtn).toBeInTheDocument();

    await fireEvent.click(unpinBtn);
    expect(toggleSpy).toHaveBeenCalledWith("artist", "Radiohead");
    expect(onClose).toHaveBeenCalled();
  });
});
