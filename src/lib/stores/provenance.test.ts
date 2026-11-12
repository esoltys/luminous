import { describe, it, expect, vi, beforeEach } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { ProvenanceStore } from "./provenance.svelte";
import { tasksStore } from "./tasks.svelte";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
}));

vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn().mockResolvedValue(() => {}),
}));

describe("ProvenanceStore", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it("loads cached provenance verdicts on initialization", async () => {
    vi.mocked(invoke).mockImplementation(async (cmd: string) => {
      if (cmd === "get_provenance_verdicts") {
        return { "42": "authentic", "101": "suspected_lossy" };
      }
      return null;
    });

    const store = new ProvenanceStore();
    await store.init();

    expect(store.isVerified(42)).toBe(true);
    expect(store.getVerdict(42)).toBe("authentic");
    expect(store.getVerdict(101)).toBe("suspected_lossy");
    expect(store.isVerified(999)).toBe(false);
  });

  it("verifySongs starts task progress notification and completes with age out", async () => {
    vi.mocked(invoke).mockImplementation(async (cmd: string) => {
      if (cmd === "analyze_songs_provenance") {
        return 3;
      }
      return null;
    });

    const startSpy = vi.spyOn(tasksStore, "startTask");
    const completeSpy = vi.spyOn(tasksStore, "completeTask");

    const store = new ProvenanceStore();
    await store.verifySongs([1, 2, 3], "Test Album");

    expect(startSpy).toHaveBeenCalledWith(
      expect.objectContaining({
        total: 3,
        contextName: "Test Album",
      }),
    );

    expect(completeSpy).toHaveBeenCalledWith(
      expect.stringContaining("provenance-"),
      "Audio authenticity verification complete",
      4000,
    );
  });
});
