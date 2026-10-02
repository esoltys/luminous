import { describe, it, expect, vi, beforeEach } from "vitest";
import { musicbrainzStore } from "./musicbrainz.svelte";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { openExternalUrl } from "../utils/openExternalUrl";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
}));

vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(() => Promise.resolve(() => {})),
}));

vi.mock("../utils/openExternalUrl", () => ({
  openExternalUrl: vi.fn(() => Promise.resolve()),
}));

describe("musicbrainzStore", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    musicbrainzStore.isLoggedIn = false;
    musicbrainzStore.username = null;
    musicbrainzStore.email = null;
    musicbrainzStore.stats = null;
    musicbrainzStore.isAuthorizing = false;
    musicbrainzStore.isLoading = false;
    musicbrainzStore.authError = null;
    // reset private initialized flag if possible
    (musicbrainzStore as any).initialized = false;
  });

  it("initializes auth state from backend", async () => {
    vi.mocked(invoke).mockImplementation((cmd: string) => {
      if (cmd === "get_musicbrainz_auth_state") {
        return Promise.resolve({
          is_logged_in: true,
          username: "musiclover",
          email: "music@example.com",
        });
      }
      if (cmd === "get_musicbrainz_user_stats") {
        return Promise.resolve({
          username: "musiclover",
          collections_count: 3,
          releases_count: 42,
          cached_at: 1728000000,
        });
      }
      return Promise.resolve(null);
    });

    await musicbrainzStore.init();

    expect(musicbrainzStore.isLoggedIn).toBe(true);
    expect(musicbrainzStore.username).toBe("musiclover");
    expect(musicbrainzStore.email).toBe("music@example.com");
    expect(listen).toHaveBeenCalledWith("musicbrainz-auth-changed", expect.any(Function));
  });

  it("starts login and opens external browser URL", async () => {
    vi.mocked(invoke).mockImplementation((cmd: string) => {
      if (cmd === "start_musicbrainz_login") {
        return Promise.resolve("https://musicbrainz.org/oauth2/authorize?client_id=123");
      }
      return Promise.resolve(null);
    });

    const url = await musicbrainzStore.startLogin(true);

    expect(invoke).toHaveBeenCalledWith("start_musicbrainz_login", {
      preferLoopback: true,
    });
    expect(openExternalUrl).toHaveBeenCalledWith("https://musicbrainz.org/oauth2/authorize?client_id=123");
    expect(url).toBe("https://musicbrainz.org/oauth2/authorize?client_id=123");
    expect(musicbrainzStore.isAuthorizing).toBe(true);
  });

  it("submits manual auth code and sets state", async () => {
    vi.mocked(invoke).mockImplementation((cmd: string) => {
      if (cmd === "submit_musicbrainz_auth_code") {
        return Promise.resolve({
          is_logged_in: true,
          username: "tester",
          email: null,
        });
      }
      if (cmd === "get_musicbrainz_user_stats") {
        return Promise.resolve({
          username: "tester",
          collections_count: 1,
          releases_count: 5,
          cached_at: 1728000000,
        });
      }
      return Promise.resolve(null);
    });

    const res = await musicbrainzStore.submitAuthCode("auth_code_xyz");

    expect(invoke).toHaveBeenCalledWith("submit_musicbrainz_auth_code", {
      code: "auth_code_xyz",
    });
    expect(res.is_logged_in).toBe(true);
    expect(musicbrainzStore.isLoggedIn).toBe(true);
    expect(musicbrainzStore.username).toBe("tester");
    expect(musicbrainzStore.isLoading).toBe(false);
  });

  it("logs out and clears user state and stats", async () => {
    musicbrainzStore.isLoggedIn = true;
    musicbrainzStore.username = "tester";
    musicbrainzStore.stats = {
      username: "tester",
      collections_count: 1,
      releases_count: 5,
      cached_at: 1728000000,
    };

    vi.mocked(invoke).mockResolvedValue(null);

    await musicbrainzStore.logout();

    expect(invoke).toHaveBeenCalledWith("logout_musicbrainz");
    expect(musicbrainzStore.isLoggedIn).toBe(false);
    expect(musicbrainzStore.username).toBe(null);
    expect(musicbrainzStore.stats).toBe(null);
  });

  it("cancels in-flight login", async () => {
    musicbrainzStore.isAuthorizing = true;
    vi.mocked(invoke).mockResolvedValue(null);

    await musicbrainzStore.cancelLogin();

    expect(invoke).toHaveBeenCalledWith("cancel_musicbrainz_login");
    expect(musicbrainzStore.isAuthorizing).toBe(false);
  });
});
