import "@testing-library/jest-dom";
import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, fireEvent, waitFor } from "@testing-library/svelte";
import AlbumTagEditor from "./AlbumTagEditor.svelte";
import { invoke } from "@tauri-apps/api/core";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
}));

vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn().mockResolvedValue(() => {}),
}));

if (typeof Element !== "undefined" && !Element.prototype.animate) {
  Element.prototype.animate = vi.fn().mockReturnValue({
    finished: Promise.resolve(),
    cancel: () => {},
  }) as any;
}

describe("AlbumTagEditor.svelte", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.mocked(invoke).mockImplementation(async (cmd: string) => {
      if (cmd === "save_album_tags") return 2;
      return null;
    });
  });

  it("renders with initial album fields", async () => {
    const onClose = vi.fn();
    const { getByLabelText, getByText } = render(AlbumTagEditor, {
      songIds: [101, 102],
      initialAlbum: "Test Album",
      initialAlbumArtist: "Test Artist",
      initialGenre: "Pop",
      initialYear: 2024,
      onClose,
    });

    expect(getByText("Applies to 2 tracks")).toBeInTheDocument();

    const albumInput = getByLabelText("Album Title") as HTMLInputElement;
    const artistInput = getByLabelText("Album Artist") as HTMLInputElement;
    const genreInput = getByLabelText("Genre") as HTMLInputElement;
    const yearInput = getByLabelText("Release Year") as HTMLInputElement;

    expect(albumInput.value).toBe("Test Album");
    expect(artistInput.value).toBe("Test Artist");
    expect(genreInput.value).toBe("Pop");
    expect(yearInput.value).toBe("2024");
  });

  it("calls onClose when cancel button is clicked", async () => {
    const onClose = vi.fn();
    const { getByRole } = render(AlbumTagEditor, {
      songIds: [101, 102],
      onClose,
    });

    const cancelBtn = getByRole("button", { name: "Cancel" });
    await fireEvent.click(cancelBtn);
    expect(onClose).toHaveBeenCalled();
  });

  it("invokes save_album_tags when Save Tags is clicked", async () => {
    const onClose = vi.fn();
    const onSave = vi.fn();
    const { getByLabelText, getByRole } = render(AlbumTagEditor, {
      songIds: [101, 102],
      initialAlbum: "Old Album",
      initialAlbumArtist: "Old Artist",
      initialGenre: "Rock",
      initialYear: 2020,
      onClose,
      onSave,
    });

    const albumInput = getByLabelText("Album Title") as HTMLInputElement;
    await fireEvent.input(albumInput, { target: { value: "New Album Title" } });

    const saveBtn = getByRole("button", { name: /save tags/i });
    await fireEvent.click(saveBtn);

    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith("save_album_tags", {
        songIds: [101, 102],
        album: "New Album Title",
        albumArtist: "Old Artist",
        genre: "Rock",
        year: 2020,
      });
      expect(onSave).toHaveBeenCalled();
      expect(onClose).toHaveBeenCalled();
    });
  });
});
