import { describe, it, expect, vi, beforeEach } from "vitest";
import { scrobblerStore } from "./scrobbler.svelte";
import { invoke } from "@tauri-apps/api/core";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
}));

describe("scrobblerStore", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it("reports status: paused counts only while scrobbling is enabled", () => {
    scrobblerStore.enabled = false;
    scrobblerStore.paused = true;
    expect(scrobblerStore.status).toBe("inactive");
    scrobblerStore.enabled = true;
    expect(scrobblerStore.status).toBe("paused");
    scrobblerStore.paused = false;
    expect(scrobblerStore.status).toBe("active");
  });

  it("initializes settings and cache status from backend", async () => {
    vi.mocked(invoke).mockImplementation((cmd: string) => {
      if (cmd === "get_scrobbler_settings") {
        return Promise.resolve({
          listenbrainz_enabled: true,
          listenbrainz_token: "test-token-123",
          listenbrainz_username: "soltys",
          scrobble_now_playing: true,
          scrobble_ratings: false,
          scrobble_paused: false,
          min_duration_secs: 30,
        });
      }
      if (cmd === "get_scrobble_cache_status") {
        return Promise.resolve({
          pending_count: 5,
          last_error: null,
          last_attempt: 1725690000,
        });
      }
      return Promise.resolve(null);
    });

    await scrobblerStore.init();

    expect(scrobblerStore.enabled).toBe(true);
    expect(scrobblerStore.token).toBe("test-token-123");
    expect(scrobblerStore.username).toBe("soltys");
    expect(scrobblerStore.ratingsEnabled).toBe(false);
    expect(scrobblerStore.pendingCount).toBe(5);
  });

  it("validates token and saves returned username", async () => {
    vi.mocked(invoke).mockImplementation((cmd: string, args?: any) => {
      if (cmd === "validate_listenbrainz_token") {
        if (args?.token === "valid-token") {
          return Promise.resolve("alice");
        }
        return Promise.reject("Invalid token");
      }
      if (cmd === "set_scrobbler_settings") {
        return Promise.resolve(null);
      }
      return Promise.resolve(null);
    });

    const success = await scrobblerStore.validateToken("valid-token");
    expect(success).toBe(true);
    expect(scrobblerStore.username).toBe("alice");
    expect(scrobblerStore.validationError).toBeNull();

    const failure = await scrobblerStore.validateToken("bad-token");
    expect(failure).toBe(false);
    expect(scrobblerStore.validationError).toBe("Invalid token");
  });

  it("flushes cache and updates status", async () => {
    vi.mocked(invoke).mockImplementation((cmd: string) => {
      if (cmd === "flush_scrobble_cache") {
        return Promise.resolve(3);
      }
      if (cmd === "get_scrobble_cache_status") {
        return Promise.resolve({
          pending_count: 0,
          last_error: null,
          last_attempt: 1_700_000_000_000,
        });
      }
      return Promise.resolve(null);
    });

    await scrobblerStore.flushCache();
    expect(scrobblerStore.flushSuccessMessage).toContain("Submitted 3 pending listens");
    expect(scrobblerStore.pendingCount).toBe(0);
  });

  it("syncs ratings with listenbrainz and updates state", async () => {
    vi.mocked(invoke).mockImplementation((cmd: string) => {
      if (cmd === "sync_ratings_to_listenbrainz") {
        return Promise.resolve({
          pulled_loved: 4,
          pulled_hated: 1,
          pulled_song_ratings: 2,
          pulled_album_ratings: 3,
          pushed: 5,
          failed: 0,
          critiquebrainz_checked: true,
        });
      }
      return Promise.resolve(null);
    });

    const res = await scrobblerStore.syncRatings();
    expect(res).not.toBeNull();
    expect(scrobblerStore.syncRatingsResult?.pulled_loved).toBe(4);
    expect(scrobblerStore.syncRatingsResult?.pushed).toBe(5);
    expect(scrobblerStore.syncRatingsError).toBeNull();
  });

  it("surfaces a ratings sync error", async () => {
    vi.mocked(invoke).mockRejectedValue("ListenBrainz username is unknown");
    expect(await scrobblerStore.syncRatings()).toBeNull();
    expect(scrobblerStore.syncRatingsError).toBe("ListenBrainz username is unknown");
    expect(scrobblerStore.syncRatingsResult).toBeNull();
  });

  it("updates discord settings and checks status", async () => {
    vi.mocked(invoke).mockImplementation((cmd: string) => {
      if (cmd === "set_scrobbler_settings") {
        return Promise.resolve(null);
      }
      if (cmd === "get_discord_status") {
        return Promise.resolve("connected");
      }
      return Promise.resolve(null);
    });

    scrobblerStore.setDiscordEnabled(true);
    expect(scrobblerStore.discordEnabled).toBe(true);

    await scrobblerStore.checkDiscordStatus();
    expect(scrobblerStore.discordStatus).toBe("connected");

    scrobblerStore.setDiscordClientId("999888777");
    scrobblerStore.resetDiscordClientId();
    expect(scrobblerStore.discordClientId).toBe("1548913001715990610");
  });
});
