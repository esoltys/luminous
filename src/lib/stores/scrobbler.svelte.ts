import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

type DiscordStatus = "connected" | "disconnected" | "not_running";

export const DEFAULT_DISCORD_CLIENT_ID = "1349887723725590558";

interface ScrobblerSettings {
  listenbrainz_enabled: boolean;
  listenbrainz_token: string;
  listenbrainz_username: string | null;
  scrobble_now_playing: boolean;
  scrobble_ratings: boolean;
  scrobble_paused: boolean;
  min_duration_secs: number;
  discord_enabled: boolean;
  discord_client_id: string;
  discord_show_album: boolean;
  discord_show_time: boolean;
}

interface ScrobbleCacheStatus {
  pending_count: number;
  last_error: string | null;
  last_attempt: number | null;
}

interface SyncFavouritesResult {
  total_favourites: number;
  synced: number;
  skipped_no_mbid: number;
  failed: number;
}

class ScrobblerStore {
  enabled = $state(false);
  token = $state("");
  username = $state<string | null>(null);
  nowPlayingEnabled = $state(true);
  ratingsEnabled = $state(true);
  paused = $state(false);
  minDurationSecs = $state(30);

  // Discord Rich Presence state (#958)
  discordEnabled = $state(false);
  discordClientId = $state(DEFAULT_DISCORD_CLIENT_ID);
  discordShowAlbum = $state(true);
  discordShowTime = $state(true);
  discordStatus = $state<DiscordStatus>("disconnected");
  isCheckingDiscord = $state(false);

  pendingCount = $state(0);
  lastError = $state<string | null>(null);
  lastAttempt = $state<number | null>(null);

  isValidating = $state(false);
  isFlushing = $state(false);
  isSyncingFavourites = $state(false);
  validationError = $state<string | null>(null);
  flushSuccessMessage = $state<string | null>(null);
  syncFavouritesResult = $state<SyncFavouritesResult | null>(null);
  syncFavouritesError = $state<string | null>(null);

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
      this.discordEnabled = settings.discord_enabled;
      this.discordClientId = settings.discord_client_id || DEFAULT_DISCORD_CLIENT_ID;
      this.discordShowAlbum = settings.discord_show_album;
      this.discordShowTime = settings.discord_show_time;
    } catch (e) {
      console.error("Failed to load scrobbler settings:", e);
    }

    try {
      await listen<ScrobblerSettings>("scrobbler-settings-changed", (event) => {
        const s = event.payload;
        this.enabled = s.listenbrainz_enabled;
        this.token = s.listenbrainz_token;
        this.username = s.listenbrainz_username;
        this.nowPlayingEnabled = s.scrobble_now_playing;
        this.ratingsEnabled = s.scrobble_ratings;
        this.paused = s.scrobble_paused;
        this.minDurationSecs = s.min_duration_secs;
        this.discordEnabled = s.discord_enabled;
        this.discordClientId = s.discord_client_id || DEFAULT_DISCORD_CLIENT_ID;
        this.discordShowAlbum = s.discord_show_album;
        this.discordShowTime = s.discord_show_time;
      });
    } catch (e) {
      console.error("Failed to register scrobbler-settings-changed listener:", e);
    }

    await this.refreshCacheStatus();
    if (this.discordEnabled) {
      await this.checkDiscordStatus();
    }
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
      discord_enabled: this.discordEnabled,
      discord_client_id: this.discordClientId,
      discord_show_album: this.discordShowAlbum,
      discord_show_time: this.discordShowTime,
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

  async syncFavourites() {
    this.isSyncingFavourites = true;
    this.syncFavouritesResult = null;
    this.syncFavouritesError = null;
    try {
      const res = await invoke<SyncFavouritesResult>("sync_favourites_to_listenbrainz");
      this.syncFavouritesResult = res;
      return res;
    } catch (err: any) {
      this.syncFavouritesError = typeof err === "string" ? err : err?.message ?? "Failed to sync favourites";
      return null;
    } finally {
      this.isSyncingFavourites = false;
    }
  }

  setToken(val: string) {
    if (this.token !== val) {
      this.token = val;
      this.username = null;
      this.enabled = false;
      this.saveSettings();
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

  async checkDiscordStatus() {
    this.isCheckingDiscord = true;
    try {
      this.discordStatus = await invoke<DiscordStatus>("get_discord_status");
    } catch (e) {
      console.error("Failed to check Discord status:", e);
      this.discordStatus = "disconnected";
    } finally {
      this.isCheckingDiscord = false;
    }
  }

  setDiscordEnabled(val: boolean) {
    this.discordEnabled = val;
    this.saveSettings();
    if (val) {
      this.checkDiscordStatus();
    }
  }

  setDiscordClientId(val: string) {
    this.discordClientId = val;
    this.saveSettings();
  }

  setDiscordShowAlbum(val: boolean) {
    this.discordShowAlbum = val;
    this.saveSettings();
  }

  setDiscordShowTime(val: boolean) {
    this.discordShowTime = val;
    this.saveSettings();
  }

  resetDiscordClientId() {
    this.discordClientId = DEFAULT_DISCORD_CLIENT_ID;
    this.saveSettings();
  }
}

export const scrobblerStore = new ScrobblerStore();
