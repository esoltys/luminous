import { describe, it, expect, beforeEach, vi } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import { welcomeStore } from "./welcome.svelte";

describe("WelcomeStore", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    welcomeStore.hasSeen = false;
    welcomeStore.initialized = false;
  });

  it("loads seen state from the backend on init", async () => {
    vi.mocked(invoke).mockImplementationOnce(async (cmd) => {
      if (cmd === "get_all_app_settings") return { welcome_seen: "true" };
      return null;
    });

    await welcomeStore.init();

    expect(welcomeStore.hasSeen).toBe(true);
    expect(welcomeStore.initialized).toBe(true);
    expect(invoke).toHaveBeenCalledWith("get_all_app_settings");
  });

  it("defaults to not seen when the backend has no flag", async () => {
    vi.mocked(invoke).mockImplementationOnce(async (cmd) => {
      if (cmd === "get_all_app_settings") return {};
      return null;
    });

    await welcomeStore.init();

    expect(welcomeStore.hasSeen).toBe(false);
  });

  it("fails open (marks seen) if the backend call throws", async () => {
    vi.mocked(invoke).mockImplementationOnce(async () => {
      throw new Error("no backend");
    });

    await welcomeStore.init();

    expect(welcomeStore.hasSeen).toBe(true);
    expect(welcomeStore.initialized).toBe(true);
  });

  it("markSeen() persists the flag exactly once", () => {
    welcomeStore.markSeen();
    expect(welcomeStore.hasSeen).toBe(true);
    expect(invoke).toHaveBeenCalledWith("set_app_setting", { key: "welcome_seen", value: "true" });

    vi.mocked(invoke).mockClear();
    welcomeStore.markSeen();
    expect(invoke).not.toHaveBeenCalled();
  });
});
