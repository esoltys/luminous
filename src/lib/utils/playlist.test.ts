import { describe, it, expect } from "vitest";
import { autoContinueRunStarts, getDaypartMixLabel, getPlaylistDisplayName, getPopulationModeSuffix, isAutoContinueItem } from "./playlist";
import type { Playlist, PlaylistItem } from "../types";
import { i18n } from "../stores/i18n.svelte";

describe("playlist utils", () => {
  it("returns base name for non-dynamic playlists", () => {
    const playlist: Playlist = {
      id: 1,
      name: "My Favorites",
      created: 100,
      updated: 100,
      track_count: 5,
      dynamic_enabled: false,
      is_queue: false,
    };
    expect(getPlaylistDisplayName(playlist)).toBe("My Favorites");
  });

  it("returns base name for a curated genre auto-playlist (tag:) in 'all' mode", () => {
    const playlist: Playlist = {
      id: 2,
      name: "Rock",
      dynamic_spec: "tag:Rock",
      population_mode: "all",
      created: 100,
      updated: 100,
      track_count: 25,
      dynamic_enabled: true,
      is_queue: false,
    };
    expect(getPlaylistDisplayName(playlist)).toBe("Rock");
  });

  it("appends mode suffix for dynamic auto-playlist when mode is set", () => {
    const playlistDeepCuts: Playlist = {
      id: 3,
      name: "1980s Rock",
      dynamic_spec: "decade:1980s",
      population_mode: "deep_cuts",
      created: 100,
      updated: 100,
      track_count: 10,
      dynamic_enabled: true,
      is_queue: false,
    };
    expect(getPlaylistDisplayName(playlistDeepCuts)).toBe("1980s Rock Deep Cuts");

    const playlistFavourites: Playlist = {
      id: 4,
      name: "Jazz",
      dynamic_spec: "tag:Jazz",
      population_mode: "favourites",
      created: 100,
      updated: 100,
      track_count: 15,
      dynamic_enabled: true,
      is_queue: false,
    };
    expect(getPlaylistDisplayName(playlistFavourites)).toBe("Jazz Favourites");
  });

  it("returns mode suffix strings correctly", () => {
    expect(getPopulationModeSuffix("deep_cuts")).toBe("Deep Cuts");
    expect(getPopulationModeSuffix("favourites")).toBe("Favourites");
    expect(getPopulationModeSuffix("familiar")).toBe("Essentials");
    expect(getPopulationModeSuffix("discover")).toBe("Discover");
    expect(getPopulationModeSuffix("all")).toBe("");
  });

  // #548: genre auto-playlists are now keyed one row per curated tag —
  // `name` is already the plain curated tag name (a sub-genre chip like
  // "Death Metal" has its own row, entirely separate from its parent
  // card's "Metal" row), so there's no combined "Metal; Death Metal" value
  // left to strip a segment out of the way the old bare-genre convention
  // required.
  it("shows a curated sub-genre chip's own plain name unaffected", () => {
    const subgenrePlaylist: Playlist = {
      id: 5,
      name: "Death Metal",
      dynamic_spec: "tag:Death Metal",
      population_mode: "all",
      created: 100,
      updated: 100,
      track_count: 8,
      dynamic_enabled: true,
      is_queue: false,
    };
    expect(getPlaylistDisplayName(subgenrePlaylist)).toBe("Death Metal");

    const otherSubgenrePlaylist: Playlist = {
      id: 6,
      name: "Indie Pop",
      dynamic_spec: "tag:Indie Pop",
      population_mode: "all",
      created: 100,
      updated: 100,
      track_count: 8,
      dynamic_enabled: true,
      is_queue: false,
    };
    expect(getPlaylistDisplayName(otherSubgenrePlaylist)).toBe("Indie Pop");
  });

  it("appends the mode suffix to a curated sub-genre chip's name", () => {
    const playlist: Playlist = {
      id: 7,
      name: "Death Metal",
      dynamic_spec: "tag:Death Metal",
      population_mode: "favourites",
      created: 100,
      updated: 100,
      track_count: 8,
      dynamic_enabled: true,
      is_queue: false,
    };
    expect(getPlaylistDisplayName(playlist)).toBe("Death Metal Favourites");
  });

  it("leaves single-value genre auto-playlist names unaffected", () => {
    const playlist: Playlist = {
      id: 8,
      name: "Jazz",
      dynamic_spec: "tag:Jazz",
      population_mode: "all",
      created: 100,
      updated: 100,
      track_count: 8,
      dynamic_enabled: true,
      is_queue: false,
    };
    expect(getPlaylistDisplayName(playlist)).toBe("Jazz");
  });

  it("leaves a non-dynamic playlist name containing ';' unaffected", () => {
    const playlist: Playlist = {
      id: 9,
      name: "Rock; Indie Faves",
      created: 100,
      updated: 100,
      track_count: 8,
      dynamic_enabled: false,
      is_queue: false,
    };
    expect(getPlaylistDisplayName(playlist)).toBe("Rock; Indie Faves");
  });

  it("leaves a Smart Playlist name containing ';' unaffected", () => {
    const playlist: Playlist = {
      id: 10,
      name: "Metal; Death Metal Favourites (Smart)",
      dynamic_spec: "genre:metal rating:>=4",
      created: 100,
      updated: 100,
      track_count: 8,
      dynamic_enabled: true,
      is_queue: false,
    };
    expect(getPlaylistDisplayName(playlist)).toBe("Metal; Death Metal Favourites (Smart)");
  });

  it("never appends a population-mode suffix to the Missing Metadata auto-playlist (#367)", () => {
    const playlist: Playlist = {
      id: 11,
      name: "Missing Metadata",
      dynamic_spec: "missingmeta",
      population_mode: "favourites",
      created: 100,
      updated: 100,
      track_count: 3,
      dynamic_enabled: true,
      is_queue: false,
    };
    expect(getPlaylistDisplayName(playlist)).toBe("Missing Metadata");
  });
});

describe("Auto Continue rows (#1235)", () => {
  const item = (uuid: string, metadata?: string): PlaylistItem =>
    ({ id: 0, playlist_id: 1, position: 0, item_type: "song", uuid, additional_metadata: metadata });
  const AUTO = '{"autoContinue":true}';

  it("recognises only rows tagged autoContinue", () => {
    expect(isAutoContinueItem(item("a", AUTO))).toBe(true);
    expect(isAutoContinueItem(item("b"))).toBe(false);
    expect(isAutoContinueItem(item("c", '{"autoContinue":false}'))).toBe(false);
    expect(isAutoContinueItem(item("d", "not json"))).toBe(false);
  });

  it("marks the first row of each run of Auto Continue songs", () => {
    const rows = [item("u1"), item("a1", AUTO), item("a2", AUTO), item("u2"), item("a3", AUTO)];
    expect([...autoContinueRunStarts(rows)]).toEqual(["a1", "a3"]);
  });
});

describe("Moment Mix label", () => {
  it("localizes the bucket name from dynamic_spec, not the stored English name", () => {
    i18n.currentLocale = "it";
    try {
      expect(getDaypartMixLabel("daypart:afternoon:2026-10-05:Deep Cuts", "Afternoon Mix")).toBe("Mix del pomeriggio");
      expect(getDaypartMixLabel("daypart:latenight:2026-10-05:", "Late Night Mix")).toBe("Mix della notte");
    } finally {
      i18n.currentLocale = "en-CA";
    }
  });

  it("falls back to the stored name for an unknown bucket or a non-daypart spec", () => {
    expect(getDaypartMixLabel("daypart:brunch:2026-10-05:", "Brunch Mix")).toBe("Brunch Mix");
    expect(getDaypartMixLabel("tag:Jazz", "Jazz")).toBe("Jazz");
    expect(getDaypartMixLabel(null, "X")).toBe("X");
  });
});
