import { invoke } from "@tauri-apps/api/core";
import { navigationStore } from "./navigation.svelte";
import { windowLayoutStore } from "./windowLayout.svelte";
import { playerStore } from "./player.svelte";
import { collectionStore } from "./collection.svelte";

type WalkthroughPlacement = "top" | "bottom" | "left" | "right" | "center";

interface WalkthroughStep {
  id: string;
  targetSelector: string;
  titleKey: string;
  descriptionKey: string;
  placement: WalkthroughPlacement;
  beforeStep?: () => void;
  /** Cleanup when leaving this step — moving to any other step (either
   * direction) or ending the tour. Pairs with beforeStep for steps that
   * mutate persistent UI state (e.g. force-opening the right panel) so that
   * state gets restored the moment the step is no longer shown, rather than
   * only at the very end of the tour — otherwise a later step (or the tour
   * finishing) can be reached with that state left dangling, with no
   * visible control left to undo it (see the right-panel step below). */
  afterStep?: () => void;
  /** Steps whose target only exists in certain app states (something
   * playing, a non-empty library) declare this so start()/next()/prev()
   * skip straight past them instead of relying on WalkthroughOverlay's
   * missing-target fallback — that fallback races against reactive layout
   * effects (e.g. the Collection tab bouncing back to Home while the
   * library is empty) and can cascade through several steps at once. */
  isAvailable?: () => boolean;
}

// Tracks whether the right-panel step itself opened the panel (vs. it
// already being open), so afterStep only closes what beforeStep opened —
// module-scoped rather than a class field since STEPS is defined before the
// WalkthroughStore class (closures over this var are fine either way, since
// they only run well after module load).
let openedRightPanelForTour = false;

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
    // Collection/Playlists/Lyrics are hidden from an empty library (see
    // +layout.svelte's bounce-back-to-Home effect), so this step can only
    // ever land on a library that already has songs.
    isAvailable: () => collectionStore.stats.total_songs > 0,
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
    id: "player-bar-cover",
    targetSelector: '[data-walkthrough-target="player-bar-cover"]',
    titleKey: "walkthrough.steps.playerBarCover.title",
    descriptionKey: "walkthrough.steps.playerBarCover.description",
    placement: "top",
    // PlayerBar only mounts while something is playing (see +layout.svelte).
    isAvailable: () => !!playerStore.currentSong,
  },
  {
    id: "player-bar-controls",
    targetSelector: '[data-walkthrough-target="player-bar-controls"]',
    titleKey: "walkthrough.steps.playerBarControls.title",
    descriptionKey: "walkthrough.steps.playerBarControls.description",
    placement: "top",
    isAvailable: () => !!playerStore.currentSong,
  },
  {
    id: "player-bar-toolbar",
    targetSelector: '[data-walkthrough-target="player-bar-toolbar"]',
    titleKey: "walkthrough.steps.playerBarToolbar.title",
    descriptionKey: "walkthrough.steps.playerBarToolbar.description",
    placement: "top",
    isAvailable: () => !!playerStore.currentSong,
  },
  {
    id: "right-panel",
    targetSelector: '[data-walkthrough-target="right-panel"]',
    titleKey: "walkthrough.steps.rightPanel.title",
    descriptionKey: "walkthrough.steps.rightPanel.description",
    placement: "left",
    // The panel's own close control (the Info toggle in the player bar's
    // button row) is only reachable while something's playing — with
    // nothing playing there'd be no way back except the Ctrl+I shortcut.
    isAvailable: () => !!playerStore.currentSong,
    beforeStep: () => {
      if (!windowLayoutStore.rightPanelOpen) {
        windowLayoutStore.toggleRightPanel();
        openedRightPanelForTour = true;
      }
    },
    afterStep: () => {
      if (openedRightPanelForTour) {
        if (windowLayoutStore.rightPanelOpen) {
          windowLayoutStore.toggleRightPanel();
        }
        openedRightPanelForTour = false;
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
 * `right_panel_active_tab` / Sidebar's `active_settings_tab`) — one-off UI
 * state doesn't need its own Rust command or a UiPreferences field. Tracks
 * which step *ids* have been shown at least once (comma-joined), rather than
 * a single "completed" flag — a step that was unreachable on a first,
 * empty-library run (e.g. the player-bar steps) should still be offered
 * later once it becomes reachable, instead of being hidden forever just
 * because the tour as a whole was "finished". */
const SEEN_STEPS_SETTING_KEY = "walkthrough_seen_steps";
/** Superseded by SEEN_STEPS_SETTING_KEY, but still read once on init() to
 * migrate existing users: someone who fully completed the old single-flag
 * tour shouldn't suddenly have it resume under the new per-step scheme. */
const LEGACY_COMPLETED_SETTING_KEY = "walkthrough_completed";

type WalkthroughRunMode =
  /** Manual replay ("Take a quick tour") — shows every currently-available
   * step regardless of prior viewing. */
  | "full"
  /** Auto-continuation on launch — shows only available steps not yet seen,
   * so returning users aren't re-shown what they've already been through. */
  | "resume";

class WalkthroughStore {
  isActive = $state(false);
  currentStepIndex = $state(0);
  seenStepIds = $state<Set<string>>(new Set());

  private runMode: WalkthroughRunMode = "full";
  private visitedThisRun = new Set<string>();

  readonly steps = STEPS;

  get totalSteps() {
    return this.steps.length;
  }

  get currentStep() {
    return this.steps[this.currentStepIndex];
  }

  /** Steps reachable *in this run* — used for the progress dots and the
   * Next/Finish label so the indicator always matches what's actually being
   * shown (no gaps for steps unavailable right now, and none already seen
   * when resuming) instead of a fixed count against the full step list. */
  get availableSteps() {
    return this.steps.filter((step) => this.isReachable(step));
  }

  get currentAvailableIndex() {
    return this.availableSteps.findIndex((step) => step.id === this.currentStep?.id);
  }

  /** True once every currently-available step has been seen — nothing left
   * to auto-resume with next launch (new steps can still become pending
   * later, e.g. once the library gets its first song or something plays). */
  get hasPendingSteps() {
    return this.steps.some((step) => this.isAvailableNow(step) && !this.seenStepIds.has(step.id));
  }

  async init() {
    try {
      const settings = await invoke<Record<string, string>>("get_all_app_settings");
      const seenRaw = settings?.[SEEN_STEPS_SETTING_KEY];
      if (seenRaw) {
        this.seenStepIds = new Set(seenRaw.split(",").filter(Boolean));
      } else if (settings?.[LEGACY_COMPLETED_SETTING_KEY] === "true") {
        this.seenStepIds = new Set(this.steps.map((step) => step.id));
        this.persistSeenSteps();
      }
    } catch (e) {
      console.error("Failed to load walkthrough progress:", e);
    }
  }

  private isAvailableNow(step: WalkthroughStep): boolean {
    return !step.isAvailable || step.isAvailable();
  }

  private isReachable(step: WalkthroughStep): boolean {
    if (!this.isAvailableNow(step)) return false;
    if (this.runMode === "resume" && this.seenStepIds.has(step.id)) return false;
    return true;
  }

  /** First index at/after `from` (stepping by `direction`) whose step is
   * reachable in this run, or -1 if none remain. */
  private findReachableIndex(from: number, direction: 1 | -1): number {
    for (let i = from; i >= 0 && i < this.steps.length; i += direction) {
      if (this.isReachable(this.steps[i])) return i;
    }
    return -1;
  }

  /** @param mode "full" (default) replays every available step — used by
   * manual "Take a quick tour" entry points. "resume" only offers steps not
   * yet seen — used to auto-continue the tour across launches. */
  start(mode: WalkthroughRunMode = "full") {
    this.runMode = mode;
    this.visitedThisRun = new Set();
    openedRightPanelForTour = false;
    const idx = this.findReachableIndex(0, 1);
    if (idx === -1) return;
    this.currentStepIndex = idx;
    this.isActive = true;
    this.visitedThisRun.add(this.steps[idx].id);
    this.steps[idx].beforeStep?.();
  }

  next() {
    const idx = this.findReachableIndex(this.currentStepIndex + 1, 1);
    if (idx === -1) {
      this.currentStep.afterStep?.();
      this.finish();
      return;
    }
    this.currentStep.afterStep?.();
    this.currentStepIndex = idx;
    this.visitedThisRun.add(this.currentStep.id);
    this.currentStep.beforeStep?.();
  }

  prev() {
    const idx = this.findReachableIndex(this.currentStepIndex - 1, -1);
    if (idx === -1) return;
    this.currentStep.afterStep?.();
    this.currentStepIndex = idx;
    this.currentStep.beforeStep?.();
  }

  skip() {
    if (!this.isActive) return;
    this.currentStep.afterStep?.();
    this.isActive = false;
    this.commitVisited();
  }

  finish() {
    this.isActive = false;
    this.commitVisited();
  }

  /** Marks every step actually shown this run as seen — on finish() that's
   * every reachable step; on skip() only the ones reached before bailing,
   * so anything never shown stays pending for a future resume. */
  private commitVisited() {
    let changed = false;
    const next = new Set(this.seenStepIds);
    for (const id of this.visitedThisRun) {
      if (!next.has(id)) {
        next.add(id);
        changed = true;
      }
    }
    if (changed) {
      this.seenStepIds = next;
      this.persistSeenSteps();
    }
  }

  private persistSeenSteps() {
    invoke("set_app_setting", {
      key: SEEN_STEPS_SETTING_KEY,
      value: Array.from(this.seenStepIds).join(","),
    }).catch((e) => console.error("Failed to persist walkthrough progress:", e));
  }
}

export const walkthroughStore = new WalkthroughStore();
