import { describe, it, expect, beforeEach, vi } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import { picardStore } from "./picard.svelte";

describe("PicardStore", () => {
  beforeEach(() => {
    (picardStore as any).initialized = false;
    picardStore.path = null;
    vi.mocked(invoke).mockReset();
  });

  it("is unavailable before init/refresh", () => {
    expect(picardStore.available).toBe(false);
    expect(picardStore.path).toBeNull();
  });

  it("becomes available once the backend resolves a path", async () => {
    vi.mocked(invoke).mockResolvedValue(String.raw`C:\Program Files\MusicBrainz Picard\picard.exe`);
    await picardStore.init();
    expect(invoke).toHaveBeenCalledWith("get_picard_path");
    expect(picardStore.available).toBe(true);
    expect(picardStore.path).toBe(String.raw`C:\Program Files\MusicBrainz Picard\picard.exe`);
  });

  it("stays unavailable when the backend finds nothing", async () => {
    vi.mocked(invoke).mockResolvedValue(null);
    await picardStore.init();
    expect(picardStore.available).toBe(false);
    expect(picardStore.path).toBeNull();
  });

  it("only calls the backend once across repeated init() calls", async () => {
    vi.mocked(invoke).mockImplementation(async (cmd: string) => {
      if (cmd === "get_picard_path") return null;
      if (cmd === "get_all_app_settings") return {};
      return null;
    });
    await picardStore.init();
    await picardStore.init();
    await picardStore.init();
    const picardCalls = vi.mocked(invoke).mock.calls.filter(([cmd]) => cmd === "get_picard_path");
    expect(picardCalls).toHaveLength(1);
  });

  it("refresh() re-checks even after init()", async () => {
    let callCount = 0;
    vi.mocked(invoke).mockImplementation(async (cmd: string) => {
      if (cmd === "get_picard_path") {
        callCount++;
        return callCount === 1 ? null : "/usr/bin/picard";
      }
      if (cmd === "get_all_app_settings") return {};
      return null;
    });
    await picardStore.init();
    expect(picardStore.available).toBe(false);
    await picardStore.refresh();
    expect(picardStore.available).toBe(true);
    expect(picardStore.path).toBe("/usr/bin/picard");
  });

  it("falls back to unavailable if the backend call throws", async () => {
    vi.mocked(invoke).mockRejectedValue("boom");
    await picardStore.init();
    expect(picardStore.available).toBe(false);
    expect(picardStore.path).toBeNull();
  });

  it("defaults missingPlaylistEnabled to true and updates via setMissingPlaylistEnabled", async () => {
    expect(picardStore.missingPlaylistEnabled).toBe(true);
    await picardStore.setMissingPlaylistEnabled(false);
    expect(picardStore.missingPlaylistEnabled).toBe(false);
    expect(invoke).toHaveBeenCalledWith("set_app_setting", {
      key: "picard_missing_playlist_enabled",
      value: "false",
    });
  });

  it("loads missingPlaylistEnabled from app settings on refresh", async () => {
    vi.mocked(invoke).mockImplementation(async (cmd: string) => {
      if (cmd === "get_picard_path") return null;
      if (cmd === "get_all_app_settings") return { picard_missing_playlist_enabled: "false" };
      return null;
    });
    await picardStore.refresh();
    expect(picardStore.missingPlaylistEnabled).toBe(false);
  });
});
