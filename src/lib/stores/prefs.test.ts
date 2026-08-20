import { describe, it, expect, beforeEach, vi } from "vitest";
import { prefs } from "./prefs.svelte";
import { invoke } from "@tauri-apps/api/core";
import { isPermissionGranted, requestPermission } from "@tauri-apps/plugin-notification";

describe("PrefsStore - track change notifications (#524)", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    prefs.trackNotificationsEnabled = false;
  });

  it("persists the preference and requests no permission when disabling", async () => {
    await prefs.setTrackNotificationsEnabled(false);

    expect(prefs.trackNotificationsEnabled).toBe(false);
    expect(invoke).toHaveBeenCalledWith("set_track_notifications_enabled", { enabled: false });
    expect(requestPermission).not.toHaveBeenCalled();
  });

  it("requests OS permission when enabling and permission isn't already granted", async () => {
    vi.mocked(isPermissionGranted).mockResolvedValueOnce(false);

    await prefs.setTrackNotificationsEnabled(true);

    expect(prefs.trackNotificationsEnabled).toBe(true);
    expect(invoke).toHaveBeenCalledWith("set_track_notifications_enabled", { enabled: true });
    expect(isPermissionGranted).toHaveBeenCalled();
    expect(requestPermission).toHaveBeenCalled();
  });

  it("does not re-request permission when already granted", async () => {
    vi.mocked(isPermissionGranted).mockResolvedValueOnce(true);

    await prefs.setTrackNotificationsEnabled(true);

    expect(requestPermission).not.toHaveBeenCalled();
  });

  it("swallows a permission-check failure without throwing", async () => {
    vi.mocked(isPermissionGranted).mockRejectedValueOnce(new Error("boom"));

    await expect(prefs.setTrackNotificationsEnabled(true)).resolves.toBeUndefined();
    expect(prefs.trackNotificationsEnabled).toBe(true);
  });
});
