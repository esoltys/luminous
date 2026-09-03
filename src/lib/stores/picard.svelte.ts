import { invoke } from "@tauri-apps/api/core";

/** Whether/where MusicBrainz Picard was found on this machine (#367) —
 * checked once at startup so every "Open in Picard" action can disable
 * itself instead of always showing and failing on click, and so the
 * Settings integration card can show the resolved path. */
class PicardStore {
  path = $state<string | null>(null);
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
  }
}

export const picardStore = new PicardStore();
