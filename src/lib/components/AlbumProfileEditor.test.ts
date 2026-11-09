import "@testing-library/jest-dom";
import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, fireEvent, waitFor } from "@testing-library/svelte";
import AlbumProfileEditor from "./AlbumProfileEditor.svelte";
import { invoke } from "@tauri-apps/api/core";
import { collectionStore } from "../stores/collection.svelte";
import type { Song } from "../types";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
}));

describe("AlbumProfileEditor.svelte", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    collectionStore.songs = [];
    vi.mocked(invoke).mockImplementation(async (cmd: string) => {
      if (cmd === "open_song_folder") return null;
      if (cmd === "get_library_snapshot") return { songs: [], albums: [], artists: [] };
      return null;
    });
  });

  it("renders the album folder in the Location field rather than the song file path", () => {
    collectionStore.songs = [
      {
        id: 1,
        path: "C:\\Users\\ericj\\OneDrive\\Music\\Big Wreck\\2012 - Albatross\\1-01 Head Together.flac",
        title: "Head Together",
        artist: "Big Wreck",
        album: "Albatross",
      } as Song,
      {
        id: 2,
        path: "C:\\Users\\ericj\\OneDrive\\Music\\Big Wreck\\2012 - Albatross\\1-02 All Is Fair.flac",
        title: "All Is Fair",
        artist: "Big Wreck",
        album: "Albatross",
      } as Song,
    ];

    const { getByText, queryByText } = render(AlbumProfileEditor, {
      albumName: "Albatross",
      artistName: "Big Wreck",
      songIds: [1, 2],
      onClose: vi.fn(),
    });

    // Should display the album folder
    expect(
      getByText("C:\\Users\\ericj\\OneDrive\\Music\\Big Wreck\\2012 - Albatross")
    ).toBeInTheDocument();

    // Should NOT display the first song's file name
    expect(
      queryByText(
        "C:\\Users\\ericj\\OneDrive\\Music\\Big Wreck\\2012 - Albatross\\1-01 Head Together.flac"
      )
    ).not.toBeInTheDocument();
  });

  it("resolves the album directory for multi-disc songs in disc subfolders", () => {
    collectionStore.songs = [
      {
        id: 10,
        path: "C:\\Music\\Pink Floyd\\The Wall\\Disc 1\\01 In the Flesh.flac",
        title: "In the Flesh",
        artist: "Pink Floyd",
        album: "The Wall",
      } as Song,
      {
        id: 11,
        path: "C:\\Music\\Pink Floyd\\The Wall\\Disc 2\\01 Hey You.flac",
        title: "Hey You",
        artist: "Pink Floyd",
        album: "The Wall",
      } as Song,
    ];

    const { getByText, queryByText } = render(AlbumProfileEditor, {
      albumName: "The Wall",
      artistName: "Pink Floyd",
      songIds: [10, 11],
      onClose: vi.fn(),
    });

    expect(getByText("C:\\Music\\Pink Floyd\\The Wall")).toBeInTheDocument();
    expect(queryByText("Disc 1")).not.toBeInTheDocument();
  });

  it("invokes open_song_folder when clicking Open Folder", async () => {
    collectionStore.songs = [
      {
        id: 1,
        path: "C:\\Music\\Artist\\Album\\01.mp3",
        title: "Track 1",
        artist: "Artist",
        album: "Album",
      } as Song,
    ];

    const { getByRole } = render(AlbumProfileEditor, {
      albumName: "Album",
      artistName: "Artist",
      songIds: [1],
      onClose: vi.fn(),
    });

    const openFolderBtn = getByRole("button", { name: /open folder/i });
    await fireEvent.click(openFolderBtn);

    expect(invoke).toHaveBeenCalledWith("open_song_folder", { songIds: [1] });
  });

  it("keeps the user's edited genres in the form while the save is in flight", async () => {
    const tracks = [{ id: 1, path: "/m/a.flac", title: "A", artist: "Artist", album: "Album", genre: "Rock" } as Song];
    collectionStore.songs = tracks;
    // The profile save lands first and updates the profile store; the tag
    // save is held open so the form can be inspected mid-save.
    let finishTagSave!: () => void;
    vi.mocked(invoke).mockImplementation(async (cmd: string, args?: any) => {
      if (cmd === "set_album_profile") return args.profile;
      if (cmd === "save_album_tags") return new Promise<void>((resolve) => { finishTagSave = resolve; });
      return null;
    });

    const { getByLabelText, getByTitle, getByRole, queryByText } = render(AlbumProfileEditor, {
      albumName: "Album",
      artistName: "Artist",
      songIds: [1],
      initialTracks: tracks,
      onClose: vi.fn(),
    });

    await waitFor(() => expect(queryByText("Rock")).toBeInTheDocument());
    await fireEvent.click(getByTitle("Remove Rock"));
    const genreInput = getByLabelText("Genre") as HTMLInputElement;
    await fireEvent.input(genreInput, { target: { value: "Folk Metal" } });
    await fireEvent.keyDown(genreInput, { key: "Enter" });

    await fireEvent.click(getByRole("button", { name: /^save$/i }));
    await waitFor(() => expect(invoke).toHaveBeenCalledWith("save_album_tags", expect.anything()));
    await waitFor(() => expect(collectionStore.getAlbumProfile("Album")).toBeDefined());

    expect(queryByText("Folk Metal")).toBeInTheDocument();
    expect(queryByText("Rock")).not.toBeInTheDocument();
    finishTagSave();
  });

  it("edits and saves album language tag", async () => {
    const tracks = [
      { id: 1, path: "/m/a.flac", title: "A", artist: "Artist", album: "Album", language: "eng" } as Song,
      { id: 2, path: "/m/b.flac", title: "B", artist: "Artist", album: "Album", language: "eng" } as Song,
    ];
    collectionStore.songs = tracks;

    vi.mocked(invoke).mockImplementation(async (cmd: string, args?: any) => {
      if (cmd === "set_album_profile") return args.profile;
      if (cmd === "save_album_tags") return null;
      return null;
    });

    const { getByLabelText, getByRole } = render(AlbumProfileEditor, {
      albumName: "Album",
      artistName: "Artist",
      songIds: [1, 2],
      initialTracks: tracks,
      onClose: vi.fn(),
    });

    const langInput = getByLabelText("Language") as HTMLInputElement;
    expect(langInput.value).toBe("eng");

    await fireEvent.input(langInput, { target: { value: "dan" } });
    await fireEvent.click(getByRole("button", { name: /^save$/i }));

    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith("save_album_tags", {
        songIds: [1, 2],
        edit: expect.objectContaining({
          language: { mode: "set", value: "dan" },
        }),
      });
    });
  });
});
