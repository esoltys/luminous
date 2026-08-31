import "@testing-library/jest-dom";
import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, fireEvent, waitFor } from "@testing-library/svelte";
import AlbumTagEditor from "./AlbumTagEditor.svelte";
import { invoke } from "@tauri-apps/api/core";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
}));

describe("AlbumTagEditor.svelte", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.mocked(invoke).mockImplementation(async (cmd: string) => {
      if (cmd === "save_album_tags") return 2;
      if (cmd === "clear_album_cover_art") return 2;
      if (cmd === "get_library_snapshot") return { songs: [], albums: [], artists: [] };
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
      initialDisc: 1,
      onClose,
    });

    expect(getByText("Applies to 2 songs")).toBeInTheDocument();

    const albumInput = getByLabelText("Album Title") as HTMLInputElement;
    const yearInput = getByLabelText("Release Year") as HTMLInputElement;
    const discInput = getByLabelText("Disc #") as HTMLInputElement;

    expect(albumInput.value).toBe("Test Album");
    // Album Artist and Genre are chip inputs — the initial values render as
    // chips, not as the value of the (empty, ready-for-new-input) labeled
    // text field.
    expect(getByText("Test Artist")).toBeInTheDocument();
    expect(getByText("Pop")).toBeInTheDocument();
    expect(yearInput.value).toBe("2024");
    expect(discInput.value).toBe("1");
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
      initialDisc: 2,
      onClose,
      onSave,
    });

    const albumInput = getByLabelText("Album Title") as HTMLInputElement;
    await fireEvent.input(albumInput, { target: { value: "New Album Title" } });

    const saveBtn = getByRole("button", { name: /save tags/i });
    await fireEvent.click(saveBtn);

    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith("save_album_tags", expect.objectContaining({
        songIds: [101, 102],
        album: "New Album Title",
        albumArtist: "Old Artist",
        genre: "Rock",
        year: 2020,
        disc: 2,
        compilation: false,
      }));
      expect(onSave).toHaveBeenCalled();
      expect(onClose).toHaveBeenCalled();
    });
  });

  it("handles null initial genre and saves empty string for genre", async () => {
    const onClose = vi.fn();
    const onSave = vi.fn();
    const { getByLabelText, getByRole } = render(AlbumTagEditor, {
      songIds: [101],
      initialAlbum: "Test Album",
      initialAlbumArtist: "Test Artist",
      initialGenre: null,
      initialYear: 2024,
      initialDisc: null,
      onClose,
      onSave,
    });

    // No initial genre means no chips render, and the chip input's own
    // text field (the one "Genre" labels) starts empty and ready for input.
    const genreInput = getByLabelText("Genre") as HTMLInputElement;
    expect(genreInput.value).toBe("");

    const saveBtn = getByRole("button", { name: /save tags/i });
    await fireEvent.click(saveBtn);

    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith("save_album_tags", expect.objectContaining({
        songIds: [101],
        album: "Test Album",
        albumArtist: "Test Artist",
        genre: "",
        year: 2024,
        disc: null,
        compilation: false,
      }));
      expect(onSave).toHaveBeenCalled();
      expect(onClose).toHaveBeenCalled();
    });
  });

  it("adds a genre chip alongside the existing one and saves both, delimited", async () => {
    const onClose = vi.fn();
    const onSave = vi.fn();
    const { getByLabelText, getByRole, getByText } = render(AlbumTagEditor, {
      songIds: [101],
      initialAlbum: "Test Album",
      initialAlbumArtist: "Test Artist",
      initialGenre: "Rock",
      initialYear: 2024,
      initialDisc: 1,
      onClose,
      onSave,
    });

    expect(getByText("Rock")).toBeInTheDocument();

    const genreInput = getByLabelText("Genre") as HTMLInputElement;
    await fireEvent.input(genreInput, { target: { value: "Jazz Fusion" } });
    await fireEvent.keyDown(genreInput, { key: "Enter" });

    expect(getByText("Jazz Fusion")).toBeInTheDocument();
    // The draft field clears once its content becomes a committed chip.
    expect(genreInput.value).toBe("");

    const saveBtn = getByRole("button", { name: /save tags/i });
    await fireEvent.click(saveBtn);

    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith("save_album_tags", expect.objectContaining({
        songIds: [101],
        album: "Test Album",
        albumArtist: "Test Artist",
        genre: "Rock; Jazz Fusion",
        year: 2024,
        disc: 1,
        compilation: false,
      }));
      expect(onSave).toHaveBeenCalled();
      expect(onClose).toHaveBeenCalled();
    });
  });

  it("removes a genre chip via its remove button", async () => {
    const onClose = vi.fn();
    const onSave = vi.fn();
    const { getByRole, getByText, queryByText } = render(AlbumTagEditor, {
      songIds: [101],
      initialAlbum: "Test Album",
      initialAlbumArtist: "Test Artist",
      initialGenre: "Rock; Blues",
      initialYear: 2024,
      initialDisc: 1,
      onClose,
      onSave,
    });

    expect(getByText("Rock")).toBeInTheDocument();
    expect(getByText("Blues")).toBeInTheDocument();

    await fireEvent.click(getByRole("button", { name: "Remove Rock" }));
    expect(queryByText("Rock")).not.toBeInTheDocument();
    expect(getByText("Blues")).toBeInTheDocument();

    const saveBtn = getByRole("button", { name: /save tags/i });
    await fireEvent.click(saveBtn);

    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith("save_album_tags", expect.objectContaining({
        songIds: [101],
        album: "Test Album",
        albumArtist: "Test Artist",
        genre: "Blues",
        year: 2024,
        disc: 1,
        compilation: false,
      }));
      expect(onSave).toHaveBeenCalled();
      expect(onClose).toHaveBeenCalled();
    });
  });

  it("adds a chip to the Album Artist field and saves it delimited alongside the original", async () => {
    const onClose = vi.fn();
    const onSave = vi.fn();
    const { getByLabelText, getByRole, getByText } = render(AlbumTagEditor, {
      songIds: [101],
      initialAlbum: "Test Album",
      initialAlbumArtist: "Artist A",
      initialGenre: "Rock",
      initialYear: 2024,
      initialDisc: 1,
      onClose,
      onSave,
    });

    expect(getByText("Artist A")).toBeInTheDocument();

    const albumArtistInput = getByLabelText("Album Artist") as HTMLInputElement;
    await fireEvent.input(albumArtistInput, { target: { value: "Artist B" } });
    await fireEvent.keyDown(albumArtistInput, { key: "Enter" });

    expect(getByText("Artist B")).toBeInTheDocument();

    const saveBtn = getByRole("button", { name: /save tags/i });
    await fireEvent.click(saveBtn);

    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith("save_album_tags", expect.objectContaining({
        songIds: [101],
        album: "Test Album",
        albumArtist: "Artist A; Artist B",
        genre: "Rock",
        year: 2024,
        disc: 1,
        compilation: false,
      }));
      expect(onSave).toHaveBeenCalled();
      expect(onClose).toHaveBeenCalled();
    });
  });

  it("allows clearing the Disc input field to save null disc", async () => {
    const onClose = vi.fn();
    const onSave = vi.fn();
    const { getByLabelText, getByRole } = render(AlbumTagEditor, {
      songIds: [101],
      initialAlbum: "Test Album",
      initialAlbumArtist: "Test Artist",
      initialGenre: "Pop",
      initialYear: 2024,
      initialDisc: 1,
      onClose,
      onSave,
    });

    const discInput = getByLabelText("Disc #") as HTMLInputElement;
    expect(discInput.value).toBe("1");

    await fireEvent.input(discInput, { target: { value: "" } });

    const saveBtn = getByRole("button", { name: /save tags/i });
    await fireEvent.click(saveBtn);

    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith("save_album_tags", expect.objectContaining({
        songIds: [101],
        album: "Test Album",
        albumArtist: "Test Artist",
        genre: "Pop",
        year: 2024,
        disc: null,
        compilation: false,
      }));
      expect(onSave).toHaveBeenCalled();
      expect(onClose).toHaveBeenCalled();
    });
  });

  it("toggling Compilation on replaces the Album Artist input with a Various Artists pill and saves both", async () => {
    const onClose = vi.fn();
    const onSave = vi.fn();
    const { getByLabelText, getByRole, getByText, queryByLabelText } = render(AlbumTagEditor, {
      songIds: [101],
      initialAlbum: "Now That's What I Call Tests",
      initialAlbumArtist: "Artist A",
      initialGenre: "Pop",
      initialYear: 2024,
      initialDisc: 1,
      initialCompilation: false,
      onClose,
      onSave,
    });

    expect(getByLabelText("Album Artist")).toBeInTheDocument();

    const compilationCheckbox = getByLabelText(/compilation/i) as HTMLInputElement;
    expect(compilationCheckbox.checked).toBe(false);
    await fireEvent.click(compilationCheckbox);
    expect(compilationCheckbox.checked).toBe(true);

    // The editable input is gone; a static "Various Artists" pill takes its place.
    expect(queryByLabelText("Album Artist")).not.toBeInTheDocument();
    expect(getByText("Various Artists")).toBeInTheDocument();

    const saveBtn = getByRole("button", { name: /save tags/i });
    await fireEvent.click(saveBtn);

    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith("save_album_tags", expect.objectContaining({
        songIds: [101],
        album: "Now That's What I Call Tests",
        albumArtist: "Various Artists",
        genre: "Pop",
        year: 2024,
        disc: 1,
        compilation: true,
      }));
      expect(onSave).toHaveBeenCalled();
      expect(onClose).toHaveBeenCalled();
    });
  });

  it("shows the real Album Artist as an editable field for a compilation-flagged album whose artist isn't literally Various Artists", async () => {
    const onClose = vi.fn();
    const { getByLabelText, getByText, queryByText } = render(AlbumTagEditor, {
      songIds: [101],
      initialAlbum: "Greatest Hits",
      initialAlbumArtist: "Artist A",
      initialCompilation: true,
      onClose,
    });

    expect(getByLabelText("Album Artist")).toBeInTheDocument();
    expect(getByText("Artist A")).toBeInTheDocument();
    expect(queryByText("Various Artists")).not.toBeInTheDocument();
  });

  it("disables Clear Embedded Artwork when no track in the album has embedded art", async () => {
    const onClose = vi.fn();
    const { getByRole } = render(AlbumTagEditor, {
      songIds: [101, 102],
      hasEmbeddedArt: false,
      onClose,
    });

    expect(getByRole("button", { name: /clear embedded artwork/i })).toBeDisabled();
  });

  it("clears embedded artwork for the whole album after confirming", async () => {
    const onClose = vi.fn();
    const onSave = vi.fn();
    const { getByRole, getAllByRole } = render(AlbumTagEditor, {
      songIds: [101, 102],
      hasEmbeddedArt: true,
      onClose,
      onSave,
    });

    const clearBtn = getByRole("button", { name: /clear embedded artwork/i });
    expect(clearBtn).not.toBeDisabled();
    await fireEvent.click(clearBtn);

    const confirmBtn = await waitFor(() => {
      const btns = getAllByRole("button", { name: /clear embedded artwork/i });
      expect(btns).toHaveLength(2);
      return btns[btns.length - 1];
    });
    await fireEvent.click(confirmBtn);

    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith("clear_album_cover_art", { songIds: [101, 102] });
      expect(onSave).toHaveBeenCalled();
      expect(onClose).toHaveBeenCalled();
    });
  });

  it("unchecking Compilation restores the previous Album Artist value", async () => {
    const onClose = vi.fn();
    const { getByLabelText, getByText, queryByLabelText } = render(AlbumTagEditor, {
      songIds: [101],
      initialAlbum: "Test Album",
      initialAlbumArtist: "Artist A",
      onClose,
    });

    const compilationCheckbox = getByLabelText(/compilation/i) as HTMLInputElement;
    await fireEvent.click(compilationCheckbox);
    expect(queryByLabelText("Album Artist")).not.toBeInTheDocument();

    await fireEvent.click(compilationCheckbox);
    expect(getByLabelText("Album Artist")).toBeInTheDocument();
    expect(getByText("Artist A")).toBeInTheDocument();
  });
});
