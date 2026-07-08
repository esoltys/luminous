import { describe, it, expect, beforeEach, vi } from "vitest";
import { PlayerStore } from "./player.svelte";
import { invoke } from "@tauri-apps/api/core";

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
    await store.setShuffleMode("on");
    expect(store.shuffleMode).toBe("on");
    expect(invoke).toHaveBeenCalledWith("set_shuffle_mode", { mode: "on" });
  });

  it("should update repeat mode locally and invoke set_repeat_mode", async () => {
    await store.setRepeatMode("one");
    expect(store.repeatMode).toBe("one");
    expect(invoke).toHaveBeenCalledWith("set_repeat_mode", { mode: "one" });
  });

  it("should update position and invoke seek_to on seek", async () => {
    await store.seek(1500.5);
    expect(store.positionNanosec).toBe(1501); // rounded
    expect(invoke).toHaveBeenCalledWith("seek_to", { positionNanosec: 1501 });
  });
});
