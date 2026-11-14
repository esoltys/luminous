import "@testing-library/jest-dom";
import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, waitFor } from "@testing-library/svelte";
import HomeView from "./HomeView.svelte";
import { collectionStore } from "../stores/collection.svelte";
import { navigationStore } from "../stores/navigation.svelte";
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

function makeTopAlbum(
  overrides: Partial<TopAlbumItem> = {},
  albumOverrides: Partial<AlbumItem> = {},
): TopAlbumItem {
  return {
    album: makeAlbum({ album: "Full Moon Fever", sample_song_id: 1, ...albumOverrides }),
    rank: 1,
    previous_rank: null,
    peak_rank: 1,
    weeks_on_chart: 1,
    movement: "new",
    period_start: 1_700_000_000,
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
  invoke: vi.fn((cmd: string, args?: any) => {
    if (cmd === "get_top_albums") return Promise.resolve(mockTopAlbums);
    if (cmd === "get_recently_added") return Promise.resolve(mockRecentlyAdded);
    if (cmd === "get_featured_albums") return Promise.resolve(mockFeaturedAlbums);
    // collectionStore initializes itself on module load and expects this shape.
    if (cmd === "get_library_snapshot")
      return Promise.resolve({ songs: [], albums: [], artists: [] });
    return Promise.resolve([]);
  }),
}));

describe("HomeView.svelte", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    localStorage.clear();
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
      album_art_bytes: 0,
      artist_art_bytes: 0,
      thumbnail_bytes: 0,
    };
    collectionStore.songs = [];
    collectionStore.albums = [];
    collectionStore.libraryLoaded = true;
  });

  it("fetches all three curated home queries on mount", async () => {
    render(HomeView);

    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith("get_top_albums", { limit: 10 });
      expect(invoke).toHaveBeenCalledWith("get_recently_added", { limit: 10 });
      expect(invoke).toHaveBeenCalledWith("get_featured_albums", { limit: 5 });
    });
  });

  it("shows LibraryWelcome and hides the time-of-day greeting when the library is genuinely empty", async () => {
    render(HomeView);

    await waitFor(() => {
      expect(screen.getByText("Play your music")).toBeInTheDocument();
    });
    expect(screen.queryByRole("heading", { level: 1 })).not.toBeInTheDocument();
  });

  it("does not show LibraryWelcome once recentlyAdded has content, even with zero plays", async () => {
    mockRecentlyAdded = [{ type: "album", album: makeAlbum() }];

    render(HomeView);

    await waitFor(() => {
      expect(screen.getAllByText("Full Moon Fever").length).toBeGreaterThan(0);
    });
    expect(screen.queryByText("Play your music")).not.toBeInTheDocument();
    expect(screen.getByRole("heading", { level: 1 })).toBeInTheDocument();
  });

  it("shows the Explore Your Library row when there is no play history, and hides it once there is", async () => {
    mockFeaturedAlbums = [{ type: "album", album: makeAlbum({ album: "Discover Me" }) }];

    render(HomeView);

    await waitFor(() => {
      expect(screen.getByText("Explore Your Library")).toBeInTheDocument();
    });
  });

  it("hides the Explore Your Library row in favor of Top Albums once play history exists", async () => {
    mockTopAlbums = [makeTopAlbum({}, { album: "Chart Topper", artist: "Tom Petty" })];
    mockFeaturedAlbums = [{ type: "album", album: makeAlbum({ album: "Discover Me" }) }];

    render(HomeView);

    await waitFor(() => {
      expect(screen.getByText(/^Top albums this week/)).toBeInTheDocument();
      expect(screen.getByText("Chart Topper")).toBeInTheDocument();
    });
    expect(screen.queryByText("Explore Your Library")).not.toBeInTheDocument();
  });

  it("navigates to the Stats view when the Top Albums heading is clicked", async () => {
    mockTopAlbums = [makeTopAlbum({}, { album: "Chart Topper", artist: "Tom Petty" })];
    render(HomeView);

    await waitFor(() => {
      expect(screen.getByText(/^Top albums this week/)).toBeInTheDocument();
    });

    screen.getByText(/^Top albums this week/).click();

    expect(navigationStore.activeTab).toBe("stats");
  });

  it("shows the chart week's compact date range as a quieter suffix on the Top Albums heading", async () => {
    mockTopAlbums = [
      makeTopAlbum({ period_start: 1_700_000_000 }, { album: "Chart Topper", artist: "Tom Petty" }),
    ];
    render(HomeView);

    await waitFor(() => {
      expect(screen.getByText(/^Nov 14\s*[–-]\s*20$/)).toBeInTheDocument();
    });
    expect(screen.getByText(/^Top albums this week/)).toBeInTheDocument();
  });

  it("draws list rows without a box, separated by a hairline", async () => {
    mockTopAlbums = [makeTopAlbum({}, { album: "Chart Topper" })];
    render(HomeView);

    await waitFor(() => {
      expect(screen.getByText("Chart Topper")).toBeInTheDocument();
    });

    const row = screen.getByText("Chart Topper").closest('[role="button"]');
    expect(row).not.toBeNull();
    expect(row!.classList).not.toContain("bg-brand-sidebar");
    expect(row!.classList).not.toContain("rounded-lg");
    expect(row!.classList).toContain("border-b");
  });

  it("ranks Top Albums with a plain number and says in words how an album moved", async () => {
    mockTopAlbums = [
      makeTopAlbum({ rank: 1, movement: "new" }, { album: "Fresh One" }),
      makeTopAlbum({ rank: 2, movement: "rising", previous_rank: 4 }, { album: "Climber" }),
      makeTopAlbum({ rank: 3, movement: "falling", previous_rank: 2 }, { album: "Slider" }),
      makeTopAlbum({ rank: 4, movement: "steady", previous_rank: 4 }, { album: "Holder" }),
    ];
    render(HomeView);

    await waitFor(() => {
      expect(screen.getByText("Fresh One")).toBeInTheDocument();
    });

    expect(screen.getByText("1")).toBeInTheDocument();
    expect(screen.queryByText("01")).not.toBeInTheDocument();
    expect(screen.getByText("New this week")).toBeInTheDocument();
    expect(screen.getByText("Up 2")).toBeInTheDocument();
    expect(screen.getByText("Down 1")).toBeInTheDocument();
    // Steady albums say nothing, and the old "Peak #n" line is gone.
    expect(screen.queryByText("Steady")).not.toBeInTheDocument();
    expect(screen.queryByText(/Peak/)).not.toBeInTheDocument();
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

  it("refreshes Top Albums when an album is excluded from or restored to stats", async () => {
    render(HomeView);

    await waitFor(() => {
      expect(listenCallbacks["stats-exclusions-changed"]).toBeDefined();
    });
    vi.mocked(invoke).mockClear();

    listenCallbacks["stats-exclusions-changed"]({ payload: undefined });

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
