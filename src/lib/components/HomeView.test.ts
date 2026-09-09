import "@testing-library/jest-dom";
import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, waitFor } from "@testing-library/svelte";
import HomeView from "./HomeView.svelte";
import { collectionStore } from "../stores/collection.svelte";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type { HomeItem, AlbumItem, TopAlbumItem, ScanProgress } from "../types";

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
    genre: "Rock",
    rating: -1,
    total_duration_nanosec: 0,
    ...overrides,
  };
}

function makeTopAlbum(overrides: Partial<TopAlbumItem> = {}): TopAlbumItem {
  return {
    album: makeAlbum(),
    rank: 1,
    previous_rank: null,
    peak_rank: 1,
    weeks_on_chart: 1,
    movement: "new",
    ...overrides,
  };
}

// Captures listen() callbacks by event name so tests can fire them directly,
// mirroring how the real backend would emit scan-progress/library-changed.
const { listenCallbacks } = vi.hoisted(() => ({
  listenCallbacks: {} as Record<string, (event: { payload: unknown }) => void>,
}));

vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn((event: string, callback: (e: { payload: unknown }) => void) => {
    listenCallbacks[event] = callback;
    return Promise.resolve(() => {});
  }),
}));

let mockTopAlbums: TopAlbumItem[] = [];
let mockRecentlyAdded: HomeItem[] = [];
let mockFeaturedAlbums: HomeItem[] = [];

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn((cmd: string) => {
    if (cmd === "get_top_albums") return Promise.resolve(mockTopAlbums);
    if (cmd === "get_recently_added") return Promise.resolve(mockRecentlyAdded);
    if (cmd === "get_featured_albums") return Promise.resolve(mockFeaturedAlbums);
    // collectionStore initializes itself on module load and expects this shape.
    if (cmd === "get_library_snapshot") return Promise.resolve({ songs: [], albums: [], artists: [] });
    return Promise.resolve([]);
  }),
}));

describe("HomeView.svelte", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    for (const key of Object.keys(listenCallbacks)) delete listenCallbacks[key];
    mockTopAlbums = [];
    mockRecentlyAdded = [];
    mockFeaturedAlbums = [];
    collectionStore.stats = {
      total_songs: 0,
      total_artists: 0,
      total_albums: 0,
      total_duration_nanosec: 0,
      total_filesize_bytes: 0,
    };
    collectionStore.songs = [];
    collectionStore.albums = [];
  });

  it("fetches all three curated home queries on mount", async () => {
    render(HomeView);

    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith("get_top_albums", { limit: 10 });
      expect(invoke).toHaveBeenCalledWith("get_recently_added", { limit: 12 });
      expect(invoke).toHaveBeenCalledWith("get_featured_albums", { limit: 5 });
    });
  });

  it("shows LibraryWelcome when the library is genuinely empty", async () => {
    render(HomeView);

    await waitFor(() => {
      expect(screen.getByText("Welcome to Luminous")).toBeInTheDocument();
    });
  });

  it("does not show LibraryWelcome once recentlyAdded has content, even with zero plays", async () => {
    mockRecentlyAdded = [{ type: "album", album: makeAlbum() }];

    render(HomeView);

    await waitFor(() => {
      expect(screen.getAllByText("Full Moon Fever").length).toBeGreaterThan(0);
    });
    expect(screen.queryByText("Welcome to Luminous")).not.toBeInTheDocument();
  });

  it("shows the Explore Your Library row when there is no play history, and hides it once there is", async () => {
    mockFeaturedAlbums = [{ type: "album", album: makeAlbum({ album: "Discover Me" }) }];

    render(HomeView);

    await waitFor(() => {
      expect(screen.getByText("Explore Your Library")).toBeInTheDocument();
    });
  });

  it("hides the Explore Your Library row in favor of Top 10 Albums once play history exists", async () => {
    mockTopAlbums = [makeTopAlbum({ album: makeAlbum({ album: "Chart Topper" }) })];
    mockFeaturedAlbums = [{ type: "album", album: makeAlbum({ album: "Discover Me" }) }];

    render(HomeView);

    await waitFor(() => {
      expect(screen.getByText("Top 10 Albums")).toBeInTheDocument();
    });
    expect(screen.queryByText("Explore Your Library")).not.toBeInTheDocument();
  });

  it("refreshes curated data when a scan-progress 'done' event fires", async () => {
    render(HomeView);

    await waitFor(() => {
      expect(listenCallbacks["scan-progress"]).toBeDefined();
    });
    vi.mocked(invoke).mockClear();

    const doneEvent: ScanProgress = { phase: "done", scanned: 10, total: 10, silent: false };
    listenCallbacks["scan-progress"]({ payload: doneEvent });

    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith("get_top_albums", { limit: 10 });
    });
  });

  it("debounces rapid library-changed events into a single refresh", async () => {
    vi.useFakeTimers();
    try {
      render(HomeView);

      await vi.waitFor(() => {
        expect(listenCallbacks["library-changed"]).toBeDefined();
      });
      vi.mocked(invoke).mockClear();

      // Simulate a burst of per-file library-changed events during a scan.
      for (let i = 0; i < 20; i++) {
        listenCallbacks["library-changed"]({ payload: undefined });
      }
      expect(invoke).not.toHaveBeenCalled();

      await vi.advanceTimersByTimeAsync(500);

      const topAlbumsCalls = vi
        .mocked(invoke)
        .mock.calls.filter(([cmd]) => cmd === "get_top_albums");
      expect(topAlbumsCalls).toHaveLength(1);
    } finally {
      vi.useRealTimers();
    }
  });

  it("matches detail view header top whitespace using pt-6", () => {
    const { container } = render(HomeView);
    const greetingHeading = container.querySelector("h1");
    expect(greetingHeading).toBeInTheDocument();
    expect(greetingHeading?.parentElement).toHaveClass("pt-6");
    expect(greetingHeading?.parentElement).not.toHaveClass("pt-8");
  });
});
