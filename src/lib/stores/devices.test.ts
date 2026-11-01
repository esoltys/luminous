import { describe, it, expect, beforeEach, vi } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import type { ConnectedDevice, DeviceSyncState, SyncProgress, SyncSettings, SyncSummary } from "../types";

const handlers = vi.hoisted(() => new Map<string, (event: { payload: unknown }) => void>());

vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(async (event: string, callback: (event: { payload: unknown }) => void) => {
    handlers.set(event, callback);
    return () => {};
  }),
}));

const walkman: ConnectedDevice = {
  id: "E:\\",
  kind: "massStorage",
  model: null,
  defaultName: "Walkman",
  storages: [],
  profile: { nickname: null } as ConnectedDevice["profile"],
  lastSyncedAt: null,
};

let running = false;
let ejectError: string | null = null;
/** The selection the backend has saved; planning saves the one it is given. */
let saved: SyncSettings = { storageId: null, sources: [], unselected: "remove" };
function backend() {
  vi.mocked(invoke).mockImplementation(async (cmd: string, args?: unknown) => {
    if (cmd === "get_connected_devices") return [walkman];
    if (cmd === "get_device_sync_state") {
      return { settings: saved, lastSummary: null, running } satisfies DeviceSyncState;
    }
    if (cmd === "preview_device_sync") {
      saved = (args as { settings: SyncSettings }).settings;
      return { planId: "plan" };
    }
    if (cmd === "eject_device" && ejectError) throw ejectError;
    return null;
  });
}
backend();

import { devicesStore } from "./devices.svelte";
import { i18n } from "./i18n.svelte";
import { toastStore } from "./toast.svelte";

function progress(): SyncProgress {
  return { deviceId: walkman.id, step: { kind: "copying", song: { artist: "A", album: null, title: "T" } }, songsDone: 3, songsTotal: 10, bytesDone: 1, bytesTotal: 10, bytesPerSec: null, etaSecs: null };
}

function summary(outcome: SyncSummary["outcome"], failures = 0): SyncSummary {
  return {
    deviceId: walkman.id,
    outcome,
    finishedAt: 0,
    songsWritten: 7,
    songsPlanned: 10,
    converted: 0,
    removed: 0,
    playlistFiles: 0,
    folderArt: 0,
    failures: Array.from({ length: failures }, (_, i) => ({ path: `${i}.flac`, song: { artist: null, album: null, title: null }, message: "" })),
    plannedBytes: 0,
    writtenBytes: 0,
    plannedFreeAfterBytes: 0,
    freeAfterBytes: null,
  };
}

describe("devicesStore", () => {
  beforeEach(async () => {
    i18n.currentLocale = "en-CA";
    await vi.waitFor(() => expect(devicesStore.loaded).toBe(true));
    running = false;
    ejectError = null;
    saved = { storageId: null, sources: [], unselected: "remove" };
    backend();
    handlers.get("devices-changed")!({ payload: [walkman] });
    handlers.get("device-sync-finished")!({ payload: summary({ kind: "completed" }) });
    await vi.waitFor(() => expect(devicesStore.isSyncing(walkman.id)).toBe(false));
  });

  it("learns a sync is running from its progress, and that it ended from the backend", async () => {
    handlers.get("device-sync-progress")!({ payload: progress() });
    expect(devicesStore.isSyncing(walkman.id)).toBe(true);
    expect(devicesStore.syncingId).toBe(walkman.id);

    handlers.get("device-sync-finished")!({ payload: summary({ kind: "stopped" }) });

    await vi.waitFor(() => expect(devicesStore.syncingId).toBeNull());
    expect(devicesStore.progress[walkman.id]).toBeUndefined();
  });

  it("names the device and the songs that failed when a sync finishes, and keeps the toast up", () => {
    const show = vi.spyOn(toastStore, "show");

    handlers.get("device-sync-finished")!({ payload: summary({ kind: "completed" }, 2) });

    expect(show).toHaveBeenCalledWith("Sync to Walkman complete, but 2 songs couldn't be copied.", "warning");
  });

  it("brings the progress drawer back after it was hidden", () => {
    devicesStore.drawerHidden = true;
    devicesStore.showProgress();
    expect(devicesStore.drawerHidden).toBe(false);
  });

  it("keeps the selection a folder was added to, so planning it later doesn't drop the folder", async () => {
    await devicesStore.refreshSyncState(walkman.id);

    expect(await devicesStore.addFolderToSync(walkman.id, "M:\Amon Amarth")).toBe(true);
    // What the device's Sync now plans when its Sync tab isn't open.
    await devicesStore.planSync(walkman.id, devicesStore.syncStates[walkman.id].settings);

    expect(saved.sources).toEqual([{ kind: "folder", path: "M:\Amon Amarth" }]);
  });

  it("adds a folder only once", async () => {
    expect(await devicesStore.addFolderToSync(walkman.id, "M:\Amon Amarth")).toBe(true);
    expect(await devicesStore.addFolderToSync(walkman.id, "M:\Amon Amarth")).toBe(false);
    expect(saved.sources).toHaveLength(1);
  });

  it("says why a device can't be ejected", async () => {
    const show = vi.spyOn(toastStore, "show");
    ejectError = "device-in-use";

    await devicesStore.eject(walkman.id);

    expect(show).toHaveBeenCalledWith(expect.stringContaining("another program is using files on it"), "error");
  });
});
