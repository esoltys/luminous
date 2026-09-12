import { invoke } from "@tauri-apps/api/core";

/** Whether/where MusicBrainz Picard was found on this machine (#367) —
 * checked once at startup so every "Open in Picard" action can disable
 * itself instead of always showing and failing on click, and so the
 * Settings integration card can show the resolved path. */
class PicardStore {
  path = $state<string | null>(null);
  missingPlaylistEnabled = $state(true);
  private initialized = false;

  get available(): boolean {
    return this.path !== null;
  }

  async init() {
    if (this.initialized) return;
    this.initialized = true;
    await this.refresh();
  }

  async refresh() {
    try {
      this.path = await invoke<string | null>("get_picard_path");
    } catch (err) {
      console.error("Failed to check MusicBrainz Picard availability:", err);
      this.path = null;
    }
    try {
      const settings = await invoke<Record<string, string>>("get_all_app_settings");
      if (settings && settings.picard_missing_playlist_enabled !== undefined) {
        this.missingPlaylistEnabled = settings.picard_missing_playlist_enabled !== "false";
      }
    } catch {
      // In tests where get_all_app_settings is not mocked, keep default
    }
  }

  async setMissingPlaylistEnabled(val: boolean) {
    this.missingPlaylistEnabled = val;
    try {
      await invoke("set_app_setting", {
        key: "picard_missing_playlist_enabled",
        value: val ? "true" : "false",
      });
    } catch (err) {
      console.error("Failed to save picard_missing_playlist_enabled setting:", err);
    }
  }
}

export const picardStore = new PicardStore();
