import { describe, it, expect, beforeEach, vi } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import { walkthroughStore } from "./walkthrough.svelte";
import { navigationStore } from "./navigation.svelte";
import { windowLayoutStore } from "./windowLayout.svelte";
import { playerStore } from "./player.svelte";
import { collectionStore } from "./collection.svelte";

describe("WalkthroughStore", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    walkthroughStore.isActive = false;
    walkthroughStore.currentStepIndex = 0;
    walkthroughStore.seenStepIds = new Set();
    // Full-tour tests opt into a populated library + active playback so
    // every step is available; availability-specific tests below override
    // these back down to the empty/nothing-playing state they're testing.
    collectionStore.stats.total_songs = 5;
    playerStore.currentSong = { id: 1 } as any;
    windowLayoutStore.rightPanelOpen = false;
  });

  it("loads seen steps from the backend on init", async () => {
    vi.mocked(invoke).mockImplementationOnce(async (cmd) => {
      if (cmd === "get_all_app_settings") return { walkthrough_seen_steps: "sidebar,top-navigation" };
      return null;
    });

    await walkthroughStore.init();

    expect(walkthroughStore.seenStepIds).toEqual(new Set(["sidebar", "top-navigation"]));
  });

  it("defaults to no seen steps when the backend has no flag", async () => {
    vi.mocked(invoke).mockImplementationOnce(async (cmd) => {
      if (cmd === "get_all_app_settings") return {};
      return null;
    });

    await walkthroughStore.init();

    expect(walkthroughStore.seenStepIds.size).toBe(0);
  });

  it("migrates the legacy walkthrough_completed flag to every step seen", async () => {
    vi.mocked(invoke).mockImplementation(async (cmd) => {
      if (cmd === "get_all_app_settings") return { walkthrough_completed: "true" };
      return null;
    });

    await walkthroughStore.init();

    expect(walkthroughStore.seenStepIds.size).toBe(walkthroughStore.totalSteps);
    expect(walkthroughStore.hasPendingSteps).toBe(false);
    expect(invoke).toHaveBeenCalledWith("set_app_setting", {
      key: "walkthrough_seen_steps",
      value: expect.stringContaining("sidebar"),
    });
  });

  it("start() activates the tour at the first step", () => {
    walkthroughStore.currentStepIndex = 3;
    walkthroughStore.start();

    expect(walkthroughStore.isActive).toBe(true);
    expect(walkthroughStore.currentStepIndex).toBe(0);
    expect(walkthroughStore.currentStep.id).toBe("sidebar");
  });

  it("next() advances through every step and finishes (persisting all as seen) after the last one", () => {
    walkthroughStore.start();
    const total = walkthroughStore.totalSteps;

    for (let i = 1; i < total; i++) {
      walkthroughStore.next();
      expect(walkthroughStore.currentStepIndex).toBe(i);
      expect(walkthroughStore.isActive).toBe(true);
    }

    walkthroughStore.next();

    expect(walkthroughStore.isActive).toBe(false);
    expect(walkthroughStore.hasPendingSteps).toBe(false);
    expect(invoke).toHaveBeenCalledWith("set_app_setting", {
      key: "walkthrough_seen_steps",
      value: expect.any(String),
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

  it("skip() deactivates the tour and persists only the steps actually shown", () => {
    walkthroughStore.start();
    walkthroughStore.next(); // top-navigation
    walkthroughStore.skip();

    expect(walkthroughStore.isActive).toBe(false);
    expect(walkthroughStore.seenStepIds).toEqual(new Set(["sidebar", "top-navigation"]));
    // Steps never reached (collection-view onward) stay pending for a future resume.
    expect(walkthroughStore.hasPendingSteps).toBe(true);
  });

  it("the collection-view step's beforeStep switches to the collection tab on an albums/artists sub-tab", () => {
    navigationStore.activeTab = "home";
    navigationStore.activeSubTab = "songs";
    walkthroughStore.start();

    walkthroughStore.next(); // top-navigation
    walkthroughStore.next(); // collection-view

    expect(walkthroughStore.currentStep.id).toBe("collection-view");
    expect(navigationStore.activeTab).toBe("collection");
    expect(["albums", "artists"]).toContain(navigationStore.activeSubTab);
  });

  it("the right-panel step's beforeStep opens the right panel when something is playing and it's closed", () => {
    walkthroughStore.start();

    while (walkthroughStore.currentStep.id !== "right-panel") {
      walkthroughStore.next();
    }

    expect(windowLayoutStore.rightPanelOpen).toBe(true);
  });

  it("leaves the right panel open if it was already open before the tour", () => {
    windowLayoutStore.rightPanelOpen = true;
    walkthroughStore.start();
    while (walkthroughStore.currentStep.id !== "right-panel") walkthroughStore.next();
    expect(windowLayoutStore.rightPanelOpen).toBe(true);

    walkthroughStore.next(); // library-folders — leaving right-panel
    expect(windowLayoutStore.rightPanelOpen).toBe(true);
  });

  it("closes the right panel again as soon as the tour moves past that step, not just at the end", () => {
    windowLayoutStore.rightPanelOpen = false;
    walkthroughStore.start();
    while (walkthroughStore.currentStep.id !== "right-panel") walkthroughStore.next();
    expect(windowLayoutStore.rightPanelOpen).toBe(true);

    // Playback stops right as they move on — no visible PlayerBar left to
    // close the panel with, so the tour must restore it itself here rather
    // than waiting for finish()/skip().
    playerStore.currentSong = undefined;
    walkthroughStore.next(); // library-folders

    expect(windowLayoutStore.rightPanelOpen).toBe(false);
  });

  it("closes the right panel it opened if the tour is skipped while still on that step", () => {
    windowLayoutStore.rightPanelOpen = false;
    walkthroughStore.start();
    while (walkthroughStore.currentStep.id !== "right-panel") walkthroughStore.next();
    expect(windowLayoutStore.rightPanelOpen).toBe(true);

    walkthroughStore.skip();

    expect(windowLayoutStore.rightPanelOpen).toBe(false);
  });

  it("doesn't reopen a panel the user closed themselves mid-step", () => {
    windowLayoutStore.rightPanelOpen = false;
    walkthroughStore.start();
    while (walkthroughStore.currentStep.id !== "right-panel") walkthroughStore.next();
    expect(windowLayoutStore.rightPanelOpen).toBe(true);

    windowLayoutStore.rightPanelOpen = false; // user closes it manually
    walkthroughStore.next(); // library-folders

    expect(windowLayoutStore.rightPanelOpen).toBe(false);
  });

  describe("skipping steps unavailable in the current app state", () => {
    it("skips collection-view when the library is empty", () => {
      collectionStore.stats.total_songs = 0;
      walkthroughStore.start();

      walkthroughStore.next(); // top-navigation
      walkthroughStore.next(); // would be collection-view, but it's unavailable

      expect(walkthroughStore.currentStep.id).not.toBe("collection-view");
    });

    it("skips every player-bar/right-panel step when nothing is playing, landing on library-folders", () => {
      playerStore.currentSong = undefined;
      walkthroughStore.start();

      walkthroughStore.next(); // top-navigation
      walkthroughStore.next(); // collection-view (library still populated)
      walkthroughStore.next(); // skips player-bar-cover/controls/toolbar/right-panel in one call

      expect(walkthroughStore.currentStep.id).toBe("library-folders");
    });

    it("finish()es immediately if every remaining step is unavailable", () => {
      collectionStore.stats.total_songs = 0;
      playerStore.currentSong = undefined;
      walkthroughStore.start();

      walkthroughStore.next(); // top-navigation
      walkthroughStore.next(); // library-folders (only step left that's always available)
      expect(walkthroughStore.currentStep.id).toBe("library-folders");

      walkthroughStore.next();

      expect(walkthroughStore.isActive).toBe(false);
    });

    it("prev() steps back over unavailable steps too", () => {
      playerStore.currentSong = undefined;
      walkthroughStore.start();
      walkthroughStore.next(); // top-navigation
      walkthroughStore.next(); // collection-view
      walkthroughStore.next(); // library-folders (player-bar/right-panel skipped)
      expect(walkthroughStore.currentStep.id).toBe("library-folders");

      walkthroughStore.prev();

      expect(walkthroughStore.currentStep.id).toBe("collection-view");
    });

    it("availableSteps/currentAvailableIndex reflect only this run's reachable steps", () => {
      collectionStore.stats.total_songs = 0;
      playerStore.currentSong = undefined;
      walkthroughStore.start();

      // sidebar, top-navigation, library-folders — not all 8.
      expect(walkthroughStore.availableSteps.map((s) => s.id)).toEqual(["sidebar", "top-navigation", "library-folders"]);
      expect(walkthroughStore.currentAvailableIndex).toBe(0);

      walkthroughStore.next();
      expect(walkthroughStore.currentAvailableIndex).toBe(1);
      walkthroughStore.next();
      expect(walkthroughStore.currentAvailableIndex).toBe(2);
    });
  });

  describe("resuming across sessions (per-step seen tracking)", () => {
    it("hasPendingSteps is false once every currently-available step has been seen", () => {
      collectionStore.stats.total_songs = 0;
      playerStore.currentSong = undefined;
      walkthroughStore.start();
      while (walkthroughStore.isActive) walkthroughStore.next();

      expect(walkthroughStore.hasPendingSteps).toBe(false);
    });

    it("hasPendingSteps becomes true again once a previously-unavailable step becomes available", () => {
      collectionStore.stats.total_songs = 0;
      playerStore.currentSong = undefined;
      walkthroughStore.start();
      while (walkthroughStore.isActive) walkthroughStore.next();
      expect(walkthroughStore.hasPendingSteps).toBe(false);

      // The library gets its first songs later on.
      collectionStore.stats.total_songs = 3;

      expect(walkthroughStore.hasPendingSteps).toBe(true);
    });

    it("start('resume') only offers steps not yet seen, skipping ones already shown", () => {
      collectionStore.stats.total_songs = 0;
      playerStore.currentSong = undefined;
      walkthroughStore.start();
      while (walkthroughStore.isActive) walkthroughStore.next();

      // Now everything is available (library + playback), but sidebar/top-navigation/
      // library-folders were already seen on the first, emptier run.
      collectionStore.stats.total_songs = 5;
      playerStore.currentSong = { id: 1 } as any;
      walkthroughStore.start("resume");

      expect(walkthroughStore.currentStep.id).toBe("collection-view");
      expect(walkthroughStore.availableSteps.map((s) => s.id)).not.toContain("sidebar");
    });

    it("start('full') replays every available step regardless of prior viewing", () => {
      walkthroughStore.seenStepIds = new Set(["sidebar", "top-navigation"]);
      walkthroughStore.start("full");

      expect(walkthroughStore.currentStep.id).toBe("sidebar");
      expect(walkthroughStore.availableSteps.map((s) => s.id)).toContain("sidebar");
    });
  });
});
