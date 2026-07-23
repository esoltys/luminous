import { describe, it, expect, beforeEach, vi } from "vitest";
import { updaterStore } from "./updater.svelte";

describe("UpdaterStore", () => {
  beforeEach(() => {
    updaterStore.updateCheckEnabled = false;
    updaterStore.updateAutoInstall = false;
    updaterStore.checkStatus = "idle";
    updaterStore.updateAvailable = false;
    updaterStore.latestVersion = "";
    updaterStore.releaseUrl = "";
    updaterStore.downloadUrl = "";
    updaterStore.errorMessage = null;
    updaterStore.stopPeriodicCheck();
  });

  it("initializes with correct defaults", () => {
    expect(updaterStore.updateCheckEnabled).toBe(false);
    expect(updaterStore.updateAutoInstall).toBe(false);
    expect(updaterStore.checkStatus).toBe("idle");
    expect(updaterStore.updateAvailable).toBe(false);
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
});
