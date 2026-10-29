import "@testing-library/jest-dom";
import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, fireEvent, waitFor } from "@testing-library/svelte";
import AutoPlaylistDetailView from "./AutoPlaylistDetailView.svelte";
import { playlistsStore } from "../stores/playlists.svelte";
import { collectionStore } from "../stores/collection.svelte";
import { invoke } from "@tauri-apps/api/core";
import type { Playlist, PlaylistItem, PlaylistSizeFit, Song } from "../types";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn().mockResolvedValue(() => {}) }));

function song(id: number, title: string): Song {
  return {
    id,
    source: "local_file",
    filetype: "MP3",
    path: `/music/${id}.mp3`,
    title,
    artist: "Artist",
    album: "Album",
    album_artist: "Artist",
    genre: "Synthwave",
    track: id,
    disc: 1,
    year: 2020,
    compilation: false,
    length_nanosec: 180_000_000_000,
    beginning_nanosec: 0,
    end_nanosec: 180_000_000_000,
    rating: 0,
    playcount: 0,
    skipcount: 0,
    art_embedded: false,
    art_unset: false,
    unavailable: false,
  };
}

const songs = [song(1, "Track One"), song(2, "Track Two"), song(3, "Track Three")];
const items: PlaylistItem[] = songs.map((s, i) => ({
  id: 100 + i,
  playlist_id: 5,
  position: i,
  item_type: "song",
  uuid: `uuid-${s.id}`,
  song: s,
}));
const genrePlaylist: Playlist = {
  id: 5,
  name: "Synthwave",
  track_count: 3,
  created: 1_700_000_000,
  updated: 1_700_000_000,
  dynamic_enabled: true,
  dynamic_spec: "tag:Synthwave",
  is_queue: false,
  size_limit_bytes: 16_000_000_000,
};
// Track One and Two fit; Track Three starts below the line.
const fit: PlaylistSizeFit = {
  limit_bytes: 16_000_000_000,
  break_index: 2,
  fit_songs: 2,
  fit_bytes: 12_400_000_000,
  total_bytes: 20_000_000_000,
  order: items.map((i) => i.uuid),
  device: null,
};

let fitAnswer: PlaylistSizeFit | null;

describe("AutoPlaylistDetailView size limit and sort (#344)", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    collectionStore.searchQuery = "";
    collectionStore.directories = [];
    playlistsStore.playlists = [genrePlaylist];
    fitAnswer = fit;
    vi.mocked(invoke).mockImplementation(async (cmd: string) => {
      if (cmd === "get_playlist_tracks") return items;
      if (cmd === "get_playlist_size_fit") return fitAnswer;
      if (cmd === "get_favourite_songs") return songs;
      return [];
    });
  });

  const genreView = { kind: "genre" as const, genre: "Synthwave", playlistId: 5 };

  it("draws the backing playlist's break above the first song that doesn't fit", async () => {
    const { findByRole, getByText, getByRole } = render(AutoPlaylistDetailView, { view: genreView });
    const brk = await findByRole("separator", { name: "16 GB limit · 2 songs above fit · 7.6 GB below" });
    expect(brk.nextElementSibling).toHaveTextContent("Track Three");
    expect(getByText("12.4 GB of 16 GB used")).toBeInTheDocument();
    expect(getByRole("button", { name: /Limit 16 GB/ })).toBeInTheDocument();
  });

  it("hides the break when the fit was computed for a different order", async () => {
    fitAnswer = { ...fit, order: ["uuid-2", "uuid-1", "uuid-3"] };
    const { findByText, queryByRole } = render(AutoPlaylistDetailView, { view: genreView });
    await findByText("Track Three");
    expect(queryByRole("separator", { name: /16 GB limit/ })).toBeNull();
  });

  it("persists a sort on the backing playlist instead of sorting locally", async () => {
    const sortSpy = vi.spyOn(playlistsStore, "setSort").mockResolvedValue();
    const { findByText, getByRole } = render(AutoPlaylistDetailView, { view: genreView });
    await findByText("Track Three");
    await fireEvent.click(getByRole("button", { name: /^Title/ }));
    expect(sortSpy).toHaveBeenCalledWith(5, "title", true);
  });

  it("sorts Favourites locally and offers no size limit there", async () => {
    playlistsStore.playlists = [];
    const sortSpy = vi.spyOn(playlistsStore, "setSort").mockResolvedValue();
    const { findByText, getByRole, queryByRole, getAllByText } = render(AutoPlaylistDetailView, {
      view: { kind: "favourites" as const },
    });
    await findByText("Track Three");
    expect(queryByRole("button", { name: "Size limit" })).toBeNull();

    await fireEvent.click(getByRole("button", { name: /^Title/ }));
    await fireEvent.click(getByRole("button", { name: /^Title/ }));
    expect(sortSpy).not.toHaveBeenCalled();
    await waitFor(() =>
      expect(getAllByText(/^Track (One|Two|Three)$/).map((e) => e.textContent)).toEqual([
        "Track Two",
        "Track Three",
        "Track One",
      ])
    );
  });
});
