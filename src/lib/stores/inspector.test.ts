import { describe, it, expect, beforeEach, afterEach, vi } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import { flushSync } from "svelte";
import type { Song } from "../types";

vi.mock("@tauri-apps/api/window", () => ({
  getCurrentWindow: vi.fn(() => ({
    onResized: vi.fn(() => Promise.resolve(() => {})),
    onMoved: vi.fn(() => Promise.resolve(() => {})),
  })),
}));

import { inspectorStore } from "./inspector.svelte";
import { navigationStore } from "./navigation.svelte";
import { playerStore } from "./player.svelte";

const song = (id: number, extra: Partial<Song> = {}) => ({ id, title: `Song ${id}`, ...extra }) as Song;

async function settle() {
  flushSync();
  await vi.advanceTimersByTimeAsync(300);
  flushSync();
}

describe("inspectorStore", () => {
  beforeEach(() => {
    vi.useFakeTimers();
    vi.mocked(invoke).mockImplementation(async (cmd: string, args?: unknown) => {
      const a = args as { album?: string; artist?: string } | undefined;
      if (cmd === "get_songs_by_album") return [song(100, { album: a?.album })];
      if (cmd === "get_songs_by_artist") return [song(200, { artist: a?.artist })];
      return null;
    });
    inspectorStore.clearAll();
    playerStore.currentSong = undefined;
    navigationStore.activeTab = "collection";
    navigationStore.selectedAlbumName = null;
    navigationStore.selectedArtistName = null;
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it("is null when nothing is playing, selected or viewed", () => {
    expect(inspectorStore.subject).toBeNull();
  });

  it("falls back to the playing song", () => {
    playerStore.currentSong = song(1);
    expect(inspectorStore.subject).toMatchObject({ kind: "song", source: "playing", song: { id: 1 } });
  });

  it("a selected song beats the playing song", () => {
    playerStore.currentSong = song(1);
    inspectorStore.setSelection("album-view", song(2));
    expect(inspectorStore.subject).toMatchObject({ kind: "song", source: "selection", song: { id: 2 } });
  });

  it("clearing the selection returns to the playing song; clearing an unknown view is a no-op", () => {
    playerStore.currentSong = song(1);
    inspectorStore.setSelection("v", song(2));
    inspectorStore.setSelection("unknown", null);
    expect(inspectorStore.subject?.song?.id).toBe(2);
    inspectorStore.setSelection("v", null);
    expect(inspectorStore.subject).toMatchObject({ source: "playing", song: { id: 1 } });
  });

  it("views cannot clobber each other when one clears", () => {
    inspectorStore.setSelection("a", song(2));
    inspectorStore.setSelection("b", song(3));
    inspectorStore.setSelection("a", null);
    expect(inspectorStore.subject?.song?.id).toBe(3);
  });

  it("describes the viewed album via a representative song, beating now playing", async () => {
    playerStore.currentSong = song(1);
    navigationStore.selectedAlbumName = "Dark Side";
    await settle();
    expect(inspectorStore.subject).toMatchObject({ kind: "album", source: "view", key: "album:Dark Side", song: { id: 100 } });
  });

  it("describes the viewed artist when no album is open", async () => {
    navigationStore.selectedArtistName = "Pink Floyd";
    await settle();
    expect(inspectorStore.subject).toMatchObject({ kind: "artist", source: "view", key: "artist:Pink Floyd", song: { id: 200 } });
  });

  it("prefers a song with MusicBrainz IDs as the representative of an artist or album", async () => {
    vi.mocked(invoke).mockImplementation(async (cmd: string) => {
      if (cmd === "get_songs_by_artist") return [song(1), song(2, { musicbrainz_artist_id: "artist-mbid" })];
      if (cmd === "get_songs_by_album") return [song(3), song(4, { musicbrainz_release_group_id: "rg-mbid" })];
      return null;
    });
    navigationStore.selectedArtistName = "Pink Floyd";
    await settle();
    expect(inspectorStore.subject?.song.id).toBe(2);
    navigationStore.selectedAlbumName = "Dark Side";
    await settle();
    expect(inspectorStore.subject?.song.id).toBe(4);
  });

  it("a selected song beats the viewed entity", async () => {
    navigationStore.selectedAlbumName = "Dark Side";
    await settle();
    inspectorStore.setSelection("album-view", song(5));
    expect(inspectorStore.subject).toMatchObject({ kind: "song", source: "selection", song: { id: 5 } });
  });

  it("ignores a viewed entity while another tab is active", async () => {
    playerStore.currentSong = song(1);
    navigationStore.selectedAlbumName = "Dark Side";
    navigationStore.activeTab = "settings";
    await settle();
    expect(inspectorStore.subject?.source).toBe("playing");
  });

  it("an album with no songs falls back to now playing", async () => {
    vi.mocked(invoke).mockImplementation(async () => []);
    playerStore.currentSong = song(1);
    navigationStore.selectedAlbumName = "Empty";
    await settle();
    expect(inspectorStore.subject?.source).toBe("playing");
  });

  it("does not let a slow earlier album resolve overwrite a newer one", async () => {
    let releaseFirst: (v: Song[]) => void = () => {};
    vi.mocked(invoke).mockImplementation(async (_cmd: string, args?: unknown) => {
      const album = (args as { album: string }).album;
      if (album === "Slow") return new Promise<Song[]>((r) => (releaseFirst = r));
      return [song(300, { album })];
    });
    navigationStore.selectedAlbumName = "Slow";
    flushSync();
    await vi.advanceTimersByTimeAsync(300);
    navigationStore.selectedAlbumName = "Fast";
    await settle();
    releaseFirst([song(999)]);
    await settle();
    expect(inspectorStore.subject).toMatchObject({ key: "album:Fast", song: { id: 300 } });
  });
});
