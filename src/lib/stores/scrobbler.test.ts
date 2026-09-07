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
          last_attempt: Date.now(),
        });
      }
      return Promise.resolve(null);
    });

    await scrobblerStore.flushCache();
    expect(scrobblerStore.flushSuccessMessage).toContain("Submitted 3 pending listens");
    expect(scrobblerStore.pendingCount).toBe(0);
  });

  it("syncs favourites to listenbrainz and updates state", async () => {
    vi.mocked(invoke).mockImplementation((cmd: string) => {
      if (cmd === "sync_favourites_to_listenbrainz") {
        return Promise.resolve({
          total_favourites: 10,
          synced: 8,
          skipped_no_mbid: 2,
          failed: 0,
        });
      }
      return Promise.resolve(null);
    });

    const res = await scrobblerStore.syncFavourites();
    expect(res).not.toBeNull();
    expect(scrobblerStore.syncFavouritesResult?.synced).toBe(8);
    expect(scrobblerStore.syncFavouritesResult?.total_favourites).toBe(10);
    expect(scrobblerStore.syncFavouritesResult?.skipped_no_mbid).toBe(2);
    expect(scrobblerStore.syncFavouritesError).toBeNull();
  });
});
