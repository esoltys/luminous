import { describe, it, expect, beforeEach, vi } from "vitest";
import { updaterStore } from "./updater.svelte";
import { check } from "@tauri-apps/plugin-updater";
import { relaunch } from "@tauri-apps/plugin-process";

function fakeUpdate(overrides: Partial<{ version: string }> = {}) {
  return {
    version: overrides.version ?? "1.2.3",
    currentVersion: "1.0.0",
    downloadAndInstall: vi.fn().mockResolvedValue(undefined),
    close: vi.fn().mockResolvedValue(undefined),
  } as any;
}

describe("UpdaterStore", () => {
  beforeEach(() => {
    (updaterStore as any).initialized = false;
    updaterStore.updateCheckEnabled = false;
    updaterStore.updateAutoInstall = false;
    updaterStore.checkStatus = "idle";
    updaterStore.updateAvailable = false;
    updaterStore.latestVersion = "";
    updaterStore.releaseUrl = "";
    updaterStore.errorMessage = null;
    updaterStore.installStatus = "idle";
    updaterStore.downloadProgress = null;
    updaterStore.lastCheckedAt = null;
    updaterStore.installFormat = {
      format: "windows_setup",
      human_name: "Windows Installer (.exe / .msi)",
      supports_self_update: true,
    };
    updaterStore.stopPeriodicCheck();
    vi.mocked(check).mockReset().mockResolvedValue(null);
    vi.mocked(relaunch).mockReset().mockResolvedValue(undefined);
  });

  it("initializes with correct defaults", () => {
    expect(updaterStore.updateCheckEnabled).toBe(false);
    expect(updaterStore.updateAutoInstall).toBe(false);
    expect(updaterStore.checkStatus).toBe("idle");
    expect(updaterStore.updateAvailable).toBe(false);
  });

  it("init() defaults updateCheckEnabled to true and triggers check", async () => {
    vi.mocked(check).mockResolvedValueOnce(null);
    await updaterStore.init();
    await Promise.resolve();
    await Promise.resolve();

    expect(updaterStore.updateCheckEnabled).toBe(true);
    expect(updaterStore.checkStatus).toBe("up-to-date");
  });

  it("init() is idempotent on subsequent calls", async () => {
    vi.mocked(check).mockResolvedValueOnce(null);
    await updaterStore.init();

    const checkCallsAfterFirstInit = vi.mocked(check).mock.calls.length;
    await updaterStore.init();
    expect(vi.mocked(check).mock.calls.length).toBe(checkCallsAfterFirstInit);
  });

  it("toggles updateCheckEnabled and stops auto-install if disabled", async () => {
    await updaterStore.setUpdateCheckEnabled(true);
    expect(updaterStore.updateCheckEnabled).toBe(true);

    await updaterStore.setUpdateAutoInstall(true);
    expect(updaterStore.updateAutoInstall).toBe(true);

    await updaterStore.setUpdateCheckEnabled(false);
    expect(updaterStore.updateCheckEnabled).toBe(false);
    expect(updaterStore.updateAutoInstall).toBe(false);
  });

  it("marks an update available when the plugin finds a newer version", async () => {
    vi.mocked(check).mockResolvedValueOnce(fakeUpdate({ version: "2.0.0" }));

    await updaterStore.checkForUpdates();

    expect(updaterStore.checkStatus).toBe("available");
    expect(updaterStore.updateAvailable).toBe(true);
    expect(updaterStore.latestVersion).toBe("v2.0.0");
  });

  it("marks up-to-date when the plugin finds no update", async () => {
    vi.mocked(check).mockResolvedValueOnce(null);

    await updaterStore.checkForUpdates();

    expect(updaterStore.checkStatus).toBe("up-to-date");
    expect(updaterStore.updateAvailable).toBe(false);
  });

  it("surfaces a raw string rejection (as Tauri command errors arrive) as errorMessage, not a generic fallback", async () => {
    // Tauri command failures reject with the raw Rust `Err(String)`, not an `Error` instance —
    // `err.message` on a string is `undefined`, so a naive `err?.message || fallback` always
    // produced the same fallback text, regardless of the real reason.
    vi.mocked(check).mockRejectedValueOnce("Could not fetch a valid release JSON");

    await updaterStore.checkForUpdates();

    expect(updaterStore.checkStatus).toBe("error");
    expect(updaterStore.errorMessage).toBe("Could not fetch a valid release JSON");
  });

  it("auto-installs when updateAutoInstall is on and the format supports self-update", async () => {
    const update = fakeUpdate({ version: "2.0.0" });
    vi.mocked(check).mockResolvedValueOnce(update);
    updaterStore.updateAutoInstall = true;

    await updaterStore.checkForUpdates();
    // downloadAndInstall runs async without being awaited by checkForUpdates; flush microtasks.
    await Promise.resolve();
    await Promise.resolve();

    expect(update.downloadAndInstall).toHaveBeenCalled();
  });

  it("does not auto-install when the format does not support self-update", async () => {
    updaterStore.installFormat = { format: "deb", human_name: "Debian Package (.deb)", supports_self_update: false };
    const update = fakeUpdate({ version: "2.0.0" });
    vi.mocked(check).mockResolvedValueOnce(update);
    updaterStore.updateAutoInstall = true;

    await updaterStore.checkForUpdates();
    await Promise.resolve();

    expect(update.downloadAndInstall).not.toHaveBeenCalled();
  });

  it("downloadAndInstall transitions to ready-to-restart on success", async () => {
    const update = fakeUpdate({ version: "2.0.0" });
    vi.mocked(check).mockResolvedValueOnce(update);

    await updaterStore.checkForUpdates();
    await updaterStore.downloadAndInstall();

    expect(updaterStore.installStatus).toBe("ready-to-restart");
  });

  it("restartNow calls relaunch", async () => {
    await updaterStore.restartNow();
    expect(relaunch).toHaveBeenCalled();
  });

  it("records lastCheckedAt after a successful check", async () => {
    vi.mocked(check).mockResolvedValueOnce(null);
    expect(updaterStore.lastCheckedAt).toBe(null);

    await updaterStore.checkForUpdates();

    expect(updaterStore.lastCheckedAt).toEqual(expect.any(Number));
  });

  it("does not record lastCheckedAt when the check errors", async () => {
    vi.mocked(check).mockRejectedValueOnce("network down");

    await updaterStore.checkForUpdates();

    expect(updaterStore.lastCheckedAt).toBe(null);
  });

  describe("updatePolicy", () => {
    it("derives never/notify/auto from the underlying toggles", () => {
      updaterStore.updateCheckEnabled = false;
      updaterStore.updateAutoInstall = false;
      expect(updaterStore.updatePolicy).toBe("never");

      updaterStore.updateCheckEnabled = true;
      updaterStore.updateAutoInstall = false;
      expect(updaterStore.updatePolicy).toBe("notify");

      updaterStore.updateAutoInstall = true;
      expect(updaterStore.updatePolicy).toBe("auto");
    });

    it("setUpdatePolicy('never') disables checks and auto-install", async () => {
      await updaterStore.setUpdatePolicy("auto");
      expect(updaterStore.updatePolicy).toBe("auto");

      await updaterStore.setUpdatePolicy("never");
      expect(updaterStore.updateCheckEnabled).toBe(false);
      expect(updaterStore.updateAutoInstall).toBe(false);
    });

    it("setUpdatePolicy('notify') enables checks without auto-install", async () => {
      await updaterStore.setUpdatePolicy("notify");
      expect(updaterStore.updateCheckEnabled).toBe(true);
      expect(updaterStore.updateAutoInstall).toBe(false);
    });

    it("setUpdatePolicy('auto') enables checks and auto-install", async () => {
      const update = fakeUpdate({ version: "2.0.0" });
      vi.mocked(check).mockResolvedValueOnce(update);

      await updaterStore.setUpdatePolicy("auto");

      expect(updaterStore.updateCheckEnabled).toBe(true);
      expect(updaterStore.updateAutoInstall).toBe(true);
    });
  });
});
