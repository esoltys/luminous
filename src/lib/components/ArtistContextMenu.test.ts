import "@testing-library/jest-dom";
import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, fireEvent } from "@testing-library/svelte";
import ArtistContextMenu from "./ArtistContextMenu.svelte";
import { invoke } from "@tauri-apps/api/core";
import { pinnedStore } from "../stores/pinned.svelte";
import { statsExclusionsStore } from "../stores/statsExclusions.svelte";
import { picardStore } from "../stores/picard.svelte";

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

  it("offers the same library actions as the album menu", () => {
    const { getByText, queryByText } = render(ArtistContextMenu, {
      props: { x: 100, y: 100, artistName: "Radiohead", onClose: vi.fn() },
    });

    expect(getByText("Open in Picard")).toBeInTheDocument();
    expect(queryByText("Edit Artist Details")).not.toBeInTheDocument();
    expect(getByText("Share Card...")).toBeInTheDocument();
    expect(getByText("Don't Include in Stats")).toBeInTheDocument();
  });

  it("Open in Picard sends every song by the artist", async () => {
    picardStore.path = "C:/Picard/picard.exe";
    vi.mocked(invoke).mockImplementation(async (cmd: string) => {
      if (cmd === "get_songs_by_artist") return [{ id: 7 }, { id: 9 }];
      return undefined;
    });
    const onClose = vi.fn();
    const { getByText } = render(ArtistContextMenu, {
      props: { x: 100, y: 100, artistName: "Radiohead", onClose },
    });

    await fireEvent.click(getByText("Open in Picard"));
    await vi.waitFor(() => expect(onClose).toHaveBeenCalled());
    expect(invoke).toHaveBeenCalledWith("get_songs_by_artist", { artist: "Radiohead" });
    expect(invoke).toHaveBeenCalledWith("open_in_picard", { songIds: [7, 9] });
  });

  it("toggles the artist's stats exclusion", async () => {
    let excluded: [string, string][] = [];
    vi.mocked(invoke).mockImplementation(async (cmd: string, args?: unknown) => {
      const a = args as { entityType: string; entityKey: string; excluded: boolean } | undefined;
      if (cmd === "set_stats_excluded" && a) {
        excluded = a.excluded ? [[a.entityType, a.entityKey]] : [];
      }
      if (cmd === "get_stats_exclusions") return excluded;
      return undefined;
    });
    await statsExclusionsStore.refresh();

    const { getByText } = render(ArtistContextMenu, {
      props: { x: 100, y: 100, artistName: "Radiohead", onClose: vi.fn() },
    });

    await fireEvent.click(getByText("Don't Include in Stats"));
    await vi.waitFor(() => expect(statsExclusionsStore.isExcluded("artist", "Radiohead")).toBe(true));
  });

  it("Share Card hides the menu without closing, so the share dialog can open", async () => {
    vi.mocked(invoke).mockResolvedValue([]);
    const onClose = vi.fn();
    const { getByText, queryByText } = render(ArtistContextMenu, {
      props: { x: 100, y: 100, artistName: "Radiohead", onClose },
    });

    await fireEvent.click(getByText("Share Card..."));
    expect(queryByText("Play Artist")).not.toBeInTheDocument();
    expect(onClose).not.toHaveBeenCalled();
  });
});
