import { describe, it, expect, beforeEach, afterEach, vi } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { OpenerStore } from "./opener.svelte";
import { playlistsStore } from "./playlists.svelte";
import { playerStore } from "./player.svelte";
import { toastStore } from "./toast.svelte";

vi.mock("@tauri-apps/api/webview", () => ({
  getCurrentWebview: vi.fn(),
}));

vi.mock("@tauri-apps/plugin-dialog", () => ({
  open: vi.fn(),
}));

describe("OpenerStore", () => {
  let store: OpenerStore;
  const baseInvoke = vi.mocked(invoke).getMockImplementation();

  beforeEach(() => {
    vi.clearAllMocks();
    if (baseInvoke) vi.mocked(invoke).mockImplementation(baseInvoke);
    store = new OpenerStore();
  });

  afterEach(() => {
    store.dispose();
  });

  it("should trigger open_and_play invoke on openAndPlay and return its outcome", async () => {
    const testPaths = ["/path/to/song.mp3", "/path/to/playlist.m3u"];
    const outcome = await store.openAndPlay(testPaths);
    expect(invoke).toHaveBeenCalledWith("open_and_play", { paths: testPaths });
    expect(outcome).toEqual({ played: 1, skipped: 0, station_lists: [] });
  });

  it("clears completedSession and sets Queue context on openAndPlay", async () => {
    playerStore.completedSession = {
      contextName: "Queue",
      isQueue: true,
      songIds: [1],
      trackCount: 1,
    };

    await store.openAndPlay(["/path/to/song.mp3"]);

    expect(playerStore.completedSession).toBeNull();
    expect(playerStore.activeContextName).toBe("Queue");
  });

  it("should trigger add_paths_to_queue invoke on addPathsToQueue and return its outcome", async () => {
    const testPaths = ["/path/to/song.mp3", "/path/to/dropped-folder"];
    const outcome = await store.addPathsToQueue(testPaths);
    expect(invoke).toHaveBeenCalledWith("add_paths_to_queue", { paths: testPaths });
    expect(outcome).toEqual({ added: 1, skipped: 0, station_lists: [] });
  });

  it("should refresh the currently-viewed Queue track list after addPathsToQueue", async () => {
    await playlistsStore.refreshPlaylists();
    const queueId = playlistsStore.queuePlaylist!.id;
    playlistsStore.activePlaylistId = queueId;
    vi.mocked(invoke).mockClear();

    await store.addPathsToQueue(["/path/to/song.mp3"]);

    // Without this, the Queue view only shows newly-added tracks after
    // navigating away and back — refreshPlaylists() alone only updates
    // playlist metadata/counts, not the currently-displayed track list.
    expect(invoke).toHaveBeenCalledWith("get_playlist_tracks", { playlistId: queueId });
  });

  describe("station-list playlists (#2279)", () => {
    const somaFm = { path: "G:/Playlists/soma_fm.m3u", name: "soma_fm", station_count: 43 };

    it("offers an opened station list for import instead of reporting nothing playable", async () => {
      vi.mocked(invoke).mockResolvedValueOnce({ played: 0, skipped: 0, station_lists: [somaFm] });
      const showSpy = vi.spyOn(toastStore, "show");

      await store.openAndPlay([somaFm.path]);

      expect(store.pendingStationLists).toEqual([somaFm]);
      expect(showSpy).not.toHaveBeenCalled();
      showSpy.mockRestore();
    });

    it("offers a Shift-dropped station list too", async () => {
      vi.mocked(invoke).mockResolvedValueOnce({ added: 0, skipped: 0, station_lists: [somaFm] });
      const showSpy = vi.spyOn(toastStore, "show");

      await store.addPathsToQueue([somaFm.path]);

      expect(store.pendingStationLists).toEqual([somaFm]);
      expect(showSpy).not.toHaveBeenCalled();
      showSpy.mockRestore();
    });

    it("still reports nothing playable when there is no station list either", async () => {
      vi.mocked(invoke).mockResolvedValueOnce({ played: 0, skipped: 1, station_lists: [] });
      const showSpy = vi.spyOn(toastStore, "show");

      await store.openAndPlay(["/path/to/readme.txt"]);

      expect(store.pendingStationLists).toEqual([]);
      expect(showSpy).toHaveBeenCalledWith("No supported audio files found to play.", "error");
      showSpy.mockRestore();
    });

    it("takePendingStationLists hands back the offer once and clears it", async () => {
      vi.mocked(invoke).mockResolvedValueOnce({ played: 0, skipped: 0, station_lists: [somaFm] });
      await store.openAndPlay([somaFm.path]);

      expect(store.takePendingStationLists()).toEqual([somaFm]);
      expect(store.pendingStationLists).toEqual([]);
      expect(store.takePendingStationLists()).toEqual([]);
    });
  });

  describe("openFileDialog & openFolderDialog", () => {
    it("fetches supported extensions from the backend once and passes them to the file dialog", async () => {
      const exts = { audio: ["mp3", "flac"], playlists: ["m3u", "pls"] };
      vi.mocked(invoke).mockImplementation(async (cmd: string, args?: unknown) => {
        if (cmd === "get_supported_extensions") return exts;
        return baseInvoke?.(cmd, args as never) ?? null;
      });
      vi.mocked(open).mockResolvedValueOnce(["/music/track.flac"]);

      await store.openFileDialog();
      expect(open).toHaveBeenCalledWith(
        expect.objectContaining({
          multiple: true,
          directory: false,
          filters: [
            expect.objectContaining({ extensions: ["mp3", "flac", "m3u", "pls"] }),
            expect.objectContaining({ extensions: ["mp3", "flac"] }),
            expect.objectContaining({ extensions: ["m3u", "pls"] }),
          ],
        }),
      );
      expect(invoke).toHaveBeenCalledWith("open_and_play", { paths: ["/music/track.flac"] });

      // Second open reuses the cached extension lists without another IPC call.
      vi.mocked(open).mockResolvedValueOnce(null);
      await store.openFileDialog();
      const extCalls = vi
        .mocked(invoke)
        .mock.calls.filter(([cmd]) => cmd === "get_supported_extensions");
      expect(extCalls).toHaveLength(1);
    });

    it("opens selected folder(s) via openFolderDialog", async () => {
      vi.mocked(open).mockResolvedValueOnce(["/music/Album"]);

      await store.openFolderDialog();

      expect(open).toHaveBeenCalledWith(
        expect.objectContaining({ multiple: true, directory: true }),
      );
      expect(invoke).toHaveBeenCalledWith("open_and_play", { paths: ["/music/Album"] });
    });
  });

  describe("drag-and-drop", () => {
    it("tracks drag hover state and routes plain vs. Shift drops", async () => {
      type DragEvent =
        | { payload: { type: "enter" | "over"; paths: string[] } }
        | { payload: { type: "leave" } }
        | { payload: { type: "drop"; paths: string[] } };
      let dragHandler: ((event: DragEvent) => void) | undefined;
      const unlisten = vi.fn();
      vi.mocked(getCurrentWebview).mockReturnValue({
        onDragDropEvent: vi.fn(async (cb: (event: DragEvent) => void) => {
          dragHandler = cb;
          return unlisten;
        }),
      } as never);

      let shiftHeld = false;
      vi.mocked(invoke).mockImplementation(async (cmd: string, args?: unknown) => {
        if (cmd === "is_shift_key_held") return shiftHeld;
        if (cmd === "open_and_play") return { played: 2, skipped: 0, station_lists: [] };
        if (cmd === "add_paths_to_queue") return { added: 1, skipped: 0, station_lists: [] };
        return baseInvoke?.(cmd, args as never) ?? null;
      });
      const showSpy = vi.spyOn(toastStore, "show");

      const detach = store.attachDragDrop();
      await new Promise((resolve) => setTimeout(resolve, 0));
      expect(dragHandler).toBeDefined();

      // Enter sets isDragActive; leave clears it.
      dragHandler!({ payload: { type: "enter", paths: [] } });
      expect(store.isDragActive).toBe(true);
      dragHandler!({ payload: { type: "leave" } });
      expect(store.isDragActive).toBe(false);

      // Plain drop replaces queue & plays.
      dragHandler!({ payload: { type: "drop", paths: ["/a.mp3", "/b.mp3"] } });
      await new Promise((resolve) => setTimeout(resolve, 0));
      expect(invoke).toHaveBeenCalledWith("open_and_play", { paths: ["/a.mp3", "/b.mp3"] });
      expect(showSpy).toHaveBeenCalledWith(expect.any(String), "success");

      // Shift-drop appends to queue.
      shiftHeld = true;
      dragHandler!({ payload: { type: "drop", paths: ["/c.mp3"] } });
      await new Promise((resolve) => setTimeout(resolve, 0));
      expect(invoke).toHaveBeenCalledWith("add_paths_to_queue", { paths: ["/c.mp3"] });

      detach();
      expect(unlisten).toHaveBeenCalled();
      expect(store.isDragActive).toBe(false);
      expect(store.isShiftHeld).toBe(false);
      showSpy.mockRestore();
    });
  });
});
