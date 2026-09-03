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
    vi.mocked(invoke).mockResolvedValue(null);
    await picardStore.init();
    await picardStore.init();
    await picardStore.init();
    expect(invoke).toHaveBeenCalledTimes(1);
  });

  it("refresh() re-checks even after init()", async () => {
    vi.mocked(invoke).mockResolvedValueOnce(null).mockResolvedValueOnce("/usr/bin/picard");
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
});
