import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { openExternalUrl } from "../utils/openExternalUrl";

interface MusicBrainzAuthState {
  is_logged_in: boolean;
  username: string | null;
  email: string | null;
}

interface MusicBrainzUserStats {
  username: string;
  collections_count: number;
  releases_count: number;
  cached_at: number;
}

class MusicBrainzStore {
  isLoggedIn = $state(false);
  username = $state<string | null>(null);
  email = $state<string | null>(null);
  stats = $state<MusicBrainzUserStats | null>(null);

  isLoading = $state(false);
  isAuthorizing = $state(false);
  authError = $state<string | null>(null);
  statsError = $state<string | null>(null);

  private initialized = false;

  async init() {
    if (this.initialized) return;
    this.initialized = true;

    try {
      const state = await invoke<MusicBrainzAuthState>("get_musicbrainz_auth_state");
      if (state && typeof state === "object") {
        this.isLoggedIn = !!state.is_logged_in;
        this.username = state.username ?? null;
        this.email = state.email ?? null;
      }

      if (this.isLoggedIn) {
        this.refreshStats(false).catch(() => {});
      }
    } catch (e) {
      console.error("Failed to load MusicBrainz auth state:", e);
    }

    try {
      await listen<MusicBrainzAuthState>("musicbrainz-auth-changed", (event) => {
        const s = event.payload;
        this.isLoggedIn = s.is_logged_in;
        this.username = s.username;
        this.email = s.email;
        this.isAuthorizing = false;
        this.isLoading = false;
        this.authError = null;

        if (this.isLoggedIn) {
          this.refreshStats(true).catch(() => {});
        } else {
          this.stats = null;
        }
      });
    } catch (e) {
      console.error("Failed to attach musicbrainz-auth-changed listener:", e);
    }

    try {
      await listen<string>("musicbrainz-auth-error", (event) => {
        this.authError = event.payload;
        this.isLoading = false;
        this.isAuthorizing = false;
      });
    } catch (e) {
      console.error("Failed to attach musicbrainz-auth-error listener:", e);
    }
  }

  async startLogin(preferLoopback = true): Promise<string> {
    this.isAuthorizing = true;
    this.authError = null;
    try {
      const authUrl = await invoke<string>("start_musicbrainz_login", {
        preferLoopback,
      });
      await openExternalUrl(authUrl);
      return authUrl;
    } catch (err) {
      this.isAuthorizing = false;
      this.authError = typeof err === "string" ? err : "Failed to start MusicBrainz login";
      throw err;
    }
  }

  async submitAuthCode(code: string): Promise<MusicBrainzAuthState> {
    this.isLoading = true;
    this.authError = null;
    try {
      const state = await invoke<MusicBrainzAuthState>("submit_musicbrainz_auth_code", {
        code: code.trim(),
      });
      this.isLoggedIn = state.is_logged_in;
      this.username = state.username;
      this.email = state.email;
      this.isAuthorizing = false;
      this.isLoading = false;
      if (this.isLoggedIn) {
        this.refreshStats(true).catch(() => {});
      }
      return state;
    } catch (err) {
      this.isLoading = false;
      this.authError = typeof err === "string" ? err : "Failed to verify authorization code";
      throw err;
    }
  }

  async cancelLogin() {
    this.isAuthorizing = false;
    this.isLoading = false;
    this.authError = null;
    try {
      await invoke("cancel_musicbrainz_login");
    } catch (e) {
      console.error("Failed to cancel login:", e);
    }
  }

  async refreshStats(forceRefresh = false) {
    if (!this.isLoggedIn) return;
    this.statsError = null;
    try {
      const stats = await invoke<MusicBrainzUserStats>("get_musicbrainz_user_stats", {
        forceRefresh,
      });
      this.stats = stats;
    } catch (err) {
      this.statsError = typeof err === "string" ? err : "Failed to load MusicBrainz stats";
      console.error("Failed to load MusicBrainz stats:", err);
    }
  }

  async logout() {
    this.isLoading = true;
    try {
      await invoke("logout_musicbrainz");
      this.isLoggedIn = false;
      this.username = null;
      this.email = null;
      this.stats = null;
    } catch (err) {
      console.error("Failed to log out of MusicBrainz:", err);
    } finally {
      this.isLoading = false;
    }
  }
}

export const musicbrainzStore = new MusicBrainzStore();
