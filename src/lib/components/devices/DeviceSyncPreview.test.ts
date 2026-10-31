import "@testing-library/jest-dom";
import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, fireEvent, waitFor } from "@testing-library/svelte";
import { invoke } from "@tauri-apps/api/core";
import DeviceSyncPreview from "./DeviceSyncPreview.svelte";
import { i18n } from "../../stores/i18n.svelte";
import { toastStore } from "../../stores/toast.svelte";
import type { ConnectedDevice, DeviceSyncState, SyncChangeList, SyncPlan } from "../../types";

const GB = 1_000_000_000;

const device: ConnectedDevice = {
  id: "E:\\",
  kind: "massStorage",
  model: null,
  defaultName: "Walkman",
  storages: [{ id: "E:\\", name: null, totalBytes: 64 * GB, freeBytes: 20 * GB }],
  profile: { nickname: null, convert: { mode: "unsupported", format: "aac", bitrateKbps: 256 } } as unknown as ConnectedDevice["profile"],
  lastSyncedAt: null,
};

const state: DeviceSyncState = {
  settings: { storageId: null, sources: [{ kind: "library" }], unselected: "remove" },
  lastSummary: null,
  running: false,
};

const none: SyncChangeList = { songs: 0, bytes: 0, items: [] };

function plan(overrides: Partial<SyncPlan> = {}, storage: Partial<SyncPlan["storage"]> = {}): SyncPlan {
  return {
    planId: "plan-1",
    deviceId: device.id,
    storageId: "E:\\",
    storage: {
      capacityBytes: 64 * GB,
      reserveBytes: 0,
      otherBytes: 4 * GB,
      staysBytes: 40 * GB,
      removedBytes: 0,
      copiedBytes: 10 * GB,
      freeBytes: 20 * GB,
      freeAfterBytes: 10 * GB,
      fits: true,
      ...storage,
    },
    copy: { songs: 200, bytes: 10 * GB, items: [] },
    remove: none,
    update: none,
    adopted: none,
    skipped: none,
    convertedSongs: 0,
    playlistFilesWritten: 0,
    playlistFilesRemoved: 0,
    folderArtWritten: 0,
    conversionTarget: null,
    sources: [],
    shortfall: null,
    ...overrides,
  };
}

const tooBig = plan(
  {
    conversionTarget: { format: "aac", bitrateKbps: 256, substituted: false, devicePlays: true },
    shortfall: { bytes: 6 * GB, largestSource: null, convertAllSavingBytes: 12 * GB },
  },
  { copiedBytes: 26 * GB, freeAfterBytes: -6 * GB, fits: false }
);

function setup(shown: SyncPlan) {
  const onback = vi.fn();
  const onplan = vi.fn();
  const onstarted = vi.fn();
  const result = render(DeviceSyncPreview, { props: { device, plan: shown, onback, onplan, onstarted } });
  return { ...result, onback, onplan, onstarted };
}

describe("DeviceSyncPreview.svelte", () => {
  let startError: string | null;
  const replanned = plan({ planId: "plan-2" });

  beforeEach(() => {
    i18n.currentLocale = "en-CA";
    startError = null;
    vi.mocked(invoke).mockReset();
    vi.mocked(invoke).mockImplementation(async (cmd: string) => {
      if (cmd === "start_device_sync" && startError) throw startError;
      if (cmd === "get_device_sync_state") return state;
      if (cmd === "preview_device_sync") return replanned;
      return null;
    });
  });

  it("starts the previewed plan when it fits", async () => {
    const { getByTestId, getByRole, onstarted } = setup(plan());

    expect(getByTestId("sync-verdict")).toHaveTextContent("Fits, with");
    await fireEvent.click(getByRole("button", { name: "Start sync" }));

    expect(invoke).toHaveBeenCalledWith("start_device_sync", { planId: "plan-1" });
    await waitFor(() => expect(onstarted).toHaveBeenCalled());
  });

  it("plans again when the device changed since the preview, and doesn't start", async () => {
    startError = "sync-plan-stale";
    const show = vi.spyOn(toastStore, "show");
    const { getByRole, onplan, onstarted } = setup(plan());

    await fireEvent.click(getByRole("button", { name: "Start sync" }));

    await waitFor(() => expect(onplan).toHaveBeenCalledWith(replanned));
    expect(onstarted).not.toHaveBeenCalled();
    expect(show).toHaveBeenCalledWith(expect.stringContaining("changed since the preview"), "info");
  });

  it("offers no Start sync when the selection doesn't fit, only the fixes", async () => {
    const { getByTestId, queryByRole, getByRole } = setup(tooBig);

    expect(getByTestId("sync-verdict")).toHaveTextContent("too much for Walkman");
    expect(queryByRole("button", { name: "Start sync" })).not.toBeInTheDocument();
    expect(getByRole("button", { name: "Change selection" })).toBeInTheDocument();
    expect(getByRole("button", { name: "Convert everything to AAC 256" })).toBeInTheDocument();
  });

  it("converts everything that gets smaller, then shows the new plan", async () => {
    const { getByRole, onplan } = setup(tooBig);

    await fireEvent.click(getByRole("button", { name: "Convert everything to AAC 256" }));

    await waitFor(() => expect(onplan).toHaveBeenCalledWith(replanned));
    expect(invoke).toHaveBeenCalledWith("update_device_profile", {
      id: device.id,
      profile: expect.objectContaining({ convert: expect.objectContaining({ mode: "saveSpace" }) }),
    });
  });
});
