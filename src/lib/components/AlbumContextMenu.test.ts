import "@testing-library/jest-dom";
import { describe, it, expect, vi } from "vitest";
import { render, screen, fireEvent, waitFor } from "@testing-library/svelte";
import { invoke } from "@tauri-apps/api/core";
import AlbumContextMenu from "./AlbumContextMenu.svelte";
import { navigationStore } from "../stores/navigation.svelte";

describe("AlbumContextMenu.svelte - View in Files", () => {
  const target = {
    source: { kind: "folder", id: 1 },
    path: ["Katatonia", "2006 - The Great Cold Distance"],
    selectedName: null,
    exact: true,
  };

  function locateReturns(result: unknown) {
    vi.mocked(invoke).mockImplementation(async (cmd: string) =>
      cmd === "locate_album_in_files" ? result : null,
    );
  }

  it("opens the album's folder in Files, looked up by title", async () => {
    locateReturns(target);
    const openInFiles = vi.spyOn(navigationStore, "openInFiles").mockImplementation(() => {});
    const onClose = vi.fn();
    render(AlbumContextMenu, {
      x: 0,
      y: 0,
      albumName: "The Great Cold Distance",
      artistName: "Katatonia",
      onPlay: () => {},
      onClose,
    });

    await fireEvent.click(await screen.findByText("View in Files"));
    expect(invoke).toHaveBeenCalledWith("locate_album_in_files", {
      album: "The Great Cold Distance",
    });
    expect(openInFiles).toHaveBeenCalledWith(target);
    expect(onClose).toHaveBeenCalled();
    openInFiles.mockRestore();
  });

  it("is hidden for an album with no place in Files", async () => {
    locateReturns(null);
    render(AlbumContextMenu, {
      x: 0,
      y: 0,
      albumName: "Live Radio",
      onPlay: () => {},
      onClose: () => {},
    });
    await waitFor(() =>
      expect(invoke).toHaveBeenCalledWith("locate_album_in_files", { album: "Live Radio" }),
    );
    expect(screen.queryByText("View in Files")).not.toBeInTheDocument();
  });
});

describe("AlbumContextMenu.svelte - rating", () => {
  const baseProps = { x: 0, y: 0, albumName: "Fake Nudes", onPlay: () => {}, onClose: () => {} };

  it("rates the album through set_album_rating and keeps the menu open", async () => {
    vi.mocked(invoke).mockImplementation(async (cmd: string) =>
      cmd === "set_album_rating" ? 4 : null,
    );
    const onClose = vi.fn();
    render(AlbumContextMenu, { ...baseProps, rating: -1, onClose });
    expect(await screen.findByText("Rating")).toBeInTheDocument();

    await fireEvent.click(await screen.findByLabelText("Rate 4 of 5"));

    expect(invoke).toHaveBeenCalledWith("set_album_rating", { album: "Fake Nudes", rating: 4 });
    expect(onClose).not.toHaveBeenCalled();
    // The row now shows the normalized rating, so the clear button becomes available.
    await waitFor(() => expect(screen.getByLabelText("Clear rating")).not.toBeDisabled());
  });

  it("clears an existing rating", async () => {
    vi.mocked(invoke).mockImplementation(async (cmd: string) =>
      cmd === "set_album_rating" ? -1 : null,
    );
    render(AlbumContextMenu, { ...baseProps, rating: 3 });

    await fireEvent.click(await screen.findByLabelText("Clear rating"));

    expect(invoke).toHaveBeenCalledWith("set_album_rating", { album: "Fake Nudes", rating: -1 });
  });

  it("leaves the rating row out when the host passes no rating", async () => {
    render(AlbumContextMenu, baseProps);
    await screen.findByText("Fake Nudes");
    expect(screen.queryByLabelText("Rate 4 of 5")).not.toBeInTheDocument();
  });
});
