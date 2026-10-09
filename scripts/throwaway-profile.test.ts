import { describe, expect, it } from "bun:test";
import { Database } from "bun:sqlite";
import { existsSync } from "node:fs";
import path from "node:path";
import {
  AppProfile,
  CDP_PORT,
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

  it("handles offline app_state manipulation and pre-seeding", async () => {
    const profile = new AppProfile({
      appState: { initial_key: "initial_value" },
    });
    try {
      // Check pre-seeded key
      const db1 = new Database(profile.dbPath, { readonly: true });
      const row1 = db1.query("SELECT value FROM app_state WHERE key = 'initial_key'").get() as { value: string };
      db1.close();
      expect(row1.value).toBe("initial_value");

      // Write additional keys and update existing
      profile.writeAppState({
        initial_key: "updated_value",
        second_key: "second_value",
      });

      const db2 = new Database(profile.dbPath, { readonly: true });
      const row2 = db2.query("SELECT value FROM app_state WHERE key = 'initial_key'").get() as { value: string };
      const row3 = db2.query("SELECT value FROM app_state WHERE key = 'second_key'").get() as { value: string };
      db2.close();
      expect(row2.value).toBe("updated_value");
      expect(row3.value).toBe("second_value");

      // Delete key with null
      profile.writeAppState({ second_key: null });

      const db3 = new Database(profile.dbPath, { readonly: true });
      const row4 = db3.query("SELECT value FROM app_state WHERE key = 'second_key'").get();
      db3.close();
      expect(row4).toBeNull();
    } finally {
      await profile.dispose();
    }
  });

  it("finds songs and cues playback state in the profile DB", async () => {
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

      const song = profile.findSong("title = ?1", "Solar Eclipse");
      expect(song).not.toBeNull();
      expect(song?.id).toBe(42);
      expect(song?.title).toBe("Solar Eclipse");
      expect(song?.album).toBe("Luminous Album");

      profile.cue(song as SongRecord);

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
});
