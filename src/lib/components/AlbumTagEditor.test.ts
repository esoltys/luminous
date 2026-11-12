import "@testing-library/jest-dom";
import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, fireEvent, waitFor } from "@testing-library/svelte";
import AlbumTagEditor from "./AlbumTagEditor.svelte";
import { invoke } from "@tauri-apps/api/core";
import { collectionStore } from "../stores/collection.svelte";
import type { Song } from "../types";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
}));

const KEEP = { mode: "keep" };
const set = <T>(value: T) => ({ mode: "set", value });

/** A track as the editors receive it; the overrides are its tags. */
function track(overrides: Partial<Song> = {}): Song {
  return {
    album: "Test Album",
    albumsort: null,
    album_artist: "Test Artist",
    album_artist_sort: null,
    artist: "Test Artist",
    genre: "Pop",
    genresort: null,
    language: null,
    year: 2024,
    disc: 1,
    disc_subtitle: null,
    compilation: false,
    ...overrides,
  } as Song;
}

/** The `edit` sent with the last save_album_tags call. */
function savedEdit() {
  const call = vi.mocked(invoke).mock.calls.find(([cmd]) => cmd === "save_album_tags");
  return (call?.[1] as { edit: Record<string, unknown> }).edit;
}

describe("AlbumTagEditor.svelte", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    collectionStore.songs = [];
    vi.mocked(invoke).mockImplementation(async (cmd: string) => {
      if (cmd === "save_album_tags") return 2;
      if (cmd === "clear_album_cover_art") return 2;
      if (cmd === "get_library_snapshot") return { songs: [], albums: [], artists: [] };
      return null;
    });
  });

  it("renders with initial album fields", async () => {
    const { getByLabelText, getByText } = render(AlbumTagEditor, {
      songIds: [101, 102],
      initialTracks: [track(), track()],
      onClose: vi.fn(),
    });

    expect(getByText("Applies to 2 songs")).toBeInTheDocument();
    expect((getByLabelText("Album Title") as HTMLInputElement).value).toBe("Test Album");
    // Album Artist and Genre are chip inputs — the initial values render as
    // chips, not as the value of the (empty, ready-for-new-input) labeled
    // text field.
    expect(getByText("Test Artist")).toBeInTheDocument();
    expect(getByText("Pop")).toBeInTheDocument();
    expect((getByLabelText("Release Year") as HTMLInputElement).value).toBe("2024");
    expect((getByLabelText("Disc #") as HTMLInputElement).value).toBe("1");
  });

  it("shows a remote-source note when the album's sample track is a WebDAV song (#682)", async () => {
    collectionStore.songs = [
      { id: 101, path: "http://user:pass@127.0.0.1:8080/Music/song.mp3" } as any,
    ];
    const { findByText } = render(AlbumTagEditor, {
      songIds: [101, 102],
      onClose: vi.fn(),
    });
    expect(await findByText(/changes are saved in Luminous only/i)).toBeInTheDocument();
  });

  it("shows no remote-source note for a local album", async () => {
    collectionStore.songs = [{ id: 101, path: "/music/rock/song.flac" } as any];
    const { queryByText, getByText } = render(AlbumTagEditor, {
      songIds: [101, 102],
      onClose: vi.fn(),
    });
    await vi.waitFor(() => expect(getByText("Cancel")).toBeInTheDocument());
    expect(queryByText(/changes are saved in Luminous only/i)).not.toBeInTheDocument();
  });

  it("hides the Clear Embedded Artwork button for a WebDAV album even with embedded art (#682)", async () => {
    collectionStore.songs = [
      { id: 101, path: "http://user:pass@127.0.0.1:8080/Music/song.mp3" } as any,
    ];
    const { findByText, queryByRole } = render(AlbumTagEditor, {
      songIds: [101, 102],
      hasEmbeddedArt: true,
      onClose: vi.fn(),
    });
    await findByText(/changes are saved in Luminous only/i);
    expect(queryByRole("button", { name: /clear embedded artwork/i })).not.toBeInTheDocument();
  });

  it("calls onClose when cancel button is clicked", async () => {
    const onClose = vi.fn();
    const { getByRole } = render(AlbumTagEditor, {
      songIds: [101, 102],
      onClose,
    });

    await fireEvent.click(getByRole("button", { name: "Cancel" }));
    expect(onClose).toHaveBeenCalled();
  });

  it("sends only the edited field, keeping the rest per track", async () => {
    const onClose = vi.fn();
    const onSave = vi.fn();
    const { getByLabelText, getByRole } = render(AlbumTagEditor, {
      songIds: [101, 102],
      initialTracks: [track({ disc: 2 }), track({ disc: 2 })],
      onClose,
      onSave,
    });

    await fireEvent.input(getByLabelText("Album Title"), { target: { value: "New Album Title" } });
    await fireEvent.click(getByRole("button", { name: /save tags/i }));

    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith("save_album_tags", {
        songIds: [101, 102],
        edit: expect.objectContaining({
          album: set("New Album Title"),
          albumArtist: KEEP,
          genre: KEEP,
          year: KEEP,
          disc: KEEP,
          compilation: KEEP,
        }),
      });
      expect(onSave).toHaveBeenCalled();
      expect(onClose).toHaveBeenCalled();
    });
  });

  // A selection spanning albums must not overwrite every track's album,
  // genre and year with a blank "consensus" when only one field changed.
  it("shows Mixed for fields the tracks disagree on and keeps them unless edited", async () => {
    const { getByLabelText, getByRole } = render(AlbumTagEditor, {
      songIds: [101, 102],
      initialTracks: [
        track({ album: "Night Visions", genre: "Pop", year: 2012, compilation: false }),
        track({ album: "Evolve", genre: "Rock", year: 2017, compilation: true }),
      ],
      onClose: vi.fn(),
    });

    const albumInput = getByLabelText("Album Title") as HTMLInputElement;
    expect(albumInput.value).toBe("");
    expect(albumInput.placeholder).toBe("Mixed");
    expect((getByLabelText("Release Year") as HTMLInputElement).placeholder).toBe("Mixed");
    expect((getByLabelText("Genre") as HTMLInputElement).placeholder).toBe("Mixed");
    expect((getByLabelText(/compilation/i) as HTMLInputElement).indeterminate).toBe(true);

    await fireEvent.input(getByLabelText("Disc Subtitle"), { target: { value: "Remixes" } });
    await fireEvent.click(getByRole("button", { name: /save tags/i }));

    await waitFor(() => expect(invoke).toHaveBeenCalledWith("save_album_tags", expect.anything()));
    expect(savedEdit()).toEqual({
      album: KEEP,
      albumsort: KEEP,
      albumArtist: KEEP,
      albumArtistSort: KEEP,
      genre: KEEP,
      genresort: KEEP,
      language: KEEP,
      year: KEEP,
      disc: KEEP,
      discSubtitle: set("Remixes"),
      compilation: KEEP,
    });
  });

  it("shows no genre chips for tracks without a genre", async () => {
    const { getByLabelText } = render(AlbumTagEditor, {
      songIds: [101],
      initialTracks: [track({ genre: "" })],
      onClose: vi.fn(),
    });

    // No genre means no chips render, and the chip input's own text field
    // (the one "Genre" labels) starts empty and ready for input.
    const genreInput = getByLabelText("Genre") as HTMLInputElement;
    expect(genreInput.value).toBe("");
    expect(genreInput.placeholder).not.toBe("Mixed");
  });

  it("adds a genre chip alongside the existing one and saves both, delimited", async () => {
    const { getByLabelText, getByRole, getByText } = render(AlbumTagEditor, {
      songIds: [101],
      initialTracks: [track({ genre: "Rock" })],
      onClose: vi.fn(),
    });

    expect(getByText("Rock")).toBeInTheDocument();

    const genreInput = getByLabelText("Genre") as HTMLInputElement;
    await fireEvent.input(genreInput, { target: { value: "Jazz Fusion" } });
    await fireEvent.keyDown(genreInput, { key: "Enter" });

    expect(getByText("Jazz Fusion")).toBeInTheDocument();
    // The draft field clears once its content becomes a committed chip.
    expect(genreInput.value).toBe("");

    await fireEvent.click(getByRole("button", { name: /save tags/i }));

    await waitFor(() =>
      expect(savedEdit()).toMatchObject({ genre: set("Rock; Jazz Fusion"), album: KEEP }),
    );
  });

  it("removes a genre chip via its remove button", async () => {
    const { getByRole, getByText, queryByText } = render(AlbumTagEditor, {
      songIds: [101],
      initialTracks: [track({ genre: "Rock; Blues" })],
      onClose: vi.fn(),
    });

    expect(getByText("Rock")).toBeInTheDocument();
    expect(getByText("Blues")).toBeInTheDocument();

    await fireEvent.click(getByRole("button", { name: "Remove Rock" }));
    expect(queryByText("Rock")).not.toBeInTheDocument();
    expect(getByText("Blues")).toBeInTheDocument();

    await fireEvent.click(getByRole("button", { name: /save tags/i }));

    await waitFor(() => expect(savedEdit()).toMatchObject({ genre: set("Blues") }));
  });

  it("adds a chip to the Album Artist field and saves it delimited alongside the original", async () => {
    const { getByLabelText, getByRole, getByText } = render(AlbumTagEditor, {
      songIds: [101],
      initialTracks: [track({ album_artist: "Artist A" })],
      onClose: vi.fn(),
    });

    expect(getByText("Artist A")).toBeInTheDocument();

    const albumArtistInput = getByLabelText("Album Artist") as HTMLInputElement;
    await fireEvent.input(albumArtistInput, { target: { value: "Artist B" } });
    await fireEvent.keyDown(albumArtistInput, { key: "Enter" });

    expect(getByText("Artist B")).toBeInTheDocument();

    await fireEvent.click(getByRole("button", { name: /save tags/i }));

    await waitFor(() =>
      expect(savedEdit()).toMatchObject({ albumArtist: set("Artist A; Artist B") }),
    );
  });

  it("allows clearing the Disc input field to save null disc", async () => {
    const { getByLabelText, getByRole } = render(AlbumTagEditor, {
      songIds: [101],
      initialTracks: [track(), track()],
      onClose: vi.fn(),
    });

    const discInput = getByLabelText("Disc #") as HTMLInputElement;
    expect(discInput.value).toBe("1");

    await fireEvent.input(discInput, { target: { value: "" } });
    await fireEvent.click(getByRole("button", { name: /save tags/i }));

    await waitFor(() => expect(savedEdit()).toMatchObject({ disc: set(null) }));
  });

  // Saving a multi-disc album without touching Disc or Disc Subtitle must
  // keep each track's own values rather than writing the first track's (#1940).
  it("shows Mixed for differing discs and subtitles and keeps them unless edited", async () => {
    const { getByLabelText, getByRole } = render(AlbumTagEditor, {
      songIds: [101, 102],
      initialTracks: [
        track({ disc: 1, disc_subtitle: "The Original Album" }),
        track({ disc: 2, disc_subtitle: "BBC Sessions" }),
      ],
      onClose: vi.fn(),
    });

    const discInput = getByLabelText("Disc #") as HTMLInputElement;
    const subtitleInput = getByLabelText("Disc Subtitle") as HTMLInputElement;
    expect(discInput.value).toBe("");
    expect(discInput.placeholder).toBe("Mixed");
    expect(subtitleInput.value).toBe("");
    expect(subtitleInput.placeholder).toBe("Mixed");

    await fireEvent.input(subtitleInput, { target: { value: "  Deluxe  " } });
    await fireEvent.click(getByRole("button", { name: /save tags/i }));

    await waitFor(() =>
      expect(savedEdit()).toMatchObject({ disc: KEEP, discSubtitle: set("  Deluxe  ") }),
    );
  });

  it("toggling Compilation on replaces the Album Artist input with a Various Artists pill and saves both", async () => {
    const onClose = vi.fn();
    const onSave = vi.fn();
    const { getByLabelText, getByRole, getByText, queryByLabelText } = render(AlbumTagEditor, {
      songIds: [101],
      initialTracks: [track({ album_artist: "Artist A" })],
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

    await fireEvent.click(getByRole("button", { name: /save tags/i }));

    await waitFor(() => {
      expect(savedEdit()).toMatchObject({
        albumArtist: set("Various Artists"),
        compilation: set(true),
      });
      expect(onSave).toHaveBeenCalled();
      expect(onClose).toHaveBeenCalled();
    });
  });

  it("shows the real Album Artist as an editable field for a compilation-flagged album whose artist isn't literally Various Artists", async () => {
    const { getByLabelText, getByText, queryByText } = render(AlbumTagEditor, {
      songIds: [101],
      initialTracks: [track({ album_artist: "Artist A", compilation: true })],
      onClose: vi.fn(),
    });

    expect(getByLabelText("Album Artist")).toBeInTheDocument();
    expect(getByText("Artist A")).toBeInTheDocument();
    expect(queryByText("Various Artists")).not.toBeInTheDocument();
  });

  it("disables Clear Embedded Artwork when no track in the album has embedded art", async () => {
    const { getByRole } = render(AlbumTagEditor, {
      songIds: [101, 102],
      hasEmbeddedArt: false,
      onClose: vi.fn(),
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

  it("unchecking Compilation restores the previous Album Artist and leaves it unedited", async () => {
    const { getByLabelText, getByText, getByRole, queryByLabelText } = render(AlbumTagEditor, {
      songIds: [101],
      initialTracks: [track({ album_artist: "Artist A" })],
      onClose: vi.fn(),
    });

    const compilationCheckbox = getByLabelText(/compilation/i) as HTMLInputElement;
    await fireEvent.click(compilationCheckbox);
    expect(queryByLabelText("Album Artist")).not.toBeInTheDocument();

    await fireEvent.click(compilationCheckbox);
    expect(getByLabelText("Album Artist")).toBeInTheDocument();
    expect(getByText("Artist A")).toBeInTheDocument();

    await fireEvent.click(getByRole("button", { name: /save tags/i }));
    await waitFor(() => expect(savedEdit()).toMatchObject({ albumArtist: KEEP }));
  });

  it("saves batch tags for file paths and scans containing folder (#1887)", async () => {
    vi.mocked(invoke).mockImplementation(async (cmd: string) => {
      if (cmd === "save_batch_file_tags") return 2;
      if (cmd === "scan_source_folder") return null;
      if (cmd === "get_file_song_details") return track({ album: "Folder Album" });
      if (cmd === "get_library_snapshot") return { songs: [], albums: [], artists: [] };
      return null;
    });

    const onClose = vi.fn();
    const onSave = vi.fn();
    const { getByLabelText, getByRole } = render(AlbumTagEditor, {
      filePaths: ["C:/Music/01.flac", "C:/Music/02.flac"],
      source: { kind: "folder", id: 1 },
      folderPath: ["Music"],
      folderName: "Music",
      onClose,
      onSave,
    });

    const albumInput = getByLabelText("Album Title") as HTMLInputElement;
    await waitFor(() => expect(albumInput.value).toBe("Folder Album"));

    await fireEvent.input(albumInput, { target: { value: "Updated Folder Album" } });
    await fireEvent.click(getByRole("button", { name: /save tags/i }));

    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith("save_batch_file_tags", {
        paths: ["C:/Music/01.flac", "C:/Music/02.flac"],
        edit: expect.objectContaining({ album: set("Updated Folder Album"), genre: KEEP }),
      });
      expect(invoke).toHaveBeenCalledWith("scan_source_folder", {
        source: { kind: "folder", id: 1 },
        path: ["Music"],
      });
      expect(onSave).toHaveBeenCalled();
      expect(onClose).toHaveBeenCalled();
    });
  });

  it("pre-populates fields with consensus metadata from songs when opened without initial values", async () => {
    collectionStore.songs = [
      {
        id: 201,
        album: "Punisher",
        album_artist: "Phoebe Bridgers",
        genre: "Indie Rock",
        year: 2020,
        disc: 1,
        compilation: false,
        art_embedded: true,
      } as any,
      {
        id: 202,
        album: "Punisher",
        album_artist: "Phoebe Bridgers",
        genre: "Indie Rock",
        year: 2020,
        disc: 1,
        compilation: false,
        art_embedded: true,
      } as any,
    ];

    const { getByLabelText, getByText } = render(AlbumTagEditor, {
      songIds: [201, 202],
      onClose: vi.fn(),
    });

    await waitFor(() => {
      const albumInput = getByLabelText("Album Title") as HTMLInputElement;
      expect(albumInput.value).toBe("Punisher");
      expect(getByText("Phoebe Bridgers")).toBeInTheDocument();
      expect(getByText("Indie Rock")).toBeInTheDocument();
      const yearInput = getByLabelText("Release Year") as HTMLInputElement;
      expect(yearInput.value).toBe("2020");
      const discInput = getByLabelText("Disc #") as HTMLInputElement;
      expect(discInput.value).toBe("1");
    });
  });
});
