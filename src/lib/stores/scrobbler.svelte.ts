import { invoke } from "@tauri-apps/api/core";

interface ScrobblerSettings {
  listenbrainz_enabled: boolean;
  listenbrainz_token: string;
  listenbrainz_username: string | null;
  scrobble_now_playing: boolean;
  scrobble_ratings: boolean;
  scrobble_paused: boolean;
  min_duration_secs: number;
}

interface ScrobbleCacheStatus {
  pending_count: number;
  last_error: string | null;
  last_attempt: number | null;
}

class ScrobblerStore {
  enabled = $state(false);
  token = $state("");
  username = $state<string | null>(null);
  nowPlayingEnabled = $state(true);
  ratingsEnabled = $state(true);
  paused = $state(false);
  minDurationSecs = $state(30);

  pendingCount = $state(0);
  lastError = $state<string | null>(null);
  lastAttempt = $state<number | null>(null);

  isValidating = $state(false);
  isFlushing = $state(false);
  validationError = $state<string | null>(null);
  flushSuccessMessage = $state<string | null>(null);

  private initialized = false;

  async init() {
    if (this.initialized) return;
    this.initialized = true;

    try {
      const settings = await invoke<ScrobblerSettings>("get_scrobbler_settings");
      this.enabled = settings.listenbrainz_enabled;
      this.token = settings.listenbrainz_token;
      this.username = settings.listenbrainz_username;
      this.nowPlayingEnabled = settings.scrobble_now_playing;
      this.ratingsEnabled = settings.scrobble_ratings;
      this.paused = settings.scrobble_paused;
      this.minDurationSecs = settings.min_duration_secs;
    } catch (e) {
      console.error("Failed to load scrobbler settings:", e);
    }

    await this.refreshCacheStatus();
  }

  async saveSettings() {
    const settings: ScrobblerSettings = {
      listenbrainz_enabled: this.enabled,
      listenbrainz_token: this.token,
      listenbrainz_username: this.username,
      scrobble_now_playing: this.nowPlayingEnabled,
      scrobble_ratings: this.ratingsEnabled,
      scrobble_paused: this.paused,
      min_duration_secs: this.minDurationSecs,
    };

    try {
      await invoke("set_scrobbler_settings", { settings });
    } catch (e) {
      console.error("Failed to persist scrobbler settings:", e);
    }
  }

  async validateToken(tokenToTest?: string) {
    const targetToken = (tokenToTest ?? this.token).trim();
    if (!targetToken) {
      this.validationError = "Please enter a user token";
      return false;
    }

    this.isValidating = true;
    this.validationError = null;

    try {
      const username = await invoke<string>("validate_listenbrainz_token", { token: targetToken });
      this.username = username;
      this.token = targetToken;
      await this.saveSettings();
      return true;
    } catch (err: any) {
      this.validationError = typeof err === "string" ? err : err?.message ?? "Failed to validate token";
      return false;
    } finally {
      this.isValidating = false;
    }
  }

  async flushCache() {
    this.isFlushing = true;
    this.flushSuccessMessage = null;
    try {
      const count = await invoke<number>("flush_scrobble_cache");
      this.flushSuccessMessage = count > 0 ? `Submitted ${count} pending listen${count === 1 ? "" : "s"}` : "Queue is empty";
      await this.refreshCacheStatus();
      setTimeout(() => {
        this.flushSuccessMessage = null;
      }, 4000);
    } catch (err: any) {
      this.lastError = typeof err === "string" ? err : err?.message ?? "Flush failed";
      await this.refreshCacheStatus();
    } finally {
      this.isFlushing = false;
    }
  }

  async refreshCacheStatus() {
    try {
      const status = await invoke<ScrobbleCacheStatus>("get_scrobble_cache_status");
      this.pendingCount = status.pending_count;
      this.lastError = status.last_error;
      this.lastAttempt = status.last_attempt;
    } catch (e) {
      console.error("Failed to fetch scrobble cache status:", e);
    }
  }

  setEnabled(val: boolean) {
    this.enabled = val;
    this.saveSettings();
  }

  setNowPlayingEnabled(val: boolean) {
    this.nowPlayingEnabled = val;
    this.saveSettings();
  }

  setRatingsEnabled(val: boolean) {
    this.ratingsEnabled = val;
    this.saveSettings();
  }

  setPaused(val: boolean) {
    this.paused = val;
    this.saveSettings();
  }
}

export const scrobblerStore = new ScrobblerStore();
