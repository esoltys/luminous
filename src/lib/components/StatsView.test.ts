import "@testing-library/jest-dom";
import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, waitFor, fireEvent } from "@testing-library/svelte";
import { invoke } from "@tauri-apps/api/core";
import StatsView from "./StatsView.svelte";
import { bucketListeningClock } from "../utils/listeningClock";
import { navigationStore } from "../stores/navigation.svelte";
import type { StatsSummary } from "../types";

function makeSummary(range: StatsSummary["range"]): StatsSummary {
  return {
    range,
    top_songs: [{ key: "1", label: "Song A", secondary: "Artist A", play_count: 5, excluded: false, album: "Album A" }],
    top_albums: [{ key: "Album A", label: "Album A", secondary: "Artist A", play_count: 3, excluded: false, album: null }],
    top_artists: [{ key: "Artist A", label: "Artist A", secondary: null, play_count: 5, excluded: false, album: null }],
    top_genres: [{ key: "Rock", label: "Rock", secondary: null, play_count: 5, excluded: false, album: null }],
    play_timestamps: [1_700_000_000, 1_700_003_600],
  };
}

const invokeMock = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));

describe("StatsView.svelte", () => {
  beforeEach(() => {
    invokeMock.mockReset();
    localStorage.clear();
    invokeMock.mockImplementation((cmd: string, args: { range?: string }) => {
      if (cmd === "get_stats_summary") {
        return Promise.resolve(makeSummary((args?.range as StatsSummary["range"]) ?? "7d"));
      }
      return Promise.resolve(null);
    });
  });

  it("fetches the 7-day range on mount and renders Top 10 sections", async () => {
    const { getByText, getAllByText } = render(StatsView);
    await waitFor(() => expect(invokeMock).toHaveBeenCalledWith("get_stats_summary", { range: "7d" }));
    await waitFor(() => expect(getByText("Song A")).toBeInTheDocument());
    expect(getByText("Album A")).toBeInTheDocument();
    // "Artist A" appears twice: as the song row's secondary line and as the Top Artists entry.
    expect(getAllByText("Artist A").length).toBeGreaterThanOrEqual(2);
    expect(getByText("Rock")).toBeInTheDocument();
  });

  it("refetches when the range switcher changes", async () => {
    const { getByText } = render(StatsView);
    await waitFor(() => expect(invokeMock).toHaveBeenCalledWith("get_stats_summary", { range: "7d" }));

    await fireEvent.click(getByText("Past 28 Days"));

    await waitFor(() => expect(invokeMock).toHaveBeenCalledWith("get_stats_summary", { range: "28d" }));
    expect(localStorage.getItem("stats_range")).toBe("28d");
  });

  it("restores the previously selected range from localStorage on mount", async () => {
    localStorage.setItem("stats_range", "1y");
    render(StatsView);
    await waitFor(() => expect(invokeMock).toHaveBeenCalledWith("get_stats_summary", { range: "1y" }));
  });

  it("navigates to the artist detail page when a Top Artists row is clicked", async () => {
    navigationStore.selectedArtistName = null;
    const { getAllByText } = render(StatsView);
    await waitFor(() => expect(getAllByText("Artist A").length).toBeGreaterThan(0));

    await fireEvent.click(getAllByText("Artist A")[0].closest('[role="button"]')!);

    expect(navigationStore.selectedArtistName).toBe("Artist A");
  });

  it("navigates to the album detail page when a Top Albums row is clicked", async () => {
    navigationStore.selectedAlbumName = null;
    const { getByText } = render(StatsView);
    await waitFor(() => expect(getByText("Album A")).toBeInTheDocument());

    await fireEvent.click(getByText("Album A").closest('[role="button"]')!);

    expect(navigationStore.selectedAlbumName).toBe("Album A");
  });

  it("navigates to the song's album when a Top Songs row is clicked", async () => {
    navigationStore.selectedAlbumName = null;
    const { getByText } = render(StatsView);
    await waitFor(() => expect(getByText("Song A")).toBeInTheDocument());

    await fireEvent.click(getByText("Song A").closest('[role="button"]')!);

    expect(navigationStore.selectedAlbumName).toBe("Album A");
  });

  it("shows the empty state when there's no listening history in range", async () => {
    invokeMock.mockImplementation((cmd: string) => {
      if (cmd === "get_stats_summary") {
        return Promise.resolve({
          range: "7d",
          top_songs: [],
          top_albums: [],
          top_artists: [],
          top_genres: [],
          play_timestamps: [],
        } satisfies StatsSummary);
      }
      return Promise.resolve(null);
    });

    const { getByText } = render(StatsView);
    await waitFor(() => expect(getByText("No listening history for this range yet.")).toBeInTheDocument());
  });
});

describe("bucketListeningClock", () => {
  it("buckets local hours into morning/afternoon/evening/late-night", () => {
    const morning = new Date();
    morning.setHours(9, 0, 0, 0);
    const afternoon = new Date();
    afternoon.setHours(14, 0, 0, 0);
    const evening = new Date();
    evening.setHours(19, 0, 0, 0);
    const lateNight = new Date();
    lateNight.setHours(2, 0, 0, 0);

    const timestamps = [morning, afternoon, evening, lateNight].map((d) => Math.floor(d.getTime() / 1000));
    const counts = bucketListeningClock(timestamps);

    expect(counts.morning).toBe(1);
    expect(counts.afternoon).toBe(1);
    expect(counts.evening).toBe(1);
    expect(counts.latenight).toBe(1);
  });

  it("returns zero counts for an empty timestamp list", () => {
    expect(bucketListeningClock([])).toEqual({ morning: 0, afternoon: 0, evening: 0, latenight: 0 });
  });

  it("agrees with getDaypartBucket at the evening/late-night boundary (21:00)", () => {
    const boundary = new Date();
    boundary.setHours(21, 0, 0, 0);
    const counts = bucketListeningClock([Math.floor(boundary.getTime() / 1000)]);
    expect(counts.latenight).toBe(1);
    expect(counts.evening).toBe(0);
  });
});
