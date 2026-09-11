import { invoke } from "@tauri-apps/api/core";
import { navigationStore } from "./navigation.svelte";
import { windowLayoutStore } from "./windowLayout.svelte";

type WalkthroughPlacement = "top" | "bottom" | "left" | "right" | "center";

interface WalkthroughStep {
  id: string;
  targetSelector: string;
  titleKey: string;
  descriptionKey: string;
  placement: WalkthroughPlacement;
  beforeStep?: () => void;
}

const STEPS: WalkthroughStep[] = [
  {
    id: "sidebar",
    targetSelector: '[data-walkthrough-target="sidebar"]',
    titleKey: "walkthrough.steps.sidebar.title",
    descriptionKey: "walkthrough.steps.sidebar.description",
    placement: "right",
  },
  {
    id: "top-navigation",
    targetSelector: '[data-walkthrough-target="top-navigation"]',
    titleKey: "walkthrough.steps.topNavigation.title",
    descriptionKey: "walkthrough.steps.topNavigation.description",
    placement: "bottom",
  },
  {
    id: "collection-view",
    targetSelector: '[data-walkthrough-target="collection-view"]',
    titleKey: "walkthrough.steps.collectionView.title",
    descriptionKey: "walkthrough.steps.collectionView.description",
    placement: "bottom",
    beforeStep: () => {
      // The cards/rows toggle only renders on the Albums/Artists sub-tabs
      // (not Songs or Genres) — force one of those so the target exists.
      navigationStore.activeTab = "collection";
      if (navigationStore.activeSubTab !== "albums" && navigationStore.activeSubTab !== "artists") {
        navigationStore.activeSubTab = "albums";
      }
    },
  },
  {
    id: "player-bar",
    targetSelector: '[data-walkthrough-target="player-bar"]',
    titleKey: "walkthrough.steps.playerBar.title",
    descriptionKey: "walkthrough.steps.playerBar.description",
    placement: "top",
  },
  {
    id: "right-panel",
    targetSelector: '[data-walkthrough-target="right-panel"]',
    titleKey: "walkthrough.steps.rightPanel.title",
    descriptionKey: "walkthrough.steps.rightPanel.description",
    placement: "left",
    beforeStep: () => {
      if (!windowLayoutStore.rightPanelOpen) {
        windowLayoutStore.toggleRightPanel();
      }
    },
  },
  {
    id: "library-folders",
    targetSelector: '[data-walkthrough-target="library-folders"]',
    titleKey: "walkthrough.steps.libraryFolders.title",
    descriptionKey: "walkthrough.steps.libraryFolders.description",
    placement: "right",
    beforeStep: () => {
      navigationStore.activeTab = "settings";
      invoke("set_app_setting", { key: "active_settings_tab", value: "folders" });
    },
  },
];

/** Persisted via the generic app_state key/value store (see RightPanel's
 * `right_panel_active_tab` / Sidebar's `active_settings_tab`) — a one-off
 * UI-only flag doesn't need its own Rust command or a UiPreferences field. */
const COMPLETED_SETTING_KEY = "walkthrough_completed";

class WalkthroughStore {
  isActive = $state(false);
  currentStepIndex = $state(0);
  hasCompleted = $state(false);

  readonly steps = STEPS;

  get totalSteps() {
    return this.steps.length;
  }

  get currentStep() {
    return this.steps[this.currentStepIndex];
  }

  async init() {
    try {
      const settings = await invoke<Record<string, string>>("get_all_app_settings");
      this.hasCompleted = settings?.[COMPLETED_SETTING_KEY] === "true";
    } catch (e) {
      console.error("Failed to load walkthrough completion state:", e);
    }
  }

  start() {
    this.currentStepIndex = 0;
    this.isActive = true;
    this.steps[0].beforeStep?.();
  }

  next() {
    if (this.currentStepIndex >= this.steps.length - 1) {
      this.finish();
      return;
    }
    this.currentStepIndex += 1;
    this.currentStep.beforeStep?.();
  }

  prev() {
    if (this.currentStepIndex === 0) return;
    this.currentStepIndex -= 1;
    this.currentStep.beforeStep?.();
  }

  skip() {
    if (!this.isActive) return;
    this.isActive = false;
    this.markCompleted();
  }

  finish() {
    this.isActive = false;
    this.markCompleted();
  }

  private markCompleted() {
    if (this.hasCompleted) return;
    this.hasCompleted = true;
    invoke("set_app_setting", { key: COMPLETED_SETTING_KEY, value: "true" }).catch((e) =>
      console.error("Failed to persist walkthrough completion:", e)
    );
  }
}

export const walkthroughStore = new WalkthroughStore();
