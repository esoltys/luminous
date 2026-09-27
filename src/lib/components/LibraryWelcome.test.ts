import "@testing-library/jest-dom";
import { describe, it, expect, vi, beforeEach } from "vitest";
import { fireEvent, render, screen, waitFor } from "@testing-library/svelte";
import LibraryWelcome from "./LibraryWelcome.svelte";
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
  });

  it("offers playing files without a library alongside building one", () => {
    render(LibraryWelcome);
    expect(screen.getByRole("button", { name: /Open Files/ })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /Play a Folder/ })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /Add Folder to Library/ })).toBeInTheDocument();
  });

  it("plays a picked folder without adding it to the watched library", async () => {
    vi.mocked(open).mockResolvedValueOnce(["C:\\Music\\Album"]);
    render(LibraryWelcome);

    await fireEvent.click(screen.getByRole("button", { name: /Play a Folder/ }));

    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith("open_and_play", { paths: ["C:\\Music\\Album"] });
    });
    expect(open).toHaveBeenCalledWith(expect.objectContaining({ directory: true, multiple: true }));
    expect(invoke).not.toHaveBeenCalledWith("add_directory", expect.anything());
    expect(invoke).not.toHaveBeenCalledWith("scan_directories", expect.anything());
  });

  it("opens the audio file picker from Open Files", async () => {
    vi.mocked(open).mockResolvedValueOnce(null);
    render(LibraryWelcome);

    await fireEvent.click(screen.getByRole("button", { name: /Open Files/ }));

    await waitFor(() => {
      expect(open).toHaveBeenCalledWith(expect.objectContaining({ directory: false, multiple: true }));
    });
  });
});
