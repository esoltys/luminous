import { describe, it, expect, beforeEach, vi } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import { walkthroughStore } from "./walkthrough.svelte";
import { navigationStore } from "./navigation.svelte";
import { windowLayoutStore } from "./windowLayout.svelte";
import { playerStore } from "./player.svelte";

describe("WalkthroughStore", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    walkthroughStore.isActive = false;
    walkthroughStore.currentStepIndex = 0;
    walkthroughStore.hasCompleted = false;
  });

  it("loads completion state from the backend on init", async () => {
    vi.mocked(invoke).mockImplementationOnce(async (cmd) => {
      if (cmd === "get_all_app_settings") return { walkthrough_completed: "true" };
      return null;
    });

    await walkthroughStore.init();

    expect(walkthroughStore.hasCompleted).toBe(true);
    expect(invoke).toHaveBeenCalledWith("get_all_app_settings");
  });

  it("defaults to not completed when the backend has no flag", async () => {
    vi.mocked(invoke).mockImplementationOnce(async (cmd) => {
      if (cmd === "get_all_app_settings") return {};
      return null;
    });

    await walkthroughStore.init();

    expect(walkthroughStore.hasCompleted).toBe(false);
  });

  it("start() activates the tour at the first step", () => {
    walkthroughStore.currentStepIndex = 3;
    walkthroughStore.start();

    expect(walkthroughStore.isActive).toBe(true);
    expect(walkthroughStore.currentStepIndex).toBe(0);
    expect(walkthroughStore.currentStep.id).toBe("sidebar");
  });

  it("next() advances through steps and finishes (persisting completion) after the last one", () => {
    walkthroughStore.start();
    const total = walkthroughStore.totalSteps;

    for (let i = 1; i < total; i++) {
      walkthroughStore.next();
      expect(walkthroughStore.currentStepIndex).toBe(i);
      expect(walkthroughStore.isActive).toBe(true);
    }

    walkthroughStore.next();

    expect(walkthroughStore.isActive).toBe(false);
    expect(walkthroughStore.hasCompleted).toBe(true);
    expect(invoke).toHaveBeenCalledWith("set_app_setting", {
      key: "walkthrough_completed",
      value: "true",
    });
  });

  it("prev() moves back a step and does nothing at the first step", () => {
    walkthroughStore.start();
    walkthroughStore.next();
    expect(walkthroughStore.currentStepIndex).toBe(1);

    walkthroughStore.prev();
    expect(walkthroughStore.currentStepIndex).toBe(0);

    walkthroughStore.prev();
    expect(walkthroughStore.currentStepIndex).toBe(0);
  });

  it("skip() deactivates the tour and persists completion", () => {
    walkthroughStore.start();
    walkthroughStore.skip();

    expect(walkthroughStore.isActive).toBe(false);
    expect(walkthroughStore.hasCompleted).toBe(true);
    expect(invoke).toHaveBeenCalledWith("set_app_setting", {
      key: "walkthrough_completed",
      value: "true",
    });
  });

  it("the collection-view step's beforeStep switches to the collection tab on an albums/artists sub-tab", () => {
    navigationStore.activeTab = "home";
    navigationStore.activeSubTab = "songs";
    walkthroughStore.start();

    walkthroughStore.next(); // top-navigation
    walkthroughStore.next(); // collection-view

    expect(navigationStore.activeTab).toBe("collection");
    expect(["albums", "artists"]).toContain(navigationStore.activeSubTab);
  });

  it("the right-panel step's beforeStep opens the right panel if something is playing and it's closed", () => {
    windowLayoutStore.rightPanelOpen = false;
    playerStore.currentSong = { id: 1 } as any;
    walkthroughStore.start();

    walkthroughStore.next(); // top-navigation
    walkthroughStore.next(); // collection-view
    walkthroughStore.next(); // player-bar-cover
    walkthroughStore.next(); // player-bar-controls
    walkthroughStore.next(); // player-bar-toolbar
    walkthroughStore.next(); // right-panel

    expect(windowLayoutStore.rightPanelOpen).toBe(true);

    playerStore.currentSong = undefined;
  });

  it("the right-panel step's beforeStep leaves the panel closed when nothing is playing", () => {
    windowLayoutStore.rightPanelOpen = false;
    playerStore.currentSong = undefined;
    walkthroughStore.start();

    for (let i = 0; i < 6; i++) walkthroughStore.next();
    expect(walkthroughStore.currentStep.id).toBe("right-panel");

    expect(windowLayoutStore.rightPanelOpen).toBe(false);
  });
});
