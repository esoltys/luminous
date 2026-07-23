import { describe, it, expect } from "vitest";
import { render } from "@testing-library/svelte";
import HomeRowList from "./HomeRowList.svelte";
import type { HomeItem, Song, AlbumItem, Playlist } from "../types";

function makeSong(overrides: Partial<Song> = {}): Song {
  return {
    id: 1,
    source: "local_file",
    filetype: "MP3",
    title: "Wildflowers",
    artist: "Tom Petty",
    album: "Wildflowers",
    art_embedded: false,
    art_unset: false,
    compilation: false,
    beginning_nanosec: 0,
    end_nanosec: 0,
    rating: -1,
    playcount: 12,
    skipcount: 0,
    length_nanosec: 195_000_000_000,
    added: 1_700_000_000,
    unavailable: false,
    ...overrides,
  };
}

function makeAlbum(overrides: Partial<AlbumItem> = {}): AlbumItem {
  return {
    artist: "Tom Petty",
    album: "Full Moon Fever",
    year: 1989,
    track_count: 12,
    disc_count: 1,
    art_embedded: false,
    art_automatic: null,
    art_manual: null,
    ...overrides,
  };
}

function makePlaylist(overrides: Partial<Playlist> = {}): Playlist {
  return {
    id: 1,
    name: "Road Trip",
    dynamic_enabled: false,
    created: 0,
    updated: 0,
    track_count: 5,
    ...overrides,
  };
}

describe("HomeRowList.svelte", () => {
  it("renders a rank numeral and track duration for song rows in the rank variant", () => {
    const items: HomeItem[] = [{ type: "song", song: makeSong() }];
    const { getByText } = render(HomeRowList, { props: { items, variant: "rank" } });

    expect(getByText("01")).toBeInTheDocument();
    expect(getByText("Wildflowers")).toBeInTheDocument();
    expect(getByText("Tom Petty")).toBeInTheDocument();
    expect(getByText("3:15")).toBeInTheDocument();
  });

  it("omits the rank numeral in the added variant", () => {
    const items: HomeItem[] = [{ type: "song", song: makeSong() }];
    const { queryByText } = render(HomeRowList, { props: { items, variant: "added" } });

    expect(queryByText("01")).not.toBeInTheDocument();
  });

  it("renders album rows using the album title and artist, without a per-song rating", () => {
    const items: HomeItem[] = [{ type: "album", album: makeAlbum() }];
    const { getByText, container } = render(HomeRowList, { props: { items, variant: "rank" } });

    expect(getByText("Full Moon Fever")).toBeInTheDocument();
    expect(getByText("Tom Petty")).toBeInTheDocument();
    // No song rating control (heart/star) for album-grouped rows.
    expect(container.querySelector('[aria-pressed]')).not.toBeInTheDocument();
    expect(container.querySelector('[aria-label="Rating"]')).not.toBeInTheDocument();
  });

  it("renders playlist rows using the playlist name", () => {
    const items: HomeItem[] = [{ type: "playlist", playlist: makePlaylist() }];
    const { getByText } = render(HomeRowList, { props: { items, variant: "rank" } });

    expect(getByText("Road Trip")).toBeInTheDocument();
  });

  it("shows the empty state when there are no items", () => {
    const { getByText } = render(HomeRowList, { props: { items: [], variant: "rank" } });

    expect(getByText(/personalized collections/i)).toBeInTheDocument();
  });

  it("renders the title heading when provided", () => {
    const items: HomeItem[] = [{ type: "song", song: makeSong() }];
    const { getByText } = render(HomeRowList, { props: { title: "Top 5 Most Played", items, variant: "rank" } });

    expect(getByText("Top 5 Most Played")).toBeInTheDocument();
  });
});
