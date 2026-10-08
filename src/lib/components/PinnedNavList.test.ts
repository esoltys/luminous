import "@testing-library/jest-dom";
import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, fireEvent, screen } from "@testing-library/svelte";
import PinnedNavList from "./PinnedNavList.svelte";
import { pinnedStore } from "../stores/pinned.svelte";
import { i18n } from "../stores/i18n.svelte";
import type { PinnedItem } from "../types";

describe("PinnedNavList.svelte", () => {
  const mockAlbumItem: PinnedItem = {
    type: "album",
    album: {
      album: "OK Computer",
      artist: "Radiohead",
      year: 1997,
      track_count: 12,
      art_embedded: false,
      art_automatic: null,
      art_manual: null,
      rating: 5,
    } as any,
  };

  const mockSongItem: PinnedItem = {
    type: "song",
    song: {
      id: 101,
      title: "Paranoid Android",
      artist: "Radiohead",
      art_embedded: false,
    } as any,
  };

  beforeEach(() => {
    vi.clearAllMocks();
    i18n.currentLocale = "en-CA";
    pinnedStore.items = [mockAlbumItem, mockSongItem];
  });

  it("renders pinned items in expanded mode with section header", () => {
    const { getByText } = render(PinnedNavList, {
      props: { collapsed: false },
    });

    expect(getByText("Pinned")).toBeInTheDocument();
    expect(getByText("OK Computer")).toBeInTheDocument();
    expect(getByText("Paranoid Android")).toBeInTheDocument();
  });

  it("renders pinned items in collapsed mode without section header text", () => {
    const { queryByText, getAllByRole } = render(PinnedNavList, {
      props: { collapsed: true },
    });

    expect(queryByText("Pinned")).not.toBeInTheDocument();
    expect(queryByText("OK Computer")).not.toBeInTheDocument();
    const buttons = getAllByRole("button");
    expect(buttons).toHaveLength(2);
    expect(buttons[0]).toHaveAttribute("title", "OK Computer • Radiohead");
    expect(buttons[1]).toHaveAttribute("title", "Paranoid Android • Radiohead");
  });

  it("does not render when pinnedStore has no items", () => {
    pinnedStore.items = [];
    const { container, queryByText } = render(PinnedNavList, {
      props: { collapsed: false },
    });

    expect(queryByText("Pinned")).not.toBeInTheDocument();
    expect(container.querySelector("[data-pinned-nav-index]")).toBeNull();
  });

  it("allows reordering via Alt+ArrowUp and Alt+ArrowDown keyboard shortcuts", async () => {
    const reorderSpy = vi.spyOn(pinnedStore, "reorderVisible").mockResolvedValue();
    const { container } = render(PinnedNavList, {
      props: { collapsed: false },
    });

    const rows = container.querySelectorAll("[data-pinned-nav-index]");
    expect(rows).toHaveLength(2);

    // Press Alt+ArrowDown on first row
    await fireEvent.keyDown(rows[0], { key: "ArrowDown", altKey: true });
    expect(reorderSpy).toHaveBeenCalledWith(0, 1);

    // Press Alt+ArrowUp on second row
    await fireEvent.keyDown(rows[1], { key: "ArrowUp", altKey: true });
    expect(reorderSpy).toHaveBeenCalledWith(1, 0);
  });

  it("opens context menu on right click", async () => {
    vi.spyOn(pinnedStore, "isPinned").mockReturnValue(true);
    const { getByText } = render(PinnedNavList, {
      props: { collapsed: false },
    });

    const albumEl = getByText("OK Computer");
    await fireEvent.contextMenu(albumEl);

    expect(await screen.findByText("Play Album")).toBeInTheDocument();
    expect(await screen.findByText("Unpin from Home")).toBeInTheDocument();
  });
});
