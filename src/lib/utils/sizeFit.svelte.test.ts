import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { flushSync } from "svelte";
import { invoke } from "@tauri-apps/api/core";
import { PlaylistSizeLimit, type SizeLimitSource } from "./sizeFit.svelte";
import type { Playlist, PlaylistItem, PlaylistSizeFit } from "../types";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn().mockResolvedValue(() => {}) }));

const items = ["a", "b"].map((uuid, i) => ({ id: i, playlist_id: 7, position: i, item_type: "song", uuid }) as PlaylistItem);
const playlist = (over: Partial<Playlist> = {}): Playlist => ({
  id: 7,
  name: "Run",
  track_count: 2,
  created: 0,
  updated: 0,
  dynamic_enabled: false,
  is_queue: false,
  size_limit_bytes: 10_000_000_000,
  ...over,
});
const answer = (over: Partial<PlaylistSizeFit> = {}): PlaylistSizeFit => ({
  limit_bytes: 10_000_000_000,
  break_index: 1,
  fit_songs: 1,
  fit_bytes: 6_000_000_000,
  total_bytes: 12_000_000_000,
  order: ["a", "b"],
  device_id: null,
  device_name: null,
  ...over,
});

/** Replies to `get_playlist_size_fit` calls in whatever order a test resolves them. */
let pending: ((fit: PlaylistSizeFit | null) => void)[] = [];
let cleanups: (() => void)[] = [];

function track(source: SizeLimitSource) {
  const state = $state(source);
  let tracker!: PlaylistSizeLimit;
  cleanups.push(
    $effect.root(() => {
      tracker = new PlaylistSizeLimit(() => state);
    })
  );
  flushSync();
  return { tracker, state };
}

const settle = () => new Promise((resolve) => setTimeout(resolve));

describe("PlaylistSizeLimit (#344)", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    pending = [];
    vi.mocked(invoke).mockImplementation(() => new Promise((resolve) => pending.push(resolve as never)));
  });

  afterEach(() => {
    cleanups.forEach((c) => c());
    cleanups = [];
  });

  it("keeps the latest measurement when an older, slower one lands after it", async () => {
    const { tracker, state } = track({ playlist: playlist({ size_limit_device: "vol-1" }), items });
    state.playlist = playlist();
    flushSync();
    expect(pending).toHaveLength(2);

    pending[1](answer());
    pending[0](answer({ device_id: "vol-1", device_name: "Gym" }));
    await settle();

    expect(tracker.fit?.device_id).toBeNull();
    expect(tracker.status).toBe("6 GB of 10 GB used");
  });

  it("shows no answer for another limit or device than the one set", async () => {
    const { tracker, state } = track({ playlist: playlist(), items });
    pending[0](answer());
    await settle();
    expect(tracker.breakBefore(["a", "b"])).toBe("b");

    state.playlist = playlist({ size_limit_bytes: 20_000_000_000 });
    expect(tracker.fit).toBeNull();
    expect(tracker.status).toBe("Measuring sizes…");

    state.playlist = playlist({ size_limit_device: "vol-1" });
    expect(tracker.fit).toBeNull();
  });

  it("names an unplugged device from the answer measured for it", async () => {
    const { tracker } = track({ playlist: playlist({ size_limit_device: "vol-1" }), items });
    pending[0](answer({ device_id: "vol-1", device_name: null }));
    await settle();
    expect(tracker.deviceName).toBeNull();
    expect(tracker.status).toBe("6 GB of 10 GB used");
  });

  it("measures nothing without a limit", () => {
    const { tracker } = track({ playlist: playlist({ size_limit_bytes: null }), items });
    expect(invoke).not.toHaveBeenCalled();
    expect(tracker.status).toBeNull();
  });
});
