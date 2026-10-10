import { describe, expect, it } from "bun:test";
import { Database } from "bun:sqlite";
import { existsSync } from "node:fs";
import path from "node:path";
import {
  AppProfile,
  CDP_PORT,
  DEFAULT_APP_STATE,
  FIRST_RUN_DONE,
  NO_DEFAULT_LIBRARY,
  canonicalView,
  type SongRecord,
} from "./throwaway-profile";

describe("throwaway-profile", () => {
  it("exports standard constants", () => {
    expect(CDP_PORT).toBe(9222);
    expect(FIRST_RUN_DONE.welcome_seen).toBe("true");
    expect(FIRST_RUN_DONE.walkthrough_completed).toBe("true");
    expect(NO_DEFAULT_LIBRARY.default_library_path).toBe("");
    expect(DEFAULT_APP_STATE.welcome_seen).toBe("true");
    expect(DEFAULT_APP_STATE.active_tab).toBe("collection");
    expect(DEFAULT_APP_STATE.active_sub_tab).toBe("songs");
    expect(canonicalView("songs")).toEqual({
      active_tab: "collection",
      active_sub_tab: "songs",
    });
    expect(canonicalView("albums")).toEqual({
      active_tab: "collection",
      active_sub_tab: "albums",
    });
  });

  it("creates isolated directory hierarchy in temporary storage", async () => {
    const profile = new AppProfile();
    try {
      expect(existsSync(profile.root)).toBe(true);
      expect(existsSync(profile.dataDir)).toBe(true);
      expect(existsSync(profile.webviewDir)).toBe(true);
      expect(profile.dataDir.startsWith(profile.root)).toBe(true);
      expect(profile.webviewDir.startsWith(profile.root)).toBe(true);
      expect(profile.dbPath).toBe(path.join(profile.dataDir, "luminous.db"));
    } finally {
      await profile.dispose();
      expect(existsSync(profile.root)).toBe(false);
    }
  });

  it("pulls defaults downward and allows caller overrides in app_state", async () => {
    // 1. Fresh profile automatically receives downward defaults
    const defaultProfile = new AppProfile();
    try {
      const db = new Database(defaultProfile.dbPath, { readonly: true });
      const welcome = db.query("SELECT value FROM app_state WHERE key = 'welcome_seen'").get() as { value: string };
      const subTab = db.query("SELECT value FROM app_state WHERE key = 'active_sub_tab'").get() as { value: string };
      db.close();
      expect(welcome.value).toBe("true");
      expect(subTab.value).toBe("songs");
    } finally {
      await defaultProfile.dispose();
    }

    // 2. Caller-specified appState overrides default values
    const customProfile = new AppProfile({
      appState: {
        active_sub_tab: "albums",
        custom_flag: "enabled",
      },
    });
    try {
      const db = new Database(customProfile.dbPath, { readonly: true });
      const welcome = db.query("SELECT value FROM app_state WHERE key = 'welcome_seen'").get() as { value: string };
      const subTab = db.query("SELECT value FROM app_state WHERE key = 'active_sub_tab'").get() as { value: string };
      const custom = db.query("SELECT value FROM app_state WHERE key = 'custom_flag'").get() as { value: string };
      db.close();
      expect(welcome.value).toBe("true"); // retained from DEFAULT_APP_STATE
      expect(subTab.value).toBe("albums"); // overridden by caller
      expect(custom.value).toBe("enabled"); // added by caller

      // Update and delete offline
      customProfile.writeAppState({
        custom_flag: "updated",
        second_key: "second_value",
      });

      const db2 = new Database(customProfile.dbPath, { readonly: true });
      const customUpdated = db2.query("SELECT value FROM app_state WHERE key = 'custom_flag'").get() as { value: string };
      db2.close();
      expect(customUpdated.value).toBe("updated");

      customProfile.writeAppState({ second_key: null });
      const db3 = new Database(customProfile.dbPath, { readonly: true });
      const secondDeleted = db3.query("SELECT value FROM app_state WHERE key = 'second_key'").get();
      db3.close();
      expect(secondDeleted).toBeNull();
    } finally {
      await customProfile.dispose();
    }
  });

  it("finds playable songs and cues playback state in the profile DB", async () => {
    const profile = new AppProfile();
    try {
      // Seed songs table in profile database
      const db = new Database(profile.dbPath);
      db.exec(`
        CREATE TABLE IF NOT EXISTS songs (
          id INTEGER PRIMARY KEY,
          title TEXT,
          album TEXT,
          filetype INTEGER,
          samplerate INTEGER,
          bitdepth INTEGER,
          bitrate INTEGER,
          length_nanosec INTEGER,
          unavailable INTEGER DEFAULT 0,
          cue_path TEXT
        );
        INSERT INTO songs (id, title, album, filetype, samplerate, bitdepth, bitrate, length_nanosec)
        VALUES (42, 'Solar Eclipse', 'Luminous Album', 2, 44100, 16, 1411, 180000000000);
      `);
      db.close();

      const playableSong = profile.findPlayableSong("title = ?1", "Solar Eclipse");
      expect(playableSong).not.toBeNull();
      expect(playableSong?.id).toBe(42);
      expect(playableSong?.title).toBe("Solar Eclipse");
      expect(playableSong?.album).toBe("Luminous Album");

      // Verify findSong alias works identically
      const songAlias = profile.findSong("title = ?1", "Solar Eclipse");
      expect(songAlias?.id).toBe(playableSong?.id);

      profile.cue(playableSong as SongRecord);

      const dbVerify = new Database(profile.dbPath, { readonly: true });
      const lastSongId = dbVerify.query("SELECT value FROM app_state WHERE key = 'last_song_id'").get() as { value: string };
      const lastPos = dbVerify.query("SELECT value FROM app_state WHERE key = 'last_position_nanosec'").get() as { value: string };
      const lastPlaylist = dbVerify.query("SELECT value FROM app_state WHERE key = 'last_playlist_id'").get() as { value: string };
      dbVerify.close();

      expect(lastSongId.value).toBe("42");
      expect(lastPos.value).toBe("0");
      expect(lastPlaylist.value).toBe("0");
    } finally {
      await profile.dispose();
    }
  });

  it("idempotently disposes and respects keepProfiles flag", async () => {
    const profile = new AppProfile();
    const root = profile.root;
    expect(existsSync(root)).toBe(true);

    // Dispose with keep=true
    await profile.dispose(true);
    expect(existsSync(root)).toBe(true);

    // Second dispose should be a no-op
    await profile.dispose(true);
    expect(existsSync(root)).toBe(true);

    // Clean up manually for test cleanliness
    const { rmSync } = await import("node:fs");
    rmSync(root, { recursive: true, force: true });
  });

  describe("cloneFrom", () => {
    /** One query on a fresh read-only handle, closed straight away so Windows can delete the folder. */
    const rows = <T>(dbPath: string, sql: string): T[] => {
      const db = new Database(dbPath, { readonly: true });
      try {
        return db.query(sql).all() as T[];
      } finally {
        db.close();
      }
    };
    const keysOf = (dbPath: string) => rows<{ key: string }>(dbPath, "SELECT key FROM app_state").map((r) => r.key);

    it("copies stats and settings, drops outward integrations, never writes the source", async () => {
      const source = new AppProfile();
      source.writeAppState({
        scrobbler_lastfm_session: "secret",
        listenbrainz_token: "secret",
        discord_enabled: "true",
        theme_mode: "dark",
      });
      const db = new Database(source.dbPath);
      db.exec("CREATE TABLE pins (id INTEGER PRIMARY KEY, name TEXT)");
      db.exec("INSERT INTO pins (name) VALUES ('My pinned album')");
      db.close();
      const sourceKeysBefore = keysOf(source.dbPath);

      const clone = new AppProfile({ cloneFrom: source.dbPath });
      try {
        expect(rows(clone.dbPath, "SELECT name FROM pins")).toEqual([{ name: "My pinned album" }]);
        const cloneKeys = keysOf(clone.dbPath);
        expect(cloneKeys).toContain("theme_mode");
        expect(cloneKeys.filter((k) => /^(scrobbler_|listenbrainz_|discord_)/.test(k))).toEqual([]);

        // The source keeps its integrations and gains nothing.
        expect(keysOf(source.dbPath)).toEqual(sourceKeysBefore);
        expect(sourceKeysBefore).toContain("scrobbler_lastfm_session");
      } finally {
        await clone.dispose();
        await source.dispose();
      }
    });

    it("pins an unchosen default library to none so no sidecar can be written, but keeps a chosen one", async () => {
      const unchosen = new AppProfile();
      unchosen.writeAppState({ default_library_path: null });
      const chosen = new AppProfile();
      chosen.writeAppState({ default_library_path: "Z:/Music" });

      const a = new AppProfile({ cloneFrom: unchosen.dbPath });
      const b = new AppProfile({ cloneFrom: chosen.dbPath });
      try {
        const value = (p: AppProfile) =>
          rows<{ value: string }>(p.dbPath, "SELECT value FROM app_state WHERE key = 'default_library_path'")[0]?.value;
        expect(value(a)).toBe("");
        expect(value(b)).toBe("Z:/Music");
      } finally {
        for (const p of [a, b, unchosen, chosen]) await p.dispose();
      }
    });

    it("refuses a missing source", () => {
      expect(() => new AppProfile({ cloneFrom: path.join("nope", "luminous.db") })).toThrow(/No database to clone/);
    });
  });
});
