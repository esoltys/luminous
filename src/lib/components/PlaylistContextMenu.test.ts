import "@testing-library/jest-dom";
import { describe, it, expect, beforeEach, vi } from "vitest";
import { render, screen, fireEvent, waitFor } from "@testing-library/svelte";
import { invoke } from "@tauri-apps/api/core";
import { navigationStore } from "../stores/navigation.svelte";
import PlaylistContextMenu from "./PlaylistContextMenu.svelte";
import { picardStore } from "../stores/picard.svelte";

describe("PlaylistContextMenu.svelte", () => {
  beforeEach(() => {
    picardStore.path = "/usr/bin/picard";
  });

  it("enables Open in Picard by default", async () => {
    render(PlaylistContextMenu, {
      x: 0,
      y: 0,
      selectedCount: 1,
      onPlay: () => {},
      onRemove: () => {},
      onOpenInPicard: () => {},
      onClose: () => {},
    });

    const item = await screen.findByText("Open in Picard");
    expect(item.closest("button")).not.toBeDisabled();
  });

  it("disables Open in Picard when the selection is all remote", async () => {
    render(PlaylistContextMenu, {
      x: 0,
      y: 0,
      selectedCount: 1,
      onPlay: () => {},
      onRemove: () => {},
      onOpenInPicard: () => {},
      allSelectedRemote: true,
      onClose: () => {},
    });

    const item = await screen.findByText("Open in Picard");
    const button = item.closest("button");
    expect(button).toBeDisabled();
    expect(button).toHaveAttribute("title", "Open in Picard isn't available for songs on a remote server");
  });

  describe("View in Files", () => {
    const target = { source: { kind: "folder", id: 1 }, path: ["Katatonia", "2006"], selectedName: "11 The Itch.flac", exact: true };
    const props = { x: 0, y: 0, selectedCount: 1, songId: 7, onPlay: () => {}, onRemove: () => {}, onGoToAlbum: () => {} };

    it("opens the entry's song in Files, right after Go to Album", async () => {
      vi.mocked(invoke).mockImplementation(async (cmd: string) => (cmd === "locate_song_in_files" ? target : null));
      const openInFiles = vi.spyOn(navigationStore, "openInFiles").mockImplementation(() => {});
      const onClose = vi.fn();
      render(PlaylistContextMenu, { ...props, onClose });

      const item = await screen.findByText("View in Files");
      const labels = screen.getAllByRole("menuitem").map((m) => m.textContent?.trim());
      expect(labels[labels.indexOf("Go to Album") + 1]).toBe("View in Files");

      await fireEvent.click(item);
      expect(invoke).toHaveBeenCalledWith("locate_song_in_files", { songId: 7 });
      expect(openInFiles).toHaveBeenCalledWith(target);
      expect(onClose).toHaveBeenCalled();
      openInFiles.mockRestore();
    });

    it("is hidden when the song has no place in Files", async () => {
      vi.mocked(invoke).mockImplementation(async () => null);
      render(PlaylistContextMenu, { ...props, onClose: () => {} });
      await waitFor(() => expect(invoke).toHaveBeenCalledWith("locate_song_in_files", { songId: 7 }));
      expect(screen.queryByText("View in Files")).not.toBeInTheDocument();
    });
  });
});
