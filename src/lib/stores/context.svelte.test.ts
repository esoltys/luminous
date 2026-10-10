import { describe, it, expect, beforeEach, afterEach, vi } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import { flushSync } from "svelte";
import type { Song, SongContextEnrichment } from "../types";

vi.mock("@tauri-apps/api/window", () => ({
  getCurrentWindow: vi.fn(() => ({
    onResized: vi.fn(() => Promise.resolve(() => {})),
    onMoved: vi.fn(() => Promise.resolve(() => {})),
  })),
}));

import { contextStore, entitySubject, type ContextSubject } from "./context.svelte";
import { collectionStore } from "./collection.svelte";
import { prefs } from "./prefs.svelte";

const MBID = "5b11f4ce-a62d-471e-81fc-a69a8278c7da";

const song = (id: number, extra: Partial<Song> = {}) =>
  ({ id, title: `Song ${id}`, artist: "Pink Floyd", album: "Dark Side", ...extra }) as Song;

const ctx = (extra: Partial<SongContextEnrichment> = {}) =>
  ({ mb_tags: [], critiquebrainz_review_links: [], ...extra }) as SongContextEnrichment;

const artist = (s: Song = song(1)): ContextSubject => ({ kind: "artist", key: "artist:Pink Floyd", name: "Pink Floyd", song: s });
const album = (s: Song = song(1)): ContextSubject => ({ kind: "album", key: "album:Dark Side", name: "Dark Side", song: s });

let cleanups: (() => void)[] = [];

/** Creates a view the way a component would, inside an effect root, and lets its debounced fetch run. */
async function mount(getSubject: () => ContextSubject | null, options?: Parameters<typeof contextStore.for>[1]) {
  let view!: ReturnType<typeof contextStore.for>;
  cleanups.push(
    $effect.root(() => {
      view = contextStore.for(getSubject, options);
    })
  );
  flushSync();
  await vi.advanceTimersByTimeAsync(300);
  flushSync();
  return view;
}

const calls = (cmd: string) => vi.mocked(invoke).mock.calls.filter(([c]) => c === cmd);

describe("contextStore", () => {
  let context: SongContextEnrichment;

  beforeEach(() => {
    vi.useFakeTimers();
    context = ctx();
    vi.mocked(invoke).mockReset();
    vi.mocked(invoke).mockImplementation(async (cmd: string) => {
      if (cmd === "get_song_context") return context;
      if (cmd === "get_artist_events") return [];
      return null;
    });
    contextStore.clearAll();
    collectionStore.artistProfiles = {};
    collectionStore.albumProfiles = {};
    prefs.onlineEnabled = true;
  });

  afterEach(() => {
    cleanups.forEach((c) => c());
    cleanups = [];
    vi.useRealTimers();
  });

  it("orders artist sections facts, bio, events, links, external", async () => {
    context = ctx({ artist_begin_date: "1965", wikipedia_extract: "Wiki bio" });
    collectionStore.artistProfiles = {
      "pink floyd": { artist_key: "Pink Floyd", website: "https://pinkfloyd.com", social_links: [], tags: [], musicbrainz_artist_id: MBID },
    } as never;
    const view = await mount(() => artist(song(1, { musicbrainz_artist_id: MBID })));
    expect(view.sections.map((s) => s.id)).toEqual(["facts", "bio", "events", "links", "external"]);
  });

  it("leaves empty sections out", async () => {
    const view = await mount(() => album());
    expect(view.sections).toEqual([]);
  });

  it("prefers the user's own artist bio over the Wikipedia extract", async () => {
    context = ctx({ wikipedia_extract: "Wiki bio" });
    collectionStore.artistProfiles = { "pink floyd": { artist_key: "Pink Floyd", bio: "My bio", social_links: [], tags: [] } } as never;
    const view = await mount(() => artist());
    expect(view.sections.find((s) => s.id === "bio")).toMatchObject({ text: "My bio", source: "profile" });
  });

  it("falls back to the Wikipedia extract for an artist, but never uses it for an album", async () => {
    context = ctx({ wikipedia_extract: "Wiki bio" });
    const artistView = await mount(() => artist());
    expect(artistView.sections.find((s) => s.id === "bio")).toMatchObject({ source: "wikipedia" });
    const albumView = await mount(() => album());
    expect(albumView.sections.find((s) => s.id === "bio")).toBeUndefined();
  });

  it("shows the album description, not artist facts", async () => {
    context = ctx({ artist_begin_date: "1965" });
    collectionStore.albumProfiles = { "dark side": { album_key: "dark side", description: "Liner notes", links: [] } } as never;
    const view = await mount(() => album());
    expect(view.sections.map((s) => s.id)).toEqual(["bio"]);
  });

  it("keeps the MusicBrainz link and drops the derived ListenBrainz link for an artist", async () => {
    collectionStore.artistProfiles = {
      "pink floyd": { artist_key: "Pink Floyd", social_links: [], tags: [], musicbrainz_artist_id: MBID },
    } as never;
    const view = await mount(() => artist());
    const links = view.sections.find((s) => s.id === "links");
    expect(links && links.id === "links" && links.items.map((i) => i.platform)).toEqual(["fanart_tv", "musicbrainz"]);
  });

  it("filters blacklisted links out of an album's links and leads with the official site", async () => {
    collectionStore.albumProfiles = {
      "dark side": {
        album_key: "dark side",
        website: "https://thebeatles.com",
        links: [
          { platform: "other_databases", handle_or_url: "https://rateyourmusic.com/release/album/x/y/" },
          { platform: "x", handle_or_url: "https://x.com/thebeatles" },
          { platform: "allmusic", handle_or_url: "https://www.allmusic.com/album/mw0000192938" },
        ],
      },
    } as never;
    const view = await mount(() => album());
    const links = view.sections.find((s) => s.id === "links");
    expect(links && links.id === "links" && links.items.map((i) => i.label)).toEqual(["thebeatles.com", "AllMusic"]);
  });

  it("hides CritiqueBrainz for an artist but exposes the rating for an album", async () => {
    context = ctx({ critiquebrainz_rating: 4.2, critiquebrainz_review_count: 7 });
    const withRg = song(1, { musicbrainz_release_group_id: MBID });
    const artistView = await mount(() => artist(withRg));
    const external = artistView.sections.find((s) => s.id === "external");
    expect(external && external.id === "external" && external.critiquebrainz).toBeNull();
    expect(artistView.communityRating).toMatchObject({ rating: 4.2, source: "critiquebrainz" });
    const albumView = await mount(() => album(withRg));
    const albumExternal = albumView.sections.find((s) => s.id === "external");
    expect(albumExternal && albumExternal.id === "external" && albumExternal.critiquebrainz).toMatchObject({ rating: 4.2 });
  });

  it("falls back to the MusicBrainz rating", async () => {
    context = ctx({ mb_rating: 3.75, mb_rating_votes: 4 });
    const view = await mount(() => album());
    expect(view.communityRating).toEqual({ rating: 3.75, count: 4, source: "musicbrainz" });
  });

  it("requests a subject once however many views read it", async () => {
    await mount(() => artist());
    await mount(() => artist());
    expect(calls("get_song_context")).toHaveLength(1);
    expect(calls("get_artist_events")).toHaveLength(1);
  });

  it("fetches events for a song only when asked", async () => {
    const songSubject: ContextSubject = { kind: "song", key: "song:1", name: "Pink Floyd", song: song(1) };
    await mount(() => songSubject, { songEvents: () => false });
    expect(calls("get_artist_events")).toHaveLength(0);
    await mount(() => songSubject, { songEvents: () => true });
    expect(calls("get_artist_events")).toHaveLength(1);
  });

  it("refresh bypasses the backend cache for both context and events", async () => {
    const view = await mount(() => artist());
    await view.refresh();
    expect(calls("get_song_context").at(-1)?.[1]).toMatchObject({ forceRefresh: true });
    expect(calls("get_artist_events").at(-1)?.[1]).toMatchObject({ forceRefresh: true });
  });

  it("fetches nothing and reports offline when online features are off", async () => {
    prefs.onlineEnabled = false;
    const view = await mount(() => artist());
    expect(calls("get_song_context")).toHaveLength(0);
    expect(view.offline).toBe(true);
  });

  it("reports a failed fetch and recovers on refresh", async () => {
    vi.mocked(invoke).mockImplementation(async (cmd: string) => {
      if (cmd === "get_song_context") throw new Error("boom");
      return [];
    });
    const view = await mount(() => album());
    expect(view.error).toBe("boom");
    vi.mocked(invoke).mockImplementation(async () => ctx({ mb_rating: 3 }));
    await view.refresh();
    expect(view.error).toBe("");
    expect(view.communityRating).not.toBeNull();
  });

  it("never shows one subject's reply under another", async () => {
    let current = $state(artist(song(1)));
    vi.mocked(invoke).mockImplementation(async (cmd: string, args?: unknown) => {
      if (cmd === "get_song_context") return ctx({ mb_rating: (args as { songId: number }).songId });
      return [];
    });
    const view = await mount(() => current);
    expect(view.communityRating?.rating).toBe(1);
    current = { ...artist(song(2)), key: "artist:Other", name: "Other" };
    flushSync();
    expect(view.communityRating).toBeNull();
    await vi.advanceTimersByTimeAsync(300);
    flushSync();
    expect(view.communityRating?.rating).toBe(2);
  });
});

describe("entitySubject", () => {
  it("is null for an entity with no songs", () => {
    expect(entitySubject("artist", "Nobody", [])).toBeNull();
  });

  it("prefers a song that carries the identifying MusicBrainz ID", () => {
    const plain = song(1);
    const tagged = song(2, { musicbrainz_album_artist_id: "x" });
    expect(entitySubject("artist", "Pink Floyd", [plain, tagged])?.song.id).toBe(2);
    expect(entitySubject("album", "Dark Side", [plain, song(3, { musicbrainz_release_group_id: "rg" })])?.song.id).toBe(3);
    expect(entitySubject("album", "Dark Side", [plain, tagged])?.song.id).toBe(1);
  });
});
