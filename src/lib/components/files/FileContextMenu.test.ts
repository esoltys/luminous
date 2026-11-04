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
    kind: "audio",
    opensInApp: false,
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

function open(entries: FileEntry[], over: {
  source?: FileSource;
  scannable?: boolean;
  inColumn?: boolean;
  onEditFileTags?: (entry: FileEntry) => void;
  onEditFolderTags?: (dir: FileEntry, dirPath: string[]) => void;
  onEditBatchTags?: (entries: FileEntry[]) => void;
} = {}) {
  const onClose = vi.fn();
  const onEditTags = vi.fn();
  const onEditFileTags = over.onEditFileTags ?? vi.fn();
  const onEditFolderTags = over.onEditFolderTags ?? vi.fn();
  const onEditBatchTags = over.onEditBatchTags ?? vi.fn();
  render(FileContextMenu, {
    props: {
      x: 0,
      y: 0,
      source: over.source ?? folderSource,
      path: ["Live"],
      folderName: "Live",
      entries,
      scannable: over.scannable ?? true,
      inColumn: over.inColumn ?? false,
      onEditTags,
      onEditFileTags,
      onEditFolderTags,
      onEditBatchTags,
      onClose,
    },
  });
  return { onClose, onEditTags, onEditFileTags, onEditFolderTags, onEditBatchTags };
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

  it("gives a folder in a folder column the file actions only", () => {
    open([entry("Rock", { isDir: true, status: null, fileType: null, location: "C:/Music/Rock" })], { inColumn: true });

    expect(screen.getAllByRole("menuitem").map((b) => b.textContent?.trim())).toEqual([
      "Edit tags for folder",
      "Open in Picard",
      expect.stringMatching(/^Show in /),
      "Copy path",
    ]);
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

  it("leads a sidecar's menu with Open, which opens it in its app", async () => {
    vi.mocked(invoke).mockResolvedValue("opened");
    open([entry("album.md", { status: "notAudio", kind: "notes", opensInApp: true, fileType: "MD" })]);

    const items = screen.getAllByRole("menuitem");
    expect(items[0]).toHaveTextContent("Open");
    await fireEvent.click(items[0]);
    expect(invoke).toHaveBeenCalledWith("open_source_file", { source: folderSource, path: ["Live", "album.md"] });
  });

  it("says why a sidecar was shown in its folder instead", async () => {
    vi.mocked(invoke).mockResolvedValue("revealed");
    const show = vi.spyOn(toastStore, "show");
    open([entry("rip.cue", { status: "inLibrary", kind: "cueSheet", opensInApp: true, fileType: "CUE" })]);

    await fireEvent.click(screen.getByText("Open"));
    await waitFor(() =>
      expect(show).toHaveBeenCalledWith("No app is set up to open .cue files, so it's shown in its folder instead.", "info")
    );
    show.mockRestore();
  });

  it("offers no Open for a song", () => {
    open([entry("03 Bonus.flac")]);
    expect(screen.queryByText("Open")).not.toBeInTheDocument();
  });

  it("offers no Open for several sidecars at once", () => {
    open([
      entry("album.md", { status: "notAudio", kind: "notes", opensInApp: true }),
      entry("cover.jpg", { status: "albumArt", kind: "image", opensInApp: true }),
    ]);
    expect(screen.queryByText("Open")).not.toBeInTheDocument();
  });

  it("offers Edit tags and Open in Picard for an unscanned audio file", async () => {
    const onEditFileTags = vi.fn();
    open([entry("13 Unfurl (bonus).flac")], { onEditFileTags });

    expect(screen.getByText("Edit tags")).toBeInTheDocument();
    expect(screen.getByText("Open in Picard")).toBeInTheDocument();

    await fireEvent.click(screen.getByText("Edit tags"));
    expect(onEditFileTags).toHaveBeenCalledWith(expect.objectContaining({ name: "13 Unfurl (bonus).flac" }));
  });

  it("offers Edit tags for folder and Open in Picard for a folder", async () => {
    const onEditFolderTags = vi.fn();
    open([entry("2009 - Night Is the New Day", { isDir: true, status: null, fileType: null, location: "C:/Music/2009" })], { onEditFolderTags });

    expect(screen.getByText("Edit tags for folder")).toBeInTheDocument();
    expect(screen.getByText("Open in Picard")).toBeInTheDocument();

    await fireEvent.click(screen.getByText("Edit tags for folder"));
    expect(onEditFolderTags).toHaveBeenCalledWith(
      expect.objectContaining({ name: "2009 - Night Is the New Day" }),
      ["Live", "2009 - Night Is the New Day"]
    );
  });

  it("offers Edit tags and Open in Picard for multi-selection of audio files", async () => {
    const onEditBatchTags = vi.fn();
    open([entry("01 Track.flac"), entry("02 Track.flac")], { onEditBatchTags });

    expect(screen.getByText("Edit tags")).toBeInTheDocument();
    expect(screen.getByText("Open in Picard")).toBeInTheDocument();

    await fireEvent.click(screen.getByText("Edit tags"));
    expect(onEditBatchTags).toHaveBeenCalledWith(expect.arrayContaining([
      expect.objectContaining({ name: "01 Track.flac" }),
      expect.objectContaining({ name: "02 Track.flac" }),
    ]));
  });

  it("doesn't open when there's nothing to offer (a sidecar on a server)", () => {
    const { onClose } = open([entry("cover.jpg", { status: "albumArt", kind: "image", location: null })], {
      source: { kind: "webdav", id: 2 },
      scannable: false,
    });
    expect(screen.queryByRole("menu")).not.toBeInTheDocument();
    expect(onClose).toHaveBeenCalled();
  });
});
