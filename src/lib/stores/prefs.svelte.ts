import { invoke } from "@tauri-apps/api/core";

export type RatingStyle = "heart" | "stars";
export type SeekBarMode = "waveform" | "moodbar";
export type CollectionViewMode = "cards" | "rows";

class PrefsStore {
  ratingStyle = $state<RatingStyle>("heart");
  seekBarMode = $state<SeekBarMode>("waveform");
  acoustidApiKey = $state<string>("");
  albumsViewMode = $state<CollectionViewMode>("cards");
  artistsViewMode = $state<CollectionViewMode>("cards");

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
      if (settings?.albums_view_mode === "cards" || settings?.albums_view_mode === "rows") {
        this.albumsViewMode = settings.albums_view_mode;
      }
      if (settings?.artists_view_mode === "cards" || settings?.artists_view_mode === "rows") {
        this.artistsViewMode = settings.artists_view_mode;
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

  async setAlbumsViewMode(mode: CollectionViewMode) {
    this.albumsViewMode = mode;
    try {
      await invoke("set_app_setting", { key: "albums_view_mode", value: mode });
    } catch (e) {
      console.error("Failed to save albums view mode:", e);
    }
  }

  async setArtistsViewMode(mode: CollectionViewMode) {
    this.artistsViewMode = mode;
    try {
      await invoke("set_app_setting", { key: "artists_view_mode", value: mode });
    } catch (e) {
      console.error("Failed to save artists view mode:", e);
    }
  }
}

export const prefs = new PrefsStore();
