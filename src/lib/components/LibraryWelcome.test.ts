import "@testing-library/jest-dom";
import { describe, it, expect, vi, beforeEach } from "vitest";
import { fireEvent, render, screen, waitFor } from "@testing-library/svelte";
import LibraryWelcome from "./LibraryWelcome.svelte";
import { collectionStore } from "../stores/collection.svelte";
import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn().mockResolvedValue({ played: 0, skipped: 0 }),
}));

vi.mock("@tauri-apps/plugin-dialog", () => ({
  open: vi.fn(),
  save: vi.fn(),
}));

describe("LibraryWelcome.svelte", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    collectionStore.libraryLoaded = true;
  });

  // Before the first library read lands, an empty view is a large library
  // still loading, not a missing one (#1968).
  it("shows a loading state instead of offering a library before the first load", () => {
    collectionStore.libraryLoaded = false;
    render(LibraryWelcome);
    expect(screen.getByRole("status")).toHaveTextContent("Loading your library...");
    expect(screen.queryByRole("button", { name: /Add a folder/ })).toBeNull();
    expect(screen.queryByRole("button", { name: /Open files/ })).toBeNull();
  });

  it("offers adding a folder as primary alongside playing files without a library", () => {
    render(LibraryWelcome);
    expect(screen.getByRole("heading", { name: "Add your music" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /Add a folder/ })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /Open files/ })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /Play a folder/ })).toBeInTheDocument();
  });

  it("plays a picked folder without adding it to the watched library", async () => {
    vi.mocked(open).mockResolvedValueOnce(["C:\\Music\\Album"]);
    render(LibraryWelcome);

    await fireEvent.click(screen.getByRole("button", { name: /Play a folder/ }));

    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith("open_and_play", { paths: ["C:\\Music\\Album"] });
    });
    expect(open).toHaveBeenCalledWith(expect.objectContaining({ directory: true, multiple: true }));
    expect(invoke).not.toHaveBeenCalledWith("add_directory", expect.anything());
    expect(invoke).not.toHaveBeenCalledWith("scan_directories", expect.anything());
  });

  it("opens the audio file picker from Open files", async () => {
    vi.mocked(open).mockResolvedValueOnce(null);
    render(LibraryWelcome);

    await fireEvent.click(screen.getByRole("button", { name: /Open files/ }));

    await waitFor(() => {
      expect(open).toHaveBeenCalledWith(
        expect.objectContaining({ directory: false, multiple: true }),
      );
    });
  });
});
