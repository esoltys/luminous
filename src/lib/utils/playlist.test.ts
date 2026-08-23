import { describe, it, expect } from "vitest";
import { getPlaylistDisplayName, getPopulationModeSuffix } from "./playlist";
import type { Playlist } from "../types";

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

  it("returns base name for dynamic playlist in 'all' mode", () => {
    const playlist: Playlist = {
      id: 2,
      name: "Rock",
      dynamic_spec: "Rock",
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
      dynamic_spec: "1980s Rock",
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
      dynamic_spec: "Jazz",
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

  it("shows only the last segment of a multi-component genre auto-playlist name", () => {
    const subgenrePlaylist: Playlist = {
      id: 5,
      name: "Metal; Death Metal",
      dynamic_spec: "Metal; Death Metal",
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
      name: "Pop; Indie Pop",
      dynamic_spec: "Pop; Indie Pop",
      population_mode: "all",
      created: 100,
      updated: 100,
      track_count: 8,
      dynamic_enabled: true,
      is_queue: false,
    };
    expect(getPlaylistDisplayName(otherSubgenrePlaylist)).toBe("Indie Pop");
  });

  it("appends the mode suffix to the stripped sub-genre label", () => {
    const playlist: Playlist = {
      id: 7,
      name: "Metal; Death Metal",
      dynamic_spec: "Metal; Death Metal",
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
      dynamic_spec: "Jazz",
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
});
