import "@testing-library/jest-dom";
import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { render, fireEvent, waitFor, within } from "@testing-library/svelte";
import { invoke } from "@tauri-apps/api/core";
import FileBrowser from "./FileBrowser.svelte";
import { i18n } from "../../stores/i18n.svelte";
import type { DirListing, FileEntry, FileKind } from "../../types";

vi.mock("svelte-virtual-list-ts", async () => {
  const mod = await import("../__mocks__/VirtualList.svelte");
  return { VirtualList: mod.default };
});

const source = { kind: "folder", id: 1 } as const;

/** What the backend says each extension in these fixtures is. */
const KIND: Record<string, FileKind> = { flac: "audio", jpg: "image", txt: "notes", lrc: "lyrics", pdf: "pdf" };

function entry(name: string, isDir: boolean, status: FileEntry["status"] = null, songId: number | null = null): FileEntry {
  const kind = isDir ? null : KIND[name.split(".").pop()!] ?? null;
  return { name, isDir, size: isDir ? null : 4_000_000, modified: null, fileType: isDir ? null : name.split(".").pop()!.toUpperCase(), kind, opensInApp: kind !== null && kind !== "audio", status, songId, missingTag: null, location: `C:/Music/${name}` };
}

function listing(entries: FileEntry[]): DirListing {
  return { location: "C:/Music", entries, fileCount: entries.filter((e) => !e.isDir).length, hiddenCount: 0, totalBytes: 0, inLibraryCount: 0, excluded: false, pathMissingTags: [] };
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
  const onactivate = vi.fn();
  const result = render(FileBrowser, { props: { source, path, onnavigate, onactivate, unreachableLabel: "Can't reach Music" } });
  return { ...result, onnavigate, onactivate };
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

  it("draws each file with its kind's icon", async () => {
    const { findByText, getByRole } = setup(["Rock", "Live"]);
    await findByText("01 Opening.flac");
    const kindOf = (name: RegExp) => getByRole("option", { name }).querySelector("[data-kind]")?.getAttribute("data-kind");
    expect(kindOf(/01 Opening/)).toBe("audio");
    expect(kindOf(/cover\.jpg/)).toBe("image");
    expect(kindOf(/notes\.txt/)).toBe("notes");
  });

  it("activates any file with Enter or a double-click, not just songs", async () => {
    const { getByRole, findByText, onactivate } = setup(["Rock", "Live"]);
    const notes = await findByText("notes.txt");
    await fireEvent.dblClick(notes);
    expect(onactivate).toHaveBeenCalledWith(expect.objectContaining({ name: "notes.txt", kind: "notes" }));

    await fireEvent.click(notes);
    await fireEvent.keyDown(getByRole("listbox", { name: "Live" }), { key: "Enter" });
    expect(onactivate).toHaveBeenCalledTimes(2);
  });

  it("labels reserved folders as excluded", async () => {
    const { findByText } = setup(["Rock"]);
    const duplicates = await findByText("Duplicates");
    expect(duplicates.closest("[role=option]")).toHaveTextContent("Excluded");
  });

  it("moves the selection with the arrow keys, opens folders and plays files", async () => {
    const { getByRole, findByText, onactivate, onnavigate } = setup(["Rock", "Live"]);
    await findByText("01 Opening.flac");
    const current = getByRole("listbox", { name: "Live" });

    await fireEvent.keyDown(current, { key: "ArrowDown" });
    await fireEvent.keyDown(current, { key: "ArrowDown" });
    expect(getByRole("option", { name: /02 Encore/ })).toHaveAttribute("aria-selected", "true");

    await fireEvent.keyDown(current, { key: "Enter" });
    expect(onactivate).toHaveBeenCalledWith(expect.objectContaining({ name: "02 Encore.flac" }));

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

  it("adds files to the selection with Ctrl and selects a run with Shift", async () => {
    const { getByRole, findByText } = setup(["Rock", "Live"]);
    await findByText("01 Opening.flac");
    const selected = () =>
      ["01 Opening.flac", "02 Encore.flac", "cover.jpg", "notes.txt"].filter(
        (name) => getByRole("option", { name: new RegExp(name) }).getAttribute("aria-selected") === "true"
      );

    await fireEvent.click(getByRole("option", { name: /01 Opening/ }));
    await fireEvent.click(getByRole("option", { name: /cover\.jpg/ }), { ctrlKey: true });
    expect(selected()).toEqual(["01 Opening.flac", "cover.jpg"]);

    await fireEvent.click(getByRole("option", { name: /notes\.txt/ }), { shiftKey: true });
    expect(selected()).toEqual(["cover.jpg", "notes.txt"]);

    await fireEvent.click(getByRole("option", { name: /02 Encore/ }));
    expect(selected()).toEqual(["02 Encore.flac"]);
  });

  /** The current folder's listing with a folder in it, as "Rock/Live" has none. */
  function withFolder() {
    vi.mocked(invoke).mockImplementation(async (cmd: string, args?: any) => {
      if (cmd !== "list_source_dir") return null;
      const key = args.path.join("/");
      return listing(key === "Rock/Live" ? [entry("Scans", true), ...tree[key]] : tree[key]);
    });
  }

  /** The names selected in a column. */
  const selectedIn = (column: HTMLElement) =>
    within(column).queryAllByRole("option", { selected: true }).map((o) => o.querySelector("[title]")?.getAttribute("title"));

  it("extends the selection with Shift+arrows and selects every file, not folders, with Ctrl+A", async () => {
    withFolder();
    const { getByRole, findByText } = setup(["Rock", "Live"]);
    await findByText("01 Opening.flac");
    const current = getByRole("listbox", { name: "Live" });

    await fireEvent.click(within(current).getByRole("option", { name: /01 Opening/ }));
    await fireEvent.keyDown(current, { key: "ArrowDown", shiftKey: true });
    await fireEvent.keyDown(current, { key: "ArrowDown", shiftKey: true });
    expect(selectedIn(current)).toEqual(["01 Opening.flac", "02 Encore.flac", "cover.jpg"]);

    await fireEvent.keyDown(current, { key: "a", ctrlKey: true });
    expect(selectedIn(current)).toEqual(["01 Opening.flac", "02 Encore.flac", "cover.jpg", "notes.txt"]);
  });

  describe("right-click", () => {
    function setupMenu() {
      const oncontextmenu = vi.fn();
      const result = render(FileBrowser, {
        props: { source, path: ["Rock", "Live"], onnavigate: vi.fn(), onactivate: vi.fn(), unreachableLabel: "", oncontextmenu },
      });
      const names = () => oncontextmenu.mock.lastCall![0].entries.map((e: FileEntry) => e.name);
      return { ...result, oncontextmenu, names };
    }

    it("acts on the whole selection when the row is in it", async () => {
      const { getByRole, findByText, names } = setupMenu();
      await findByText("01 Opening.flac");
      await fireEvent.click(getByRole("option", { name: /01 Opening/ }));
      await fireEvent.click(getByRole("option", { name: /02 Encore/ }), { shiftKey: true });

      await fireEvent.contextMenu(getByRole("option", { name: /02 Encore/ }), { clientX: 10, clientY: 20 });
      expect(names()).toEqual(["01 Opening.flac", "02 Encore.flac"]);
    });

    it("replaces the selection with a row outside it, and acts on a folder alone", async () => {
      withFolder();
      const { getByRole, findByText, names } = setupMenu();
      await findByText("01 Opening.flac");
      await fireEvent.click(getByRole("option", { name: /01 Opening/ }));

      await fireEvent.contextMenu(getByRole("option", { name: /cover\.jpg/ }));
      expect(names()).toEqual(["cover.jpg"]);
      expect(getByRole("option", { name: /01 Opening/ })).toHaveAttribute("aria-selected", "false");

      await fireEvent.contextMenu(getByRole("option", { name: /Scans/ }));
      expect(names()).toEqual(["Scans"]);
    });

    it("offers a folder in a folder column its own menu, without opening or selecting it", async () => {
      const { getByRole, findByText, names, oncontextmenu } = setupMenu();
      await findByText("01 Opening.flac");
      const studio = within(getByRole("listbox", { name: "Rock" })).getByRole("option", { name: /Studio/ });

      await fireEvent.contextMenu(studio);
      expect(names()).toEqual(["Studio"]);
      expect(oncontextmenu.mock.lastCall![0]).toMatchObject({ dir: ["Rock"], inColumn: true });
      expect(studio).toHaveAttribute("aria-selected", "false");
    });
  });

  describe("arriving at a file (View in Files)", () => {
    afterEach(() => {
      vi.useRealTimers();
    });

    const option = (getByRole: (role: string, opts: object) => HTMLElement, name: string) =>
      getByRole("option", { name: new RegExp(name) });

    it("selects the target, outlines it, and fades the halo after a moment", async () => {
      vi.useFakeTimers({ shouldAdvanceTime: true });
      const { getByRole, findByText } = render(FileBrowser, {
        props: { source, path: ["Rock", "Live"], target: "02 Encore.flac", arrival: "02 Encore.flac", onnavigate: vi.fn(), onactivate: vi.fn(), unreachableLabel: "" },
      });
      await findByText("02 Encore.flac");
      const encore = option(getByRole, "02 Encore");

      expect(encore).toHaveAttribute("aria-selected", "true");
      expect(encore).toHaveClass("arrival", "arrival-halo");

      await vi.advanceTimersByTimeAsync(1200);
      expect(encore).toHaveClass("arrival");
      expect(encore).not.toHaveClass("arrival-halo");

      await fireEvent.click(option(getByRole, "01 Opening"));
      expect(encore).not.toHaveClass("arrival");
    });

    it("starts the halo when the row appears, not before, on a slow source", async () => {
      vi.useFakeTimers({ shouldAdvanceTime: true });
      let release = () => {};
      vi.mocked(invoke).mockImplementation(
        (_cmd: string, args?: any) =>
          new Promise((resolve) => {
            const reply = () => resolve(listing(tree[args.path.join("/")]));
            if (args.path.length === 2) release = reply;
            else reply();
          })
      );
      const { getByRole, findByText } = render(FileBrowser, {
        props: { source, path: ["Rock", "Live"], target: "02 Encore.flac", arrival: "02 Encore.flac", onnavigate: vi.fn(), onactivate: vi.fn(), unreachableLabel: "" },
      });

      await vi.advanceTimersByTimeAsync(2000);
      release();
      await findByText("02 Encore.flac");
      await waitFor(() => expect(option(getByRole, "02 Encore")).toHaveClass("arrival-halo"));

      await vi.advanceTimersByTimeAsync(1200);
      expect(option(getByRole, "02 Encore")).not.toHaveClass("arrival-halo");
    });

    it("arrives again at the file already targeted after another row was clicked", async () => {
      const { getByRole, findByText, rerender } = render(FileBrowser, {
        props: { source, path: ["Rock", "Live"], target: "02 Encore.flac", arrival: "02 Encore.flac", onnavigate: vi.fn(), onactivate: vi.fn(), unreachableLabel: "" },
      });
      await findByText("02 Encore.flac");
      await fireEvent.click(option(getByRole, "01 Opening"));
      expect(option(getByRole, "02 Encore")).not.toHaveClass("arrival");

      await rerender({ arrival: "02 Encore.flac" });
      await waitFor(() => expect(option(getByRole, "02 Encore")).toHaveClass("arrival", "arrival-halo"));
      expect(option(getByRole, "02 Encore")).toHaveAttribute("aria-selected", "true");
      expect(option(getByRole, "01 Opening")).toHaveAttribute("aria-selected", "false");
    });

    it("selects the target without marking it when it isn't an arrival (Back, restart)", async () => {
      const { getByRole, findByText } = render(FileBrowser, {
        props: { source, path: ["Rock", "Live"], target: "02 Encore.flac", onnavigate: vi.fn(), onactivate: vi.fn(), unreachableLabel: "" },
      });
      await findByText("02 Encore.flac");
      const encore = option(getByRole, "02 Encore");
      expect(encore).toHaveAttribute("aria-selected", "true");
      expect(encore).not.toHaveClass("arrival");
    });

    it("selects a new target in the folder already shown, and the usual entry after moving on", async () => {
      const { getByRole, findByText, rerender } = render(FileBrowser, {
        props: { source, path: ["Rock", "Live"], target: "01 Opening.flac", onnavigate: vi.fn(), onactivate: vi.fn(), unreachableLabel: "" },
      });
      await findByText("01 Opening.flac");
      expect(option(getByRole, "01 Opening")).toHaveAttribute("aria-selected", "true");

      await rerender({ target: "02 Encore.flac" });
      expect(option(getByRole, "02 Encore")).toHaveAttribute("aria-selected", "true");
      expect(option(getByRole, "01 Opening")).toHaveAttribute("aria-selected", "false");

      await rerender({ path: ["Rock"], target: null });
      await findByText("Studio");
      const selected = [...getByRole("listbox", { name: "Rock" }).querySelectorAll("[aria-selected=true]")];
      expect(selected.map((o) => o.textContent?.trim())).toEqual(["Live"]);
    });
  });

  it("says when the source is gone", async () => {
    const { findByText } = setup(["Missing"]);
    expect(await findByText("This source is no longer available.")).toBeInTheDocument();
  });

  it("words a Subsonic folder for songs missing a tag and opens it by its empty name", async () => {
    // Keyed by the whole path: `[""]` is a folder, not the root.
    const subsonic: Record<string, DirListing> = {
      "[]": listing([{ ...entry("", true), missingTag: "artist" }, entry("AC/DC", true)]),
      '[""]': { ...listing([{ ...entry("", true), missingTag: "album" }]), pathMissingTags: ["artist"] },
    };
    vi.mocked(invoke).mockImplementation(async (cmd: string, args?: any) => {
      if (cmd !== "list_source_dir") return null;
      const found = subsonic[JSON.stringify(args.path)];
      if (!found) throw "source-not-found";
      return found;
    });
    const onnavigate = vi.fn();
    const { getAllByRole, findByText, getByText } = render(FileBrowser, {
      props: { source: { kind: "subsonic", id: 3 }, path: [""], onnavigate, onactivate: vi.fn(), unreachableLabel: "Can't reach" },
    });

    await findByText("Unknown Album");
    await waitFor(() =>
      expect(getAllByRole("listbox").map((c) => c.getAttribute("aria-label"))).toEqual(["Folders", "Unknown Artist"])
    );
    expect(getByText("Unknown Artist").closest("[role=option]")).toHaveAttribute("aria-selected", "true");
    expect(getByText("AC/DC")).toBeInTheDocument();

    await fireEvent.click(getByText("Unknown Album"));
    expect(onnavigate).toHaveBeenCalledWith(["", ""]);
  });

  describe("folder filtering (#1886)", () => {
    it("filters entries by free text, ext:, and status:, and highlights matched substrings", async () => {
      const { findByText, queryByText, container, rerender } = render(FileBrowser, {
        props: {
          source,
          path: ["Rock", "Live"],
          filterQuery: "ext:flac encore",
          onnavigate: vi.fn(),
          onactivate: vi.fn(),
          unreachableLabel: "Can't reach Music",
        },
      });

      await findByText(/02/);
      expect(queryByText("01 Opening.flac")).not.toBeInTheDocument();
      expect(queryByText("cover.jpg")).not.toBeInTheDocument();
      expect(container.querySelector("mark")?.textContent).toBe("Encore");

      await rerender({ filterQuery: "status:unscanned" });
      expect(queryByText("02 Encore.flac")).toBeInTheDocument();
      expect(queryByText("01 Opening.flac")).not.toBeInTheDocument();
    });

    it("shows the empty filter state and clears the filter when Clear filter is clicked", async () => {
      const { findByTestId, findByText, getByRole } = render(FileBrowser, {
        props: {
          source,
          path: ["Rock", "Live"],
          filterQuery: "ext:cue",
          onnavigate: vi.fn(),
          onactivate: vi.fn(),
          unreachableLabel: "Can't reach Music",
        },
      });

      const empty = await findByTestId("files-filter-empty");
      expect(empty).toHaveTextContent("Nothing in this folder matches ext:cue.");
      expect(empty).toHaveTextContent("Clear the filter to see all 4 files, or change it to look for something else.");

      await fireEvent.click(getByRole("button", { name: "Clear filter" }));
      expect(await findByText("01 Opening.flac")).toBeInTheDocument();
    });

    it("selects only filtered files on Ctrl+A", async () => {
      const { findByText, getByRole } = render(FileBrowser, {
        props: {
          source,
          path: ["Rock", "Live"],
          filterQuery: "ext:flac",
          onnavigate: vi.fn(),
          onactivate: vi.fn(),
          unreachableLabel: "Can't reach Music",
        },
      });

      await findByText("01 Opening.flac");
      const current = getByRole("listbox", { name: "Live" });
      await fireEvent.keyDown(current, { key: "a", ctrlKey: true });
      expect(selectedIn(current)).toEqual(["01 Opening.flac", "02 Encore.flac"]);
    });
  });

  describe("sorting, keyboard navigation, and drag-and-drop (#1888)", () => {
    it("toggles column sorting and updates arrow indicators", async () => {
      const { findByText, getByRole } = setup(["Rock", "Live"]);
      await findByText("01 Opening.flac");

      const nameBtn = getByRole("button", { name: "Name" });
      expect(nameBtn).toHaveTextContent("▲");

      await fireEvent.click(nameBtn);
      expect(nameBtn).toHaveTextContent("▼");

      const typeBtn = getByRole("button", { name: "Type" });
      await fireEvent.click(typeBtn);
      expect(typeBtn).toHaveTextContent("▲");
      expect(nameBtn).not.toHaveTextContent("▼");
      expect(nameBtn).not.toHaveTextContent("▲");
    });

    it("navigates to parent folder on Backspace and Alt+ArrowUp", async () => {
      const { findByText, getByRole, onnavigate } = setup(["Rock", "Live"]);
      await findByText("01 Opening.flac");
      const current = getByRole("listbox", { name: "Live" });

      await fireEvent.keyDown(current, { key: "Backspace" });
      expect(onnavigate).toHaveBeenCalledWith(["Rock"]);

      await fireEvent.keyDown(current, { key: "ArrowUp", altKey: true });
      expect(onnavigate).toHaveBeenCalledTimes(2);
      expect(onnavigate).toHaveBeenLastCalledWith(["Rock"]);
    });

    it("starts file drag on pointer drag gesture", async () => {
      const { songDrag } = await import("../../stores/songDrag.svelte");
      const { findByText, getByRole } = setup(["Rock", "Live"]);
      await findByText("01 Opening.flac");
      const option = getByRole("option", { name: /01 Opening/ });

      await fireEvent.pointerDown(option, { button: 0, clientX: 100, clientY: 100 });
      expect(songDrag.isDragging).toBe(false);

      // Move past the 4px drag threshold
      await fireEvent.pointerMove(window, { clientX: 110, clientY: 110 });
      expect(songDrag.isDragging).toBe(true);
      expect(songDrag.payload?.songIds).toContain(7);
      expect(songDrag.payload?.label).toBe("01 Opening.flac");

      await fireEvent.pointerUp(window);
      expect(songDrag.isDragging).toBe(false);
    });
  });

  it("renders AlphabetScrubber when a folder has >= 15 items sorted by name (#1973)", async () => {
    const manyEntries: FileEntry[] = Array.from({ length: 20 }, (_, i) =>
      entry(`Track ${String.fromCharCode(65 + (i % 26))}${i}.flac`, false, "inLibrary", i + 100)
    );
    tree["ManyTracks"] = manyEntries;

    const { findByTestId } = setup(["ManyTracks"]);
    const scrubber = await findByTestId("alphabet-scrubber");
    expect(scrubber).toBeInTheDocument();
  });
});
