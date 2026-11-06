import { describe, it, expect, vi, beforeEach } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import { tagCleanupStore } from "./tagCleanup.svelte";
import { tasksStore } from "./tasks.svelte";
import { toastStore } from "./toast.svelte";
import { navigationStore } from "./navigation.svelte";
import type { TagCleanupPreview } from "../types";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn().mockResolvedValue(() => {}) }));

const found: TagCleanupPreview = {
  songs: [{ song_id: 1, title: "A", artist: "B", fields: [{ kind: "encoder", value: "LAME" }] }],
  skipped: 0,
};

/** Starts a library check whose result the test settles. */
async function startCheck() {
  let settle!: { resolve: (p: TagCleanupPreview) => void; reject: (e: unknown) => void };
  vi.mocked(invoke).mockImplementation((cmd) =>
    cmd === "preview_tag_cleanup"
      ? new Promise((resolve, reject) => { settle = { resolve, reject }; })
      : Promise.resolve(undefined),
  );
  tagCleanupStore.start();
  await vi.waitFor(() => expect(invoke).toHaveBeenCalledWith("preview_tag_cleanup", { songIds: [] }));
  return settle;
}

const settled = () => vi.waitFor(() => expect(tagCleanupStore.checking).toBe(false));

describe("tagCleanupStore", () => {
  beforeEach(() => {
    vi.mocked(invoke).mockReset();
    toastStore.messages = [];
    tasksStore.tasks = [];
    tagCleanupStore.reviewOpen = false;
  });

  it("keeps checking after the review closes and offers it when done", async () => {
    const check = await startCheck();
    tagCleanupStore.close();
    expect(tasksStore.isTaskActive("tag-cleanup-check")).toBe(true);

    check.resolve(found);
    await settled();
    expect(tasksStore.isTaskActive("tag-cleanup-check")).toBe(false);
    const toast = toastStore.messages.find((m) => m.action);
    expect(toast?.text).toBe("Tag check done. Tracks with clutter: 1");

    navigationStore.activeTab = "collection";
    toast!.action!.onClick();
    expect(navigationStore.activeTab).toBe("organize");
    expect(tagCleanupStore.reviewOpen).toBe(true);
    expect(tagCleanupStore.preview).toEqual(found);
  });

  it("shows the findings in the open review without a toast", async () => {
    const check = await startCheck();
    check.resolve(found);
    await settled();
    expect(tagCleanupStore.preview).toEqual(found);
    expect(toastStore.messages).toHaveLength(0);
  });

  it("removes in the background and reports when done", async () => {
    vi.mocked(invoke).mockResolvedValue({ cleaned_songs: 2, removed_fields: 3, errors: [] });
    tagCleanupStore.reviewOpen = true;
    const removal = tagCleanupStore.remove([{ song_id: 1, fields: [{ kind: "encoder", value: "LAME" }] }]);
    expect(tagCleanupStore.reviewOpen).toBe(false);
    expect(tasksStore.isTaskActive("tag-cleanup-remove")).toBe(true);

    await removal;
    expect(invoke).toHaveBeenCalledWith("apply_tag_cleanup", {
      selections: [{ song_id: 1, fields: [{ kind: "encoder", value: "LAME" }] }],
    });
    expect(tasksStore.tasks.find((t) => t.id === "tag-cleanup-remove")?.status).toBe("done");
    expect(tagCleanupStore.removing).toBe(false);
  });

  it("stops the check on cancel without reporting a failure", async () => {
    const check = await startCheck();
    tagCleanupStore.cancel();
    expect(invoke).toHaveBeenCalledWith("cancel_tag_cleanup");
    expect(tagCleanupStore.reviewOpen).toBe(false);

    check.reject("Tag clean-up preview cancelled");
    await settled();
    expect(toastStore.messages).toHaveLength(0);
  });
});
