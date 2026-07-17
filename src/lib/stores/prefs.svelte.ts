import { invoke } from "@tauri-apps/api/core";

export type RatingStyle = "heart" | "stars";

class PrefsStore {
  ratingStyle = $state<RatingStyle>("heart");

  async init() {
    try {
      const settings = await invoke<Record<string, string>>("get_all_app_settings");
      if (settings?.rating_style === "stars" || settings?.rating_style === "heart") {
        this.ratingStyle = settings.rating_style;
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
}

export const prefs = new PrefsStore();
