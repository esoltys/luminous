import "@testing-library/jest-dom";
import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, fireEvent } from "@testing-library/svelte";
import TopTenList from "./TopTenList.svelte";
import { navigationStore } from "../stores/navigation.svelte";
import type { StatsTopItem } from "../types";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn((cmd: string) => {
    if (cmd === "set_song_rating") return Promise.resolve(5);
    if (cmd === "set_album_rating") return Promise.resolve(4);
    if (cmd === "get_cover_art_uri") return Promise.resolve(null);
    if (cmd === "fetch_remote_cover") return Promise.resolve(null);
    return Promise.resolve(null);
  }),
}));

describe("TopTenList.svelte", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it("renders a 2-digit rank numeral and item labels", () => {
    const items: StatsTopItem[] = [
      {
        key: "1",
        label: "Song One",
        secondary: "Artist One",
        play_count: 10,
        minutes: 32,
        excluded: false,
        album: "Album One",
        song_id: 101,
        art_embedded: false,
        year: 2021,
        rating: 5,
      },
    ];

    const { getByText } = render(TopTenList, {
      props: {
        title: "Top Songs",
        items,
        kind: "song",
      },
    });

    expect(getByText("Top Songs")).toBeInTheDocument();
    expect(getByText("01")).toBeInTheDocument();
    expect(getByText("Song One")).toBeInTheDocument();
    expect(getByText("Artist One")).toBeInTheDocument();
    expect(getByText("2021")).toBeInTheDocument();
    expect(getByText("32 min")).toBeInTheDocument();
  });

  it("uses secondaryFallback when secondary is null or empty", () => {
    const items: StatsTopItem[] = [
      {
        key: "album_1",
        label: "Compilation Album",
        secondary: null,
        play_count: 5,
        minutes: 20,
        excluded: false,
        album: null,
        sample_song_id: 202,
      },
    ];

    const { getByText } = render(TopTenList, {
      props: {
        title: "Top Albums",
        items,
        kind: "album",
        secondaryFallback: "Various Artists",
      },
    });

    expect(getByText("Compilation Album")).toBeInTheDocument();
    expect(getByText("Various Artists")).toBeInTheDocument();
  });

  it("navigates to album when an album item is clicked", async () => {
    navigationStore.selectedAlbumName = null;
    const items: StatsTopItem[] = [
      {
        key: "album_1",
        label: "Greatest Hits",
        secondary: "Queen",
        play_count: 15,
        minutes: 60,
        excluded: false,
        album: null,
        sample_song_id: 1,
      },
    ];

    const { getByText } = render(TopTenList, {
      props: {
        items,
        kind: "album",
      },
    });

    await fireEvent.click(getByText("Greatest Hits"));
    expect(navigationStore.selectedAlbumName).toBe("Greatest Hits");
  });

  it("navigates to artist when an artist item is clicked", async () => {
    navigationStore.selectedArtistName = null;
    const items: StatsTopItem[] = [
      {
        key: "Queen",
        label: "Queen",
        secondary: null,
        play_count: 50,
        minutes: 200,
        excluded: false,
        album: null,
      },
    ];

    const { getByText } = render(TopTenList, {
      props: {
        items,
        kind: "artist",
      },
    });

    await fireEvent.click(getByText("Queen"));
    expect(navigationStore.selectedArtistName).toBe("Queen");
  });

  it("hides the duration column when showDuration is false", () => {
    const items: StatsTopItem[] = [
      {
        key: "1",
        label: "Song One",
        secondary: "Artist One",
        play_count: 10,
        minutes: 32,
        excluded: false,
        album: "Album One",
        song_id: 101,
        art_embedded: false,
        year: 2021,
        rating: 5,
      },
    ];

    const { queryByText } = render(TopTenList, {
      props: {
        items,
        kind: "song",
        showDuration: false,
      },
    });

    expect(queryByText("32 min")).not.toBeInTheDocument();
  });

  it("calls onHeaderClick when the title is clicked", async () => {
    const onHeaderClick = vi.fn();

    const { getByText } = render(TopTenList, {
      props: {
        title: "Top Albums",
        items: [],
        kind: "album",
        onHeaderClick,
      },
    });

    await fireEvent.click(getByText("Top Albums"));
    expect(onHeaderClick).toHaveBeenCalledOnce();
  });

  it("shows a movement indicator title for chart items but not for items without movement data", () => {
    const items: StatsTopItem[] = [
      {
        key: "album_1",
        label: "Rising Album",
        secondary: "Some Artist",
        play_count: 0,
        minutes: 0,
        excluded: false,
        album: null,
        sample_song_id: 1,
        movement: "rising",
        previous_rank: 3,
        peak_rank: 1,
        weeks_on_chart: 2,
      },
      {
        key: "album_2",
        label: "Ordinary Album",
        secondary: "Another Artist",
        play_count: 5,
        minutes: 10,
        excluded: false,
        album: null,
        sample_song_id: 2,
      },
    ];

    const { container } = render(TopTenList, {
      props: {
        items,
        kind: "album",
      },
    });

    const movementColumns = container.querySelectorAll("[title*='Rising']");
    expect(movementColumns).toHaveLength(1);
  });

  it("renders custom emptyText when items array is empty", () => {
    const { getByText } = render(TopTenList, {
      props: {
        items: [],
        kind: "album",
        emptyText: "Custom empty message",
      },
    });

    expect(getByText("Custom empty message")).toBeInTheDocument();
  });
});
