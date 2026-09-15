import { invoke } from "@tauri-apps/api/core";

// One-off first-run flag, persisted the same lightweight way as
// walkthrough_completed (see walkthrough.svelte.ts) — a single UI-only
// boolean doesn't need its own Rust field.
const SEEN_SETTING_KEY = "welcome_seen";

class WelcomeStore {
  hasSeen = $state(false);
  initialized = $state(false);

  async init() {
    try {
      const settings = await invoke<Record<string, string>>("get_all_app_settings");
      this.hasSeen = settings?.[SEEN_SETTING_KEY] === "true";
    } catch (e) {
      console.error("Failed to load welcome-screen state:", e);
      // Fail open: never trap the user behind a screen we couldn't confirm the state of.
      this.hasSeen = true;
    } finally {
      this.initialized = true;
    }
  }

  markSeen() {
    if (this.hasSeen) return;
    this.hasSeen = true;
    invoke("set_app_setting", { key: SEEN_SETTING_KEY, value: "true" }).catch((e) =>
      console.error("Failed to persist welcome-screen state:", e)
    );
  }
}

export const welcomeStore = new WelcomeStore();
