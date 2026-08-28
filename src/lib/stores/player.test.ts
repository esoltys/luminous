import { describe, it, expect, beforeEach, vi } from "vitest";
import { PlayerStore } from "./player.svelte";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { playlistsStore } from "./playlists.svelte";

describe("PlayerStore", () => {
  let store: PlayerStore;

  beforeEach(() => {
    vi.clearAllMocks();
    store = new PlayerStore();
  });

  it("should initialize with correct default state from Tauri backend", async () => {
    // Wait for the async init to complete
    await new Promise((resolve) => setTimeout(resolve, 50));

    expect(store.state).toBe("stopped");
    expect(store.currentSong).toBeNull();
    expect(store.volume).toBe(1.0);
    expect(store.shuffleMode).toBe("off");
    expect(store.repeatMode).toBe("off");
    expect(invoke).toHaveBeenCalledWith("get_playback_state");
  });

  it("should trigger play_song invoke on playSong", async () => {
    await store.playSong(42);
    expect(invoke).toHaveBeenCalledWith("play_song", { songId: 42 });
  });

  it("should trigger pause invoke on pause", async () => {
    await store.pause();
    expect(invoke).toHaveBeenCalledWith("pause");
  });

  it("should trigger resume invoke on resume", async () => {
    await store.resume();
    expect(invoke).toHaveBeenCalledWith("resume");
  });

  it("should pause when togglePlayPause is called while playing", async () => {
    store.state = "playing";

    await store.togglePlayPause();

    expect(invoke).toHaveBeenCalledWith("pause");
  });

  it("should resume when togglePlayPause is called while paused or stopped", async () => {
    store.state = "paused";

    await store.togglePlayPause();

    expect(invoke).toHaveBeenCalledWith("resume");
  });

  it("should trigger stop invoke on stop", async () => {
    await store.stop();
    expect(invoke).toHaveBeenCalledWith("stop");
  });

  it("should trigger next_track invoke on next", async () => {
    await store.next();
    expect(invoke).toHaveBeenCalledWith("next_track");
  });

  it("should trigger previous_track invoke on previous", async () => {
    await store.previous();
    expect(invoke).toHaveBeenCalledWith("previous_track");
  });

  it("should update volume locally and invoke set_volume on setVolume", async () => {
    await store.setVolume(0.75);
    expect(store.volume).toBe(0.75);
    expect(invoke).toHaveBeenCalledWith("set_volume", { volume: 0.75 });
  });

  it("should update shuffle mode locally and invoke set_shuffle_mode", async () => {
    await store.setShuffleMode("all");
    expect(store.shuffleMode).toBe("all");
    expect(invoke).toHaveBeenCalledWith("set_shuffle_mode", { mode: "all" });
  });

  it("should update repeat mode locally and invoke set_repeat_mode", async () => {
    await store.setRepeatMode("track");
    expect(store.repeatMode).toBe("track");
    expect(invoke).toHaveBeenCalledWith("set_repeat_mode", { mode: "track" });
  });

  it("should update position and invoke seek_to on seek", async () => {
    await store.seek(1500.5);
    expect(store.positionNanosec).toBe(1501); // rounded
    expect(invoke).toHaveBeenCalledWith("seek_to", { positionNanosec: 1501 });
  });

  it("should clamp relative seeking to the start and current song duration", async () => {
    store.currentSong = { length_nanosec: 30_000_000_000 } as any;
    store.positionNanosec = 25_000_000_000;

    await store.seekRelative(10_000_000_000);
    expect(invoke).toHaveBeenCalledWith("seek_to", { positionNanosec: 30_000_000_000 });

    await store.seekRelative(-40_000_000_000);
    expect(invoke).toHaveBeenCalledWith("seek_to", { positionNanosec: 0 });
  });

  it("should clamp adjusted volume between muted and full volume", async () => {
    store.volume = 0.98;

    await store.adjustVolume(0.05);
    expect(store.volume).toBe(1);
    expect(invoke).toHaveBeenCalledWith("set_volume", { volume: 1 });

    await store.adjustVolume(-1.5);
    expect(store.volume).toBe(0);
    expect(invoke).toHaveBeenCalledWith("set_volume", { volume: 0 });
  });

  it("should trigger open_and_play invoke on openAndPlay and return its outcome", async () => {
    const testPaths = ["/path/to/song.mp3", "/path/to/playlist.m3u"];
    const outcome = await store.openAndPlay(testPaths);
    expect(invoke).toHaveBeenCalledWith("open_and_play", { paths: testPaths });
    expect(outcome).toEqual({ played: 1, skipped: 0 });
  });

  it("should trigger add_paths_to_queue invoke on addPathsToQueue and return its outcome", async () => {
    const testPaths = ["/path/to/song.mp3", "/path/to/dropped-folder"];
    const outcome = await store.addPathsToQueue(testPaths);
    expect(invoke).toHaveBeenCalledWith("add_paths_to_queue", { paths: testPaths });
    expect(outcome).toEqual({ added: 1, skipped: 0 });
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

  it("should clear currentSong when track-changed reports no song", async () => {
    const originalListenImpl = vi.mocked(listen).getMockImplementation();
    let trackChangedCallback: ((event: { payload: { song: unknown } }) => void) | undefined;
    vi.mocked(listen).mockImplementation(async (event: string, callback: any) => {
      if (event === "track-changed") trackChangedCallback = callback;
      return () => {};
    });

    store = new PlayerStore();
    await new Promise((resolve) => setTimeout(resolve, 50));

    expect(store.currentSong).toBeFalsy();

    trackChangedCallback?.({ payload: { song: { id: 1, title: "Test Song" } } });
    expect(store.currentSong).toEqual({ id: 1, title: "Test Song" });

    trackChangedCallback?.({ payload: { song: null } });
    expect(store.currentSong).toBeUndefined();

    if (originalListenImpl) vi.mocked(listen).mockImplementation(originalListenImpl);
  });

  it("should clear the Queue playlist when queue playback naturally completes", async () => {
    const originalListenImpl = vi.mocked(listen).getMockImplementation();
    let playbackStateCallback: ((event: { payload: any }) => Promise<void>) | undefined;
    vi.mocked(listen).mockImplementation(async (event: string, callback: any) => {
      if (event === "playback-state") playbackStateCallback = callback;
      return () => {};
    });

    store = new PlayerStore();
    await new Promise((resolve) => setTimeout(resolve, 50));

    await playlistsStore.refreshPlaylists();
    const queueId = playlistsStore.queuePlaylist!.id;
    vi.mocked(invoke).mockClear();

    // Simulate active playing state on Queue
    await playbackStateCallback?.({
      payload: {
        state: "playing",
        current_song: { id: 1, title: "Last Track" },
        playlist_id: queueId,
        playlist_item_uuid: "uuid-123",
        remaining_playlist_items: 0,
        position_nanosec: 1000,
        volume: 1,
        shuffle_mode: "off",
        repeat_mode: "off",
      },
    });

    vi.mocked(invoke).mockClear();

    // Simulate natural stop when last track completes
    await playbackStateCallback?.({
      payload: {
        state: "stopped",
        current_song: null,
        playlist_id: null,
        playlist_item_uuid: null,
        remaining_playlist_items: 0,
        position_nanosec: 0,
        volume: 1,
        shuffle_mode: "off",
        repeat_mode: "off",
      },
    });

    expect(invoke).toHaveBeenCalledWith("clear_playlist", { playlistId: queueId });

    if (originalListenImpl) vi.mocked(listen).mockImplementation(originalListenImpl);
  });

  it("should not clear custom playlists when their playback completes", async () => {
    const originalListenImpl = vi.mocked(listen).getMockImplementation();
    let playbackStateCallback: ((event: { payload: any }) => Promise<void>) | undefined;
    vi.mocked(listen).mockImplementation(async (event: string, callback: any) => {
      if (event === "playback-state") playbackStateCallback = callback;
      return () => {};
    });

    playlistsStore.playlists = [
      { id: 1, name: "Queue", dynamic_enabled: false, created: 0, updated: 0, track_count: 0, is_queue: true },
      { id: 2, name: "Rock Classics", dynamic_enabled: false, created: 0, updated: 0, track_count: 5, is_queue: false },
    ];
    const customPl = playlistsStore.playlists.find((p) => !p.is_queue);
    expect(customPl).toBeDefined();
    const customId = customPl!.id;
    vi.mocked(invoke).mockClear();

    // Simulate active playing state on Custom Playlist
    await playbackStateCallback?.({
      payload: {
        state: "playing",
        current_song: { id: 1, title: "Last Track" },
        playlist_id: customId,
        playlist_item_uuid: "uuid-456",
        remaining_playlist_items: 0,
        position_nanosec: 1000,
        volume: 1,
        shuffle_mode: "off",
        repeat_mode: "off",
      },
    });

    vi.mocked(invoke).mockClear();

    // Simulate natural stop when last track completes
    await playbackStateCallback?.({
      payload: {
        state: "stopped",
        current_song: null,
        playlist_id: null,
        playlist_item_uuid: null,
        remaining_playlist_items: 0,
        position_nanosec: 0,
        volume: 1,
        shuffle_mode: "off",
        repeat_mode: "off",
      },
    });

    expect(invoke).not.toHaveBeenCalledWith("clear_playlist", { playlistId: customId });

    if (originalListenImpl) vi.mocked(listen).mockImplementation(originalListenImpl);
  });

  it("should not trim queue tracks before uuid when shuffle_mode is enabled", async () => {
    const originalListenImpl = vi.mocked(listen).getMockImplementation();
    let playbackStateCallback: ((event: { payload: any }) => Promise<void>) | undefined;
    vi.mocked(listen).mockImplementation(async (event: string, callback: any) => {
      if (event === "playback-state") playbackStateCallback = callback;
      return () => {};
    });

    store = new PlayerStore();
    await new Promise((resolve) => setTimeout(resolve, 50));

    await playlistsStore.refreshPlaylists();
    const queueId = playlistsStore.queuePlaylist!.id;
    vi.mocked(invoke).mockClear();

    // Trigger playback-state with shuffle_mode: "all"
    await playbackStateCallback?.({
      payload: {
        state: "playing",
        current_song: { id: 2, title: "Shuffled Track" },
        playlist_id: queueId,
        playlist_item_uuid: "uuid-shuffled-2",
        remaining_playlist_items: 4,
        position_nanosec: 500,
        volume: 1,
        shuffle_mode: "all",
        repeat_mode: "off",
      },
    });

    expect(invoke).not.toHaveBeenCalledWith("trim_playlist_before_uuid", expect.anything());

    if (originalListenImpl) vi.mocked(listen).mockImplementation(originalListenImpl);
  });

  it("should trim queue tracks before uuid when shuffle_mode is off", async () => {
    const originalListenImpl = vi.mocked(listen).getMockImplementation();
    let playbackStateCallback: ((event: { payload: any }) => Promise<void>) | undefined;
    vi.mocked(listen).mockImplementation(async (event: string, callback: any) => {
      if (event === "playback-state") playbackStateCallback = callback;
      return () => {};
    });

    store = new PlayerStore();
    await new Promise((resolve) => setTimeout(resolve, 50));

    await playlistsStore.refreshPlaylists();
    const queueId = playlistsStore.queuePlaylist!.id;
    vi.mocked(invoke).mockClear();

    // Trigger playback-state with shuffle_mode: "off"
    await playbackStateCallback?.({
      payload: {
        state: "playing",
        current_song: { id: 2, title: "Sequential Track" },
        playlist_id: queueId,
        playlist_item_uuid: "uuid-seq-2",
        remaining_playlist_items: 4,
        position_nanosec: 500,
        volume: 1,
        shuffle_mode: "off",
        repeat_mode: "off",
      },
    });

    expect(invoke).toHaveBeenCalledWith("trim_playlist_before_uuid", {
      playlistId: queueId,
      uuid: "uuid-seq-2",
    });

    if (originalListenImpl) vi.mocked(listen).mockImplementation(originalListenImpl);
  });
});
