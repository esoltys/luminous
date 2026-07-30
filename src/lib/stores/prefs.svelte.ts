import { invoke } from "@tauri-apps/api/core";

export type RatingStyle = "heart" | "stars";
export type SeekBarMode = "waveform" | "moodbar";

class PrefsStore {
  ratingStyle = $state<RatingStyle>("heart");
  seekBarMode = $state<SeekBarMode>("waveform");
  acoustidApiKey = $state<string>("");
  soundCuesEnabled = $state<boolean>(false);

  async init() {
    try {
      const settings = await invoke<Record<string, string>>("get_all_app_settings");
      if (settings?.rating_style === "stars" || settings?.rating_style === "heart") {
        this.ratingStyle = settings.rating_style;
      }
      if (settings?.seekbar_mode === "waveform" || settings?.seekbar_mode === "moodbar") {
        this.seekBarMode = settings.seekbar_mode;
      }
      if (settings?.acoustid_api_key) {
        this.acoustidApiKey = settings.acoustid_api_key;
      }
      if (settings?.sound_cues_enabled !== undefined) {
        this.soundCuesEnabled = settings.sound_cues_enabled === "true";
      }
    } catch (e) {
      console.error("Failed to load preference settings:", e);
    }
  }

  async setRatingStyle(style: RatingStyle) {
    this.ratingStyle = style;
    try {
      await invoke("set_app_setting", { key: "rating_style", value: style });
    } catch (e) {
      console.error("Failed to save rating style:", e);
    }
  }

  async toggleSeekBarMode() {
    this.seekBarMode = this.seekBarMode === "waveform" ? "moodbar" : "waveform";
    try {
      await invoke("set_app_setting", { key: "seekbar_mode", value: this.seekBarMode });
    } catch (e) {
      console.error("Failed to save seek bar mode:", e);
    }
  }

  async setAcoustidApiKey(key: string) {
    this.acoustidApiKey = key;
    try {
      await invoke("set_app_setting", { key: "acoustid_api_key", value: key });
    } catch (e) {
      console.error("Failed to save AcoustID API key:", e);
    }
  }

  async setSoundCuesEnabled(enabled: boolean) {
    this.soundCuesEnabled = enabled;
    try {
      await invoke("set_app_setting", { key: "sound_cues_enabled", value: enabled ? "true" : "false" });
    } catch (e) {
      console.error("Failed to save sound cues setting:", e);
    }
  }
}

export const prefs = new PrefsStore();
