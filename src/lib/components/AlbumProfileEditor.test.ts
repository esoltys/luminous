import "@testing-library/jest-dom";
import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, fireEvent } from "@testing-library/svelte";
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
      isOpen: true,
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
      isOpen: true,
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
      isOpen: true,
      onClose: vi.fn(),
    });

    const openFolderBtn = getByRole("button", { name: /open folder/i });
    await fireEvent.click(openFolderBtn);

    expect(invoke).toHaveBeenCalledWith("open_song_folder", { songIds: [1] });
  });
});
