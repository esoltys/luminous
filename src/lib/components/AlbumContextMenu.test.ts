import "@testing-library/jest-dom";
import { describe, it, expect, vi } from "vitest";
import { render, screen, fireEvent, waitFor } from "@testing-library/svelte";
import { invoke } from "@tauri-apps/api/core";
import AlbumContextMenu from "./AlbumContextMenu.svelte";
import { navigationStore } from "../stores/navigation.svelte";

describe("AlbumContextMenu.svelte - View in Files", () => {
  const target = { source: { kind: "folder", id: 1 }, path: ["Katatonia", "2006 - The Great Cold Distance"], selectedName: null, exact: true };

  function locateReturns(result: unknown) {
    vi.mocked(invoke).mockImplementation(async (cmd: string) => (cmd === "locate_album_in_files" ? result : null));
  }

  it("opens the album's folder in Files, looked up by title", async () => {
    locateReturns(target);
    const openInFiles = vi.spyOn(navigationStore, "openInFiles").mockImplementation(() => {});
    const onClose = vi.fn();
    render(AlbumContextMenu, { x: 0, y: 0, albumName: "The Great Cold Distance", artistName: "Katatonia", onPlay: () => {}, onClose });

    await fireEvent.click(await screen.findByText("View in Files"));
    expect(invoke).toHaveBeenCalledWith("locate_album_in_files", { album: "The Great Cold Distance" });
    expect(openInFiles).toHaveBeenCalledWith(target);
    expect(onClose).toHaveBeenCalled();
    openInFiles.mockRestore();
  });

  it("is hidden for an album with no place in Files", async () => {
    locateReturns(null);
    render(AlbumContextMenu, { x: 0, y: 0, albumName: "Live Radio", onPlay: () => {}, onClose: () => {} });
    await waitFor(() => expect(invoke).toHaveBeenCalledWith("locate_album_in_files", { album: "Live Radio" }));
    expect(screen.queryByText("View in Files")).not.toBeInTheDocument();
  });
});
