import { invoke } from "@tauri-apps/api/core";
import { createEventScope } from "../utils/eventScope";
import { openExternalUrl } from "../utils/openExternalUrl";
import { i18n } from "./i18n.svelte";

type DiscordStatus = "connected" | "disconnected" | "not_running";

export const DEFAULT_DISCORD_CLIENT_ID = "1548913001715990610";

interface ScrobblerSettings {
  listenbrainz_enabled: boolean;
  listenbrainz_token: string;
  listenbrainz_username: string | null;
  critiquebrainz_user_id: string;
  lastfm_enabled?: boolean;
  lastfm_session_key?: string;
  lastfm_username?: string | null;
  lastfm_api_key?: string;
  lastfm_api_secret?: string;
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
  listenbrainz_pending_count?: number;
  lastfm_pending_count?: number;
  last_error: string | null;
  last_attempt: number | null;
}

interface SyncRatingsResult {
  pulled_loved: number;
  pulled_hated: number;
  pulled_song_ratings: number;
  pulled_album_ratings: number;
  pushed: number;
  failed: number;
  critiquebrainz_checked: boolean;
}

class ScrobblerStore {
  enabled = $state(false);
  token = $state("");
  username = $state<string | null>(null);
  critiquebrainzUserId = $state("");
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

  // Last.fm state (#1954)
  lastfmEnabled = $state(false);
  lastfmUsername = $state<string | null>(null);
  lastfmSessionKey = $state("");
  lastfmPendingToken = $state<string | null>(null);
  lastfmPendingAuthUrl = $state<string | null>(null);
  isStartingLastfm = $state(false);
  isFinishingLastfm = $state(false);
  lastfmAuthError = $state<string | null>(null);

  pendingCount = $state(0);
  listenbrainzPendingCount = $state(0);
  lastfmPendingCount = $state(0);
  lastError = $state<string | null>(null);
  lastAttempt = $state<number | null>(null);

  isValidating = $state(false);
  isFlushing = $state(false);
  isSyncingRatings = $state(false);
  validationError = $state<string | null>(null);
  flushSuccessMessage = $state<string | null>(null);
  syncRatingsResult = $state<SyncRatingsResult | null>(null);
  syncRatingsError = $state<string | null>(null);

  private initialized = false;
  private events = createEventScope(import.meta.hot);

  /**
   * What the user-facing indicators (sidebar dot, profile popover badge) should
   * show. `paused` only counts while scrobbling is enabled, so a stale paused
   * flag on a disabled scrobbler reads as inactive.
   */
  get status(): "inactive" | "paused" | "active" {
    if (!this.enabled && !this.lastfmEnabled) return "inactive";
    return this.paused ? "paused" : "active";
  }

  async init() {
    if (this.initialized) return;
    this.initialized = true;

    try {
      const settings = await invoke<ScrobblerSettings>("get_scrobbler_settings");
      this.enabled = settings.listenbrainz_enabled;
      this.token = settings.listenbrainz_token;
      this.username = settings.listenbrainz_username;
      this.critiquebrainzUserId = settings.critiquebrainz_user_id;
      this.lastfmEnabled = settings.lastfm_enabled ?? false;
      this.lastfmSessionKey = settings.lastfm_session_key ?? "";
      this.lastfmUsername = settings.lastfm_username ?? null;
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
      await this.events.listen<ScrobblerSettings>("scrobbler-settings-changed", (event) => {
        const s = event.payload;
        this.enabled = s.listenbrainz_enabled;
        this.token = s.listenbrainz_token;
        this.username = s.listenbrainz_username;
        this.critiquebrainzUserId = s.critiquebrainz_user_id;
        this.lastfmEnabled = s.lastfm_enabled ?? false;
        this.lastfmSessionKey = s.lastfm_session_key ?? "";
        this.lastfmUsername = s.lastfm_username ?? null;
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
      critiquebrainz_user_id: this.critiquebrainzUserId,
      lastfm_enabled: this.lastfmEnabled,
      lastfm_session_key: this.lastfmSessionKey,
      lastfm_username: this.lastfmUsername,
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
      this.validationError = i18n.t("listenbrainz.tokenRequired");
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
      this.validationError = typeof err === "string" ? err : err?.message ?? i18n.t("listenbrainz.validateFailed");
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
      this.flushSuccessMessage = count > 0 ? i18n.plural("listenbrainz.submittedPending", count) : i18n.t("listenbrainz.queueEmpty");
      await this.refreshCacheStatus();
      setTimeout(() => {
        this.flushSuccessMessage = null;
      }, 4000);
    } catch (err: any) {
      this.lastError = typeof err === "string" ? err : err?.message ?? i18n.t("listenbrainz.flushFailed");
      await this.refreshCacheStatus();
    } finally {
      this.isFlushing = false;
    }
  }

  async refreshCacheStatus() {
    try {
      const status = await invoke<ScrobbleCacheStatus>("get_scrobble_cache_status");
      this.pendingCount = status.pending_count;
      this.listenbrainzPendingCount = status.listenbrainz_pending_count ?? 0;
      this.lastfmPendingCount = status.lastfm_pending_count ?? 0;
      this.lastError = status.last_error;
      this.lastAttempt = status.last_attempt;
    } catch (e) {
      console.error("Failed to fetch scrobble cache status:", e);
    }
  }

  async startLastfmAuth() {
    this.isStartingLastfm = true;
    this.lastfmAuthError = null;
    try {
      const res = await invoke<{ token: string; auth_url: string }>("start_lastfm_login");
      this.lastfmPendingToken = res.token;
      this.lastfmPendingAuthUrl = res.auth_url;
      await openExternalUrl(res.auth_url);
    } catch (err: any) {
      this.lastfmAuthError = typeof err === "string" ? err : err?.message ?? i18n.t("lastfm.authFailed");
    } finally {
      this.isStartingLastfm = false;
    }
  }

  async finishLastfmAuth() {
    if (!this.lastfmPendingToken) return false;
    this.isFinishingLastfm = true;
    this.lastfmAuthError = null;
    try {
      const session = await invoke<{ key: string; username: string }>("finish_lastfm_login", {
        token: this.lastfmPendingToken,
      });
      this.lastfmUsername = session.username;
      this.lastfmSessionKey = session.key;
      this.lastfmEnabled = true;
      this.lastfmPendingToken = null;
      this.lastfmPendingAuthUrl = null;
      await this.refreshCacheStatus();
      return true;
    } catch (err: any) {
      this.lastfmAuthError = typeof err === "string" ? err : err?.message ?? i18n.t("lastfm.authFailed");
      return false;
    } finally {
      this.isFinishingLastfm = false;
    }
  }

  cancelLastfmAuth() {
    this.lastfmPendingToken = null;
    this.lastfmPendingAuthUrl = null;
    this.lastfmAuthError = null;
  }

  async disconnectLastfm() {
    try {
      await invoke("disconnect_lastfm");
      this.lastfmEnabled = false;
      this.lastfmUsername = null;
      this.lastfmSessionKey = "";
      this.lastfmPendingToken = null;
      this.lastfmPendingAuthUrl = null;
      this.lastfmAuthError = null;
    } catch (e) {
      console.error("Failed to disconnect Last.fm:", e);
    }
  }

  setLastfmEnabled(val: boolean) {
    this.lastfmEnabled = val;
    this.saveSettings();
  }

  async syncRatings() {
    this.isSyncingRatings = true;
    this.syncRatingsResult = null;
    this.syncRatingsError = null;
    try {
      const res = await invoke<SyncRatingsResult>("sync_ratings_to_listenbrainz");
      this.syncRatingsResult = res;
      return res;
    } catch (err: any) {
      this.syncRatingsError = typeof err === "string" ? err : err?.message ?? i18n.t("listenbrainz.syncRatingsError");
      return null;
    } finally {
      this.isSyncingRatings = false;
    }
  }

  setCritiquebrainzUserId(val: string) {
    if (this.critiquebrainzUserId !== val) {
      this.critiquebrainzUserId = val;
      this.saveSettings();
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

  disconnectListenBrainz() {
    this.token = "";
    this.username = null;
    this.enabled = false;
    this.validationError = null;
    this.saveSettings();
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

  async setDiscordEnabled(val: boolean) {
    this.discordEnabled = val;
    await this.saveSettings();
    if (val) {
      await this.checkDiscordStatus();
    } else {
      this.discordStatus = "disconnected";
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

  /** Stops listening to backend events. */
  dispose() {
    this.events.dispose();
    this.initialized = false;
  }
}

export const scrobblerStore = new ScrobblerStore();
