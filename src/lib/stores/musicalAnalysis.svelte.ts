import { invoke } from "@tauri-apps/api/core";
import { createEventScope } from "../utils/eventScope";

/** How many songs are still waiting for background tempo/key detection (#1943), so Settings can
 *  explain the disk activity. Fed by the initial count plus "musical-analysis-progress" events. */
class MusicalAnalysisStore {
  remaining = $state(0);
  private initialized = false;
  private events = createEventScope(import.meta.hot);

  async init() {
    if (this.initialized) return;
    this.initialized = true;

    try {
      this.remaining = await invoke<number>("get_musical_analysis_remaining");
    } catch (e) {
      console.error("Failed to load tempo/key analysis progress:", e);
    }

    await this.events.listen<{ remaining: number }>("musical-analysis-progress", (event) => {
      this.remaining = event.payload.remaining;
    });
  }

  /** Stops listening to backend events. */
  dispose() {
    this.events.dispose();
    this.initialized = false;
  }
}

export const musicalAnalysisStore = new MusicalAnalysisStore();
