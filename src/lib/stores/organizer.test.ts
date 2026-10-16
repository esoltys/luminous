import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import { toastStore } from "./toast.svelte";
import { organizeStore, computeOrganizeConsequences } from "./organizer.svelte";
import { TOAST_DURATION_MS } from "../constants";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
}));

vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn().mockResolvedValue(() => {}),
}));

describe("organizeStore", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.useFakeTimers();
    vi.spyOn(toastStore, "show");
  });

  afterEach(() => {
    organizeStore.destroy();
    vi.useRealTimers();
  });

  it("loads config on init and applies default/custom values", async () => {
    vi.mocked(invoke).mockResolvedValueOnce({
      auto_organize: true,
      template: "%artist/%title",
      preset: "custom",
      destination_mode: "custom",
      custom_destination_dir: "/music/organized",
      replace_spaces: true,
      ascii_only: true,
      clean_empty_dirs: false,
      move_extra_files: false,
    });

    await organizeStore.init();

    expect(invoke).toHaveBeenCalledWith("get_organize_config");
    expect(organizeStore.autoOrganize).toBe(true);
    expect(organizeStore.template).toBe("%artist/%title");
    expect(organizeStore.preset).toBe("custom");
    expect(organizeStore.destinationMode).toBe("custom");
    expect(organizeStore.customDestinationDir).toBe("/music/organized");
    expect(organizeStore.replaceSpaces).toBe(true);
    expect(organizeStore.asciiOnly).toBe(true);
    expect(organizeStore.cleanEmptyDirs).toBe(false);
    expect(organizeStore.moveExtraFiles).toBe(false);
  });

  it("toggles autoOrganize and persists via set_organize_config", async () => {
    vi.mocked(invoke).mockResolvedValue(undefined);

    await organizeStore.setAutoOrganize(true);

    expect(organizeStore.autoOrganize).toBe(true);
    expect(invoke).toHaveBeenCalledWith("set_organize_config", {
      config: expect.objectContaining({ auto_organize: true }),
    });
  });

  it("updates partial config and persists", async () => {
    vi.mocked(invoke).mockResolvedValue(undefined);

    await organizeStore.updateConfig({
      template: "%albumartist/%album/%title",
      replace_spaces: true,
    });

    expect(organizeStore.template).toBe("%albumartist/%album/%title");
    expect(organizeStore.replaceSpaces).toBe(true);
    expect(invoke).toHaveBeenCalledWith("set_organize_config", {
      config: expect.objectContaining({
        template: "%albumartist/%album/%title",
        replace_spaces: true,
      }),
    });
  });

  it("debounces rapid successful moves into a single non-sticky final notification", () => {
    // Two rapid batches of file moves
    organizeStore.handleResult({ moved_count: 5, duplicates_count: 0, errors: [] });
    organizeStore.handleResult({ moved_count: 3, duplicates_count: 0, errors: [] });

    // Initially no toast has fired while debouncing
    expect(toastStore.show).not.toHaveBeenCalled();

    // Advance past the 1200ms debounce window
    vi.advanceTimersByTime(1200);

    // Exactly one toast showing the accumulated 8 files, non-sticky (TOAST_DURATION_MS = 4000)
    expect(toastStore.show).toHaveBeenCalledTimes(1);
    expect(toastStore.show).toHaveBeenCalledWith(
      expect.stringContaining("8"),
      "success",
      TOAST_DURATION_MS
    );
  });

  it("shows sticky warning notification when duplicates are detected", () => {
    organizeStore.handleResult({ moved_count: 0, duplicates_count: 2, errors: [] });

    // Duplicates toast fires immediately and omits durationMs (sticky)
    expect(toastStore.show).toHaveBeenCalledWith(
      expect.stringContaining("2"),
      "warning"
    );
  });

  it("shows sticky error notification when file move errors occur", () => {
    organizeStore.handleResult({
      moved_count: 0,
      duplicates_count: 0,
      errors: ["Permission denied"],
    });

    // Error toast fires immediately and omits durationMs (sticky)
    expect(toastStore.show).toHaveBeenCalledWith(
      expect.stringContaining("1"),
      "error"
    );
  });

  it("remains silent when 0 files were moved and no duplicates/errors", () => {
    organizeStore.handleResult({ moved_count: 0, duplicates_count: 0, errors: [] });
    vi.advanceTimersByTime(2000);

    expect(toastStore.show).not.toHaveBeenCalled();
  });

  it("updates isDraftActive state via setDraftActive", () => {
    expect(organizeStore.isDraftActive).toBe(false);
    organizeStore.setDraftActive(true);
    expect(organizeStore.isDraftActive).toBe(true);
    organizeStore.setDraftActive(false);
    expect(organizeStore.isDraftActive).toBe(false);
  });

  describe("computeOrganizeConsequences", () => {
    it("computes file and folder counts excluding unchanged, collisions, and errors", () => {
      const items = [
        { from_path: "/music/a.mp3", to_path: "/music/artist/a.mp3", status: "ok" },
        { from_path: "/music/b.mp3", to_path: "/music/artist/b.mp3", status: "ok" },
        { from_path: "/music/c.mp3", to_path: "/music/other/c.mp3", status: "missing_tag" },
        // unchanged item
        { from_path: "/music/d.mp3", to_path: "/music/d.mp3", status: "unchanged" },
        // collision item
        { from_path: "/music/e.mp3", to_path: "/music/artist/e.mp3", status: "collision" },
        // collision masked as ok with error message
        { from_path: "/music/f.mp3", to_path: "/music/artist/f.mp3", status: "ok", error_message: "Collision" },
        // error item
        { from_path: "/music/g.mp3", to_path: "/music/artist/g.mp3", status: "error" },
      ];

      const res = computeOrganizeConsequences(items, {
        cleanEmptyDirs: true,
        moveExtraFiles: false,
      });

      expect(res.fileCount).toBe(3);
      expect(res.folderCount).toBe(2); // "/music/artist" and "/music/other"
      expect(res.cleanEmptyDirs).toBe(true);
      expect(res.moveExtraFiles).toBe(false);
    });

    it("returns zero counts when items list is empty", () => {
      const res = computeOrganizeConsequences([], {
        cleanEmptyDirs: false,
        moveExtraFiles: true,
      });
      expect(res.fileCount).toBe(0);
      expect(res.folderCount).toBe(0);
      expect(res.cleanEmptyDirs).toBe(false);
      expect(res.moveExtraFiles).toBe(true);
    });
  });
});
