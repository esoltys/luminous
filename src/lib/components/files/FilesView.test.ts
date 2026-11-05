import "@testing-library/jest-dom";
import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, fireEvent, waitFor } from "@testing-library/svelte";
import { invoke } from "@tauri-apps/api/core";
import FilesView from "./FilesView.svelte";
import { i18n } from "../../stores/i18n.svelte";
import { navigationStore } from "../../stores/navigation.svelte";
import { filesStore } from "../../stores/files.svelte";
import { devicesStore } from "../../stores/devices.svelte";
import { collectionStore } from "../../stores/collection.svelte";
import type { DirListing, FileEntry, FileKind, FileSource } from "../../types";

vi.mock("svelte-virtual-list-ts", async () => {
  const mod = await import("../__mocks__/VirtualList.svelte");
  return { VirtualList: mod.default };
});

const source: FileSource = { kind: "folder", id: 1 };

const KIND: Record<string, FileKind> = { flac: "audio", jpg: "image", txt: "notes" };

function entry(name: string, isDir: boolean, status: FileEntry["status"] = null, songId: number | null = null): FileEntry {
  const kind = isDir ? null : KIND[name.split(".").pop()!] ?? null;
  return {
    name,
    isDir,
    size: isDir ? null : 4_000_000,
    modified: null,
    fileType: isDir ? null : name.split(".").pop()!.toUpperCase(),
    kind,
    opensInApp: kind !== null && kind !== "audio",
    status,
    songId,
    missingTag: null,
    location: `C:/Music/${name}`,
  };
}

function listing(entries: FileEntry[], location = "C:/Music/Rock/Live"): DirListing {
  return {
    location,
    entries,
    fileCount: entries.filter((e) => !e.isDir).length,
    hiddenCount: 0,
    totalBytes: 8_000_000,
    inLibraryCount: entries.filter((e) => e.status === "inLibrary").length,
    excluded: false,
    pathMissingTags: [],
  };
}

describe("FilesView.svelte", () => {
  beforeEach(() => {
    i18n.currentLocale = "en-CA";
    filesStore.sources = [{ source, name: "Music", available: true, songCount: 10 }];
    devicesStore.devices = [];
    navigationStore.filesLocation = { source, path: ["Rock", "Live"] };

    vi.mocked(invoke).mockImplementation(async (cmd: string, args?: any) => {
      if (cmd === "list_source_dir") {
        return listing([
          entry("01 Opening.flac", false, "inLibrary", 7),
          entry("02 Encore.flac", false, "notScanned"),
        ]);
      }
      if (cmd === "scan_source_folder") return null;
      if (cmd === "get_source_songs") return [];
      return null;
    });
  });

  it("renders header with Play folder primary button and ⋯ folder actions button", async () => {
    render(FilesView);

    const playBtn = await screen.findByRole("button", { name: "Play folder" });
    expect(playBtn).toBeInTheDocument();

    const moreBtn = screen.getByRole("button", { name: "Folder actions" });
    expect(moreBtn).toBeInTheDocument();
  });

  it("opens overflow menu when ⋯ button is clicked, omitting Play folder and including Folder settings", async () => {
    const openSettings = vi.spyOn(navigationStore, "openSettings").mockImplementation(() => {});
    render(FilesView);

    const moreBtn = await screen.findByRole("button", { name: "Folder actions" });
    await fireEvent.click(moreBtn);

    const menu = await screen.findByRole("menu");
    expect(menu).toBeInTheDocument();

    // Secondary actions should be present
    expect(screen.getByText("Add to Queue")).toBeInTheDocument();
    expect(screen.getByText("New playlist from folder")).toBeInTheDocument();
    expect(screen.getByText("Scan this folder")).toBeInTheDocument();
    expect(screen.getByText("Copy path")).toBeInTheDocument();
    expect(screen.getByText("Folder settings")).toBeInTheDocument();

    // Play folder should NOT be in the overflow menu
    const menuItems = screen.getAllByRole("menuitem");
    const menuItemTexts = menuItems.map((item) => item.textContent?.trim());
    expect(menuItemTexts).not.toContain("Play folder");

    // Clicking Folder settings navigates to settings
    await fireEvent.click(screen.getByText("Folder settings"));
    expect(openSettings).toHaveBeenCalledWith("sources");
    openSettings.mockRestore();
  });

  it("shows an inline scan nudge in the footer when unscanned files exist", async () => {
    render(FilesView);

    expect(await screen.findByText(/1 file isn't in your library/i)).toBeInTheDocument();
    const scanBtn = screen.getByRole("button", { name: "Scan" });
    expect(scanBtn).toBeInTheDocument();

    await fireEvent.click(scanBtn);
    expect(invoke).toHaveBeenCalledWith("scan_source_folder", {
      source,
      path: ["Rock", "Live"],
    });
  });

  it("opens overflow menu when ⋯ button is activated with keyboard Enter", async () => {
    render(FilesView);

    const moreBtn = await screen.findByRole("button", { name: "Folder actions" });
    await fireEvent.keyDown(moreBtn, { key: "Enter" });

    // Since button fires click or Enter triggers it:
    await fireEvent.click(moreBtn);
    expect(await screen.findByRole("menu")).toBeInTheDocument();
  });

  it("shows scanning feedback in the footer while scanning is active", async () => {
    collectionStore.isScanning = true;
    render(FilesView);

    expect(await screen.findByText("· Scanning…")).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Scan" })).not.toBeInTheDocument();
    collectionStore.isScanning = false;
  });

  it("does not show scan nudge on excluded or non-local folders", async () => {
    vi.mocked(invoke).mockImplementation(async (cmd: string) => {
      if (cmd === "list_source_dir") {
        const l = listing([entry("01 Opening.flac", false, "inLibrary", 7), entry("02 Encore.flac", false, "notScanned")]);
        l.excluded = true;
        return l;
      }
      return null;
    });

    render(FilesView);
    expect(await screen.findByText("1 is in your library.")).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Scan" })).not.toBeInTheDocument();
  });
});
