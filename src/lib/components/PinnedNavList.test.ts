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

  it("renders pinned items in expanded mode without extra section header", () => {
    const { getByText, queryByText } = render(PinnedNavList, {
      props: { collapsed: false },
    });

    expect(queryByText("Pinned")).not.toBeInTheDocument();
    expect(getByText("OK Computer")).toBeInTheDocument();
    expect(getByText("Paranoid Android")).toBeInTheDocument();
  });

  it("renders pinned items in collapsed mode without text", () => {
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

  it("reorders items via pointer drag gestures", async () => {
    const reorderSpy = vi.spyOn(pinnedStore, "reorderVisible").mockResolvedValue();
    const { container } = render(PinnedNavList, {
      props: { collapsed: false },
    });

    const rows = container.querySelectorAll<HTMLElement>("[data-pinned-nav-index]");
    expect(rows).toHaveLength(2);

    rows[0].setPointerCapture = vi.fn();

    // Start drag on row 0
    await fireEvent.pointerDown(rows[0], { clientX: 10, clientY: 10, button: 0, pointerId: 1 });
    expect(rows[0].setPointerCapture).toHaveBeenCalledWith(1);

    // Mock elementFromPoint to hit row 1
    const originalElementFromPoint = document.elementFromPoint;
    document.elementFromPoint = vi.fn().mockImplementation(() => rows[1]);

    try {
      // Move past 4px threshold
      await fireEvent.pointerMove(window, { clientX: 10, clientY: 50 });

      // Drop on row 1
      await fireEvent.pointerUp(window);
      expect(reorderSpy).toHaveBeenCalledWith(0, 1);
    } finally {
      document.elementFromPoint = originalElementFromPoint;
    }
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
