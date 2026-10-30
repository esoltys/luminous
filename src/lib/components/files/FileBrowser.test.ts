import "@testing-library/jest-dom";
import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, fireEvent, waitFor } from "@testing-library/svelte";
import { invoke } from "@tauri-apps/api/core";
import FileBrowser from "./FileBrowser.svelte";
import { i18n } from "../../stores/i18n.svelte";
import type { DirListing, FileEntry } from "../../types";

vi.mock("svelte-virtual-list-ts", async () => {
  const mod = await import("../__mocks__/VirtualList.svelte");
  return { VirtualList: mod.default };
});

const source = { kind: "folder", id: 1 } as const;

function entry(name: string, isDir: boolean, status: FileEntry["status"] = null, songId: number | null = null): FileEntry {
  return { name, isDir, size: isDir ? null : 4_000_000, modified: null, fileType: isDir ? null : "FLAC", status, songId };
}

function listing(entries: FileEntry[]): DirListing {
  return { location: "C:/Music", entries, fileCount: entries.filter((e) => !e.isDir).length, totalBytes: 0, inLibraryCount: 0, excluded: false };
}

const tree: Record<string, FileEntry[]> = {
  "": [entry("Duplicates", true, "excluded"), entry("Rock", true)],
  Rock: [entry("Live", true), entry("Studio", true)],
  "Rock/Live": [
    entry("01 Opening.flac", false, "inLibrary", 7),
    entry("02 Encore.flac", false, "notScanned"),
    entry("cover.jpg", false, "albumArt"),
    entry("notes.txt", false, "notAudio"),
  ],
};

function setup(path: string[]) {
  const onnavigate = vi.fn();
  const onplay = vi.fn();
  const result = render(FileBrowser, { props: { source, path, onnavigate, onplay, unreachableLabel: "Can't reach Music" } });
  return { ...result, onnavigate, onplay };
}

describe("FileBrowser.svelte", () => {
  beforeEach(() => {
    i18n.currentLocale = "en-CA";
    vi.mocked(invoke).mockImplementation(async (cmd: string, args?: any) => {
      if (cmd !== "list_source_dir") return null;
      const entries = tree[args.path.join("/")];
      if (!entries) throw "source-not-found";
      return listing(entries);
    });
  });

  it("shows the folders above the current one, then the current folder with each file's status", async () => {
    const { getAllByRole, findByText, getByText } = setup(["Rock", "Live"]);

    await findByText("01 Opening.flac");
    const columns = getAllByRole("listbox");
    expect(columns.map((c) => c.getAttribute("aria-label"))).toEqual(["Folders", "Rock", "Live"]);

    expect(getByText("Yes")).toBeInTheDocument();
    expect(getByText("No, added since last scan")).toBeInTheDocument();
    expect(getByText("Album art")).toBeInTheDocument();
    expect(getByText("Not audio")).toBeInTheDocument();
    expect(getAllByRole("option", { selected: true }).map((o) => o.textContent?.trim())).toEqual(["Rock", "Live"]);
  });

  it("labels reserved folders as excluded", async () => {
    const { findByText } = setup(["Rock"]);
    const duplicates = await findByText("Duplicates");
    expect(duplicates.closest("[role=option]")).toHaveTextContent("Excluded");
  });

  it("moves the selection with the arrow keys, opens folders and plays files", async () => {
    const { getByRole, findByText, onplay, onnavigate } = setup(["Rock", "Live"]);
    await findByText("01 Opening.flac");
    const current = getByRole("listbox", { name: "Live" });

    await fireEvent.keyDown(current, { key: "ArrowDown" });
    await fireEvent.keyDown(current, { key: "ArrowDown" });
    expect(getByRole("option", { name: /02 Encore/ })).toHaveAttribute("aria-selected", "true");

    await fireEvent.keyDown(current, { key: "Enter" });
    expect(onplay).toHaveBeenCalledWith(expect.objectContaining({ name: "02 Encore.flac" }));

    const parent = getByRole("listbox", { name: "Rock" });
    await fireEvent.keyDown(parent, { key: "ArrowDown" });
    expect(onnavigate).toHaveBeenCalledWith(["Rock", "Studio"]);
  });

  it("goes up a level with the Left arrow and selects the folder it left", async () => {
    const { getByRole, findByText, onnavigate, rerender } = setup(["Rock"]);
    await findByText("Live");

    await fireEvent.keyDown(getByRole("listbox", { name: "Rock" }), { key: "ArrowLeft" });
    expect(onnavigate).not.toHaveBeenCalled();

    await rerender({ path: [] });
    await waitFor(() => expect(getByRole("option", { name: /Rock/ })).toHaveAttribute("aria-selected", "true"));
  });

  it("says when the source is gone", async () => {
    const { findByText } = setup(["Missing"]);
    expect(await findByText("This source is no longer available.")).toBeInTheDocument();
  });
});
