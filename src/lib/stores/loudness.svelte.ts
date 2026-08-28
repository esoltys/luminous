import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

/** Shared across Equalizer (settings UI) and SettingsFolders (background-activity
 *  notice) so both reflect the same enabled/remaining state without each
 *  polling separately or registering its own "loudness-analysis-progress"
 *  listener. */
class LoudnessStore {
  enabled = $state(false);
  analysisRemaining = $state(0);
  private initialized = false;

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

    await listen<{ remaining: number }>("loudness-analysis-progress", (event) => {
      this.analysisRemaining = event.payload.remaining;
    });
  }

  setEnabled(enabled: boolean) {
    this.enabled = enabled;
  }
}

export const loudnessStore = new LoudnessStore();
