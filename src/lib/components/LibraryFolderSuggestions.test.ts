import "@testing-library/jest-dom";
import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, waitFor, fireEvent } from "@testing-library/svelte";
import { invoke } from "@tauri-apps/api/core";
import LibraryFolderSuggestions from "./LibraryFolderSuggestions.svelte";
import { collectionStore } from "../stores/collection.svelte";
import type { SuggestedFolder } from "../types";

// Captures listen() callbacks by event name so tests can fire them the way the
// backend would emit them.
const { listenCallbacks } = vi.hoisted(() => ({
  listenCallbacks: {} as Record<string, (event: { payload: unknown }) => void>,
}));

vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn((event: string, callback: (e: { payload: unknown }) => void) => {
    listenCallbacks[event] = callback;
    return Promise.resolve(() => {});
  }),
}));

let mockFolders: SuggestedFolder[] = [];

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn((cmd: string) => {
    if (cmd === "get_suggested_library_folders") return Promise.resolve(mockFolders);
    // collectionStore initializes itself on module load and expects this shape.
    if (cmd === "get_library_snapshot") return Promise.resolve({ songs: [], albums: [], artists: [] });
    return Promise.resolve([]);
  }),
}));

const MUSIC: SuggestedFolder = { path: "E:\\Music\\Jazz", song_count: 3, last_played: 100 };
const PODCASTS: SuggestedFolder = { path: "/home/me/Tunes", song_count: 1, last_played: null };

describe("LibraryFolderSuggestions.svelte", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mockFolders = [];
    for (const key of Object.keys(listenCallbacks)) delete listenCallbacks[key];
  });

  it("renders nothing when there are no suggestions", async () => {
    const { container } = render(LibraryFolderSuggestions);
    await waitFor(() =>
      expect(invoke).toHaveBeenCalledWith("get_suggested_library_folders", { limit: 3 }),
    );
    expect(container.querySelector("section")).toBeNull();
  });

  it("lists each folder with its name, path and played-song count", async () => {
    mockFolders = [MUSIC, PODCASTS];
    render(LibraryFolderSuggestions);

    expect(await screen.findByText("Jazz")).toBeInTheDocument();
    expect(screen.getByText("E:\\Music\\Jazz")).toBeInTheDocument();
    expect(screen.getByText("3 songs played")).toBeInTheDocument();
    expect(screen.getByText("Tunes")).toBeInTheDocument();
    expect(screen.getByText("1 song played")).toBeInTheDocument();
  });

  it("adds the chosen folder through the collection store", async () => {
    mockFolders = [MUSIC, PODCASTS];
    const addDirectory = vi.spyOn(collectionStore, "addDirectory").mockResolvedValue(undefined);
    render(LibraryFolderSuggestions);

    const addButton = await screen.findByRole("button", { name: "Add Tunes to your library" });
    // Adding makes the folder watched, so the backend stops returning it.
    mockFolders = [MUSIC];
    await fireEvent.click(addButton);
    expect(addDirectory).toHaveBeenCalledWith("/home/me/Tunes");

    // Once the add resolves the card asks the backend again, with no event needed.
    await waitFor(() => expect(screen.queryByText("Tunes")).not.toBeInTheDocument());
    expect(screen.getByText("Jazz")).toBeInTheDocument();
  });

  it("dismisses one folder, then refetches only when the backend says it changed", async () => {
    mockFolders = [MUSIC, PODCASTS];
    render(LibraryFolderSuggestions);

    await fireEvent.click(await screen.findByRole("button", { name: "Not now for Jazz" }));
    expect(invoke).toHaveBeenCalledWith("dismiss_library_folder_suggestion", {
      path: "E:\\Music\\Jazz",
    });
    // Nothing is removed optimistically.
    expect(screen.getByText("Jazz")).toBeInTheDocument();

    mockFolders = [PODCASTS];
    listenCallbacks["library-folder-suggestions-changed"]({ payload: null });
    await waitFor(() => expect(screen.queryByText("Jazz")).not.toBeInTheDocument());
    expect(screen.getByText("Tunes")).toBeInTheDocument();
  });

  it("hides every suggestion on request", async () => {
    mockFolders = [MUSIC];
    render(LibraryFolderSuggestions);

    await fireEvent.click(await screen.findByRole("button", { name: "Hide suggestions" }));
    expect(invoke).toHaveBeenCalledWith("hide_library_folder_suggestions");

    mockFolders = [];
    listenCallbacks["library-folder-suggestions-changed"]({ payload: null });
    await waitFor(() => expect(screen.queryByText("Jazz")).not.toBeInTheDocument());
  });

  it("refetches when the library or the connected drives change", async () => {
    vi.useFakeTimers();
    try {
      mockFolders = [MUSIC];
      render(LibraryFolderSuggestions);
      await vi.advanceTimersByTimeAsync(0);
      const calls = () =>
        vi.mocked(invoke).mock.calls.filter(([cmd]) => cmd === "get_suggested_library_folders").length;
      const initial = calls();

      listenCallbacks["library-changed"]({ payload: null });
      await vi.advanceTimersByTimeAsync(600);
      expect(calls()).toBe(initial + 1);

      listenCallbacks["devices-changed"]({ payload: [] });
      await vi.advanceTimersByTimeAsync(600);
      expect(calls()).toBe(initial + 2);
    } finally {
      vi.useRealTimers();
    }
  });
});
