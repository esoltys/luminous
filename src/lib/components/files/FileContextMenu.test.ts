import "@testing-library/jest-dom";
import { describe, it, expect, beforeEach, vi } from "vitest";
import { render, screen, fireEvent, waitFor } from "@testing-library/svelte";
import { invoke } from "@tauri-apps/api/core";
import FileContextMenu from "./FileContextMenu.svelte";
import { i18n } from "../../stores/i18n.svelte";
import { playlistsStore } from "../../stores/playlists.svelte";
import { playerStore } from "../../stores/player.svelte";
import { toastStore } from "../../stores/toast.svelte";
import type { FileEntry, FileSource, Song } from "../../types";

const folderSource: FileSource = { kind: "folder", id: 1 };

function entry(name: string, over: Partial<FileEntry> = {}): FileEntry {
  return {
    name,
    isDir: false,
    size: 1000,
    modified: null,
    fileType: "FLAC",
    status: "notScanned",
    songId: null,
    missingTag: null,
    location: `C:/Music/Live/${name}`,
    ...over,
  };
}

function song(id: number, title: string): Song {
  return {
    id,
    title,
    source: "local_file",
    filetype: "FLAC",
    art_embedded: false,
    art_unset: false,
    compilation: false,
    beginning_nanosec: 0,
    end_nanosec: 0,
    rating: 0,
    playcount: 0,
    skipcount: 0,
    length_nanosec: 0,
    added: 0,
    unavailable: false,
  };
}

const library = { 7: song(7, "Opening"), 8: song(8, "Encore") } as Record<number, Song>;

function open(entries: FileEntry[], over: { source?: FileSource; scannable?: boolean } = {}) {
  const onClose = vi.fn();
  const onEditTags = vi.fn();
  render(FileContextMenu, {
    props: {
      x: 0,
      y: 0,
      source: over.source ?? folderSource,
      path: ["Live"],
      folderName: "Live",
      entries,
      scannable: over.scannable ?? true,
      onEditTags,
      onClose,
    },
  });
  return { onClose, onEditTags };
}

describe("FileContextMenu.svelte", () => {
  beforeEach(() => {
    i18n.currentLocale = "en-CA";
    vi.mocked(invoke).mockImplementation(async (cmd: string, args?: any) => {
      if (cmd === "get_songs_by_ids") return args.songIds.map((id: number) => library[id]);
      if (cmd === "get_source_songs") return [library[7], library[8]];
      return null;
    });
  });

  it("gives a folder the folder actions, acting on that folder", async () => {
    const addToQueue = vi.spyOn(playlistsStore, "addSongsToQueue").mockResolvedValue(undefined as never);
    open([entry("Disc 2", { isDir: true, status: null, fileType: null, location: "C:/Music/Live/Disc 2" })]);

    for (const label of ["Play folder", "Add to Queue", "New playlist from folder", "Scan this folder", "Copy path"]) {
      expect(screen.getByText(label)).toBeInTheDocument();
    }
    await fireEvent.click(screen.getByText("Add to Queue"));
    await waitFor(() => expect(addToQueue).toHaveBeenCalledWith([7, 8]));
    expect(invoke).toHaveBeenCalledWith("get_source_songs", { source: folderSource, path: ["Live", "Disc 2"] });
    addToQueue.mockRestore();
  });

  it("gives a library song the song menu, with the file's own actions after it", async () => {
    open([entry("01 Opening.flac", { status: "inLibrary", songId: 7 })]);

    expect(await screen.findByText("Play Song")).toBeInTheDocument();
    expect(screen.getByText("Copy path")).toBeInTheDocument();
    expect(screen.queryByText("View in Files")).not.toBeInTheDocument();
  });

  it("acts on the library songs of a mixed selection and says how many files it left out", async () => {
    const addToQueue = vi.spyOn(playlistsStore, "addSongsToQueue").mockResolvedValue(undefined as never);
    const show = vi.spyOn(toastStore, "show");
    open([
      entry("01 Opening.flac", { status: "inLibrary", songId: 7 }),
      entry("02 Encore.flac", { status: "inLibrary", songId: 8 }),
      entry("03 Bonus.flac"),
    ]);

    await fireEvent.click(await screen.findByText("Add to Queue"));
    await waitFor(() => expect(addToQueue).toHaveBeenCalledWith([7, 8]));
    expect(show).toHaveBeenCalledWith(
      "Added 2 songs to Queue. 1 file isn't in your library yet: scan this folder to include it."
    );
    addToQueue.mockRestore();
    show.mockRestore();
  });

  it("offers a file not in the library to play without adding it, or to scan its folder", async () => {
    const openAndPlay = vi.spyOn(playerStore, "openAndPlay").mockResolvedValue(undefined as never);
    open([entry("03 Bonus.flac")]);

    expect(screen.getByText("Scan this folder")).toBeInTheDocument();
    await fireEvent.click(screen.getByText("Play without adding"));
    expect(openAndPlay).toHaveBeenCalledWith(["C:/Music/Live/03 Bonus.flac"]);
    openAndPlay.mockRestore();
  });

  it("doesn't open when there's nothing to offer (a sidecar on a server)", () => {
    const { onClose } = open([entry("cover.jpg", { status: "albumArt", location: null })], {
      source: { kind: "webdav", id: 2 },
      scannable: false,
    });
    expect(screen.queryByRole("menu")).not.toBeInTheDocument();
    expect(onClose).toHaveBeenCalled();
  });
});
