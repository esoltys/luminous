import { invoke } from "@tauri-apps/api/core";
import { createEventScope } from "../utils/eventScope";

/** Shared across Equalizer (settings UI) and SettingsSources (background-activity
 *  notice) so both reflect the same enabled/remaining state without each
 *  polling separately or registering its own "loudness-analysis-progress"
 *  listener. */
class LoudnessStore {
  enabled = $state(false);
  analysisRemaining = $state(0);
  private initialized = false;
  private events = createEventScope(import.meta.hot);

  async init() {
    if (this.initialized) return;
    this.initialized = true;

    try {
      const settings = await invoke<{ enabled: boolean }>("get_loudness_settings");
      this.enabled = settings.enabled;
    } catch (e) {
      console.error("Failed to load loudness settings:", e);
    }

    try {
      this.analysisRemaining = await invoke<number>("get_loudness_analysis_remaining");
    } catch (e) {
      console.error("Failed to load loudness analysis progress:", e);
    }

    await this.events.listen<{ remaining: number }>("loudness-analysis-progress", (event) => {
      this.analysisRemaining = event.payload.remaining;
    });
  }

  setEnabled(enabled: boolean) {
    this.enabled = enabled;
  }

  /** Stops listening to backend events. */
  dispose() {
    this.events.dispose();
    this.initialized = false;
  }
}

export const loudnessStore = new LoudnessStore();
