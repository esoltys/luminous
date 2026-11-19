import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, fireEvent, waitFor } from "@testing-library/svelte";
import type {
  RadioDirectoryFacets,
  RadioDirectoryStation,
  RadioDirectoryStatus,
} from "../../types";

vi.mock("svelte-virtual-list-ts", async () => {
  const mod = await import("../__mocks__/VirtualList.svelte");
  return { VirtualList: mod.default };
});

const listeners = vi.hoisted(() => ({}) as Record<string, (event: { payload: any }) => void>);

const mockInvoke = vi.hoisted(() => vi.fn());
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: any[]) => mockInvoke(...args),
}));

vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(async (eventName: string, handler: (event: { payload: any }) => void) => {
    listeners[eventName] = handler;
    return () => {
      delete listeners[eventName];
    };
  }),
}));

import RadioView from "./RadioView.svelte";
import { radioStore } from "../../stores/radio.svelte";

function makeStation(overrides: Partial<RadioDirectoryStation> = {}): RadioDirectoryStation {
  return {
    stationuuid: "uuid-1",
    name: "Jazz FM91",
    urlResolved: "https://stream.example/jazz.mp3",
    homepage: "https://jazz.fm",
    favicon: "",
    tags: "jazz,smooth jazz",
    country: "Canada",
    countrycode: "CA",
    language: "english",
    codec: "MP3",
    bitrate: 128,
    hls: false,
    votes: 42,
    clickcount: 100,
    lastcheckok: true,
    changeuuid: "chg-1",
    updatedAt: 1700000000,
    playable: true,
    ...overrides,
  };
}

const defaultFacets: RadioDirectoryFacets = {
  tags: [{ name: "jazz", stationcount: 12 }],
  countries: [{ name: "Canada", isoCode: "CA", stationcount: 5 }],
  languages: [{ name: "english", stationcount: 10 }],
  codecs: [{ name: "MP3", stationcount: 8 }],
};

describe("RadioView", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    localStorage.clear();
    for (const key of Object.keys(listeners)) {
      delete listeners[key];
    }
    radioStore.dispose();
    radioStore.subView = "discover";
    radioStore.searchQuery = "";
    radioStore.selectedTag = null;
    radioStore.selectedCountry = null;
    radioStore.selectedLanguage = null;
    radioStore.selectedCodec = null;
  });

  it("shows opt-in card when not downloaded and triggers sync on click", async () => {
    const status: RadioDirectoryStatus = {
      optedIn: false,
      stationCount: 0,
      playableStationCount: 0,
      lastSyncedAt: null,
      lastFullSyncAt: null,
      nextAutoSyncAt: null,
      syncInProgress: false,
      lastError: null,
      attributionUrl: "https://www.radio-browser.info",
    };

    mockInvoke.mockImplementation(async (cmd: string) => {
      if (cmd === "get_radio_directory_status") return status;
      if (cmd === "get_saved_radio_stations") return [];
      if (cmd === "get_recent_radio_stations") return [];
      if (cmd === "get_radio_playlist_ids") return [];
      if (cmd === "sync_radio_directory") {
        return { mode: "full", upserted: 1, removed: 0, totalStations: 1, syncedAt: 1700000000 };
      }
      return null;
    });

    render(RadioView);

    const optInCard = await screen.findByTestId("radio-opt-in-card");
    expect(optInCard).toBeInTheDocument();

    const downloadBtn = screen.getByTestId("radio-download-index-btn");
    await fireEvent.click(downloadBtn);

    expect(mockInvoke).toHaveBeenCalledWith("sync_radio_directory");
  });

  it("shows syncing progress state from radio-directory-sync-progress events", async () => {
    const status: RadioDirectoryStatus = {
      optedIn: false,
      stationCount: 0,
      playableStationCount: 0,
      lastSyncedAt: null,
      lastFullSyncAt: null,
      nextAutoSyncAt: null,
      syncInProgress: true,
      lastError: null,
      attributionUrl: "https://www.radio-browser.info",
    };

    mockInvoke.mockImplementation(async (cmd: string) => {
      if (cmd === "get_radio_directory_status") return status;
      if (cmd === "get_saved_radio_stations") return [];
      if (cmd === "get_recent_radio_stations") return [];
      if (cmd === "get_radio_playlist_ids") return [];
      return null;
    });

    render(RadioView);

    const progress = await screen.findByTestId("radio-sync-progress");
    expect(progress).toBeInTheDocument();

    listeners["radio-directory-sync-progress"]?.({
      payload: { mode: "full", fetchedStations: 250, page: 3, done: false },
    });

    await waitFor(() => {
      expect(screen.getByText(/250/)).toBeInTheDocument();
    });
  });

  it("shows offline error banner when sync fails", async () => {
    const status: RadioDirectoryStatus = {
      optedIn: false,
      stationCount: 0,
      playableStationCount: 0,
      lastSyncedAt: null,
      lastFullSyncAt: null,
      nextAutoSyncAt: null,
      syncInProgress: false,
      lastError: "DNS lookup failed",
      attributionUrl: "https://www.radio-browser.info",
    };

    mockInvoke.mockImplementation(async (cmd: string) => {
      if (cmd === "get_radio_directory_status") return status;
      if (cmd === "get_saved_radio_stations") return [];
      if (cmd === "get_recent_radio_stations") return [];
      if (cmd === "get_radio_playlist_ids") return [];
      return null;
    });

    render(RadioView);

    const errBanner = await screen.findByTestId("radio-sync-error");
    expect(errBanner).toHaveTextContent("DNS lookup failed");
  });

  it("renders Discover stations, empty filter state, and Show unsupported toggle", async () => {
    const playableStation = makeStation({
      stationuuid: "uuid-1",
      name: "Jazz FM91",
      playable: true,
    });
    const unsupportedStation = makeStation({
      stationuuid: "uuid-2",
      name: "HLS Stream",
      hls: true,
      playable: false,
    });

    const staleTs = Math.floor(Date.now() / 1000) - 48 * 3600;
    const status: RadioDirectoryStatus = {
      optedIn: true,
      stationCount: 2,
      playableStationCount: 1,
      lastSyncedAt: staleTs,
      lastFullSyncAt: staleTs,
      nextAutoSyncAt: null,
      syncInProgress: false,
      lastError: null,
      attributionUrl: "https://www.radio-browser.info",
    };

    mockInvoke.mockImplementation(async (cmd: string, args?: any) => {
      if (cmd === "get_radio_directory_status") return status;
      if (cmd === "get_saved_radio_stations") return [];
      if (cmd === "get_recent_radio_stations") return [];
      if (cmd === "get_radio_playlist_ids") return [];
      if (cmd === "get_radio_directory_facets") return defaultFacets;
      if (cmd === "query_radio_directory") {
        if (args?.query?.tag === "empty-tag") return [];
        if (args?.query?.hideUnsupported === false) return [playableStation, unsupportedStation];
        return [playableStation];
      }
      if (cmd === "save_radio_station") return null;
      if (cmd === "play_radio_station") return { id: 99, title: "Jazz FM91" };
      return null;
    });

    render(RadioView);

    await waitFor(() => {
      expect(screen.getByText("Jazz FM91")).toBeInTheDocument();
    });
    expect(screen.getByTestId("radio-stale-indicator")).toBeInTheDocument();

    // Toggle "Show unsupported" -> passes hideUnsupported: false and renders unsupported badge.
    const toggle = screen.getByTestId("radio-show-unsupported-toggle");
    await fireEvent.click(toggle);

    await waitFor(() => {
      expect(screen.getByText("HLS Stream")).toBeInTheDocument();
      expect(screen.getByText("Unsupported")).toBeInTheDocument();
    });

    // Save and Play actions
    const saveBtns = screen.getAllByRole("button", { name: "Save station" });
    await fireEvent.click(saveBtns[0]);
    expect(mockInvoke).toHaveBeenCalledWith("save_radio_station", { stationuuid: "uuid-1" });

    const playBtn = screen.getByRole("button", { name: "Play Jazz FM91" });
    await fireEvent.click(playBtn);
    expect(mockInvoke).toHaveBeenCalledWith("play_radio_station", { stationuuid: "uuid-1" });
  });

  it("renders My stations empty and populated states", async () => {
    const saved = makeStation({ stationuuid: "uuid-saved", name: "CBC Music" });
    const status: RadioDirectoryStatus = {
      optedIn: true,
      stationCount: 1,
      playableStationCount: 1,
      lastSyncedAt: Math.floor(Date.now() / 1000),
      lastFullSyncAt: Math.floor(Date.now() / 1000),
      nextAutoSyncAt: null,
      syncInProgress: false,
      lastError: null,
      attributionUrl: "https://www.radio-browser.info",
    };

    let returnSaved = false;
    mockInvoke.mockImplementation(async (cmd: string) => {
      if (cmd === "get_radio_directory_status") return status;
      if (cmd === "get_saved_radio_stations") return returnSaved ? [saved] : [];
      if (cmd === "get_recent_radio_stations") return [];
      if (cmd === "get_radio_directory_facets") return defaultFacets;
      if (cmd === "query_radio_directory") return [];
      return null;
    });

    render(RadioView);

    const myTab = screen.getByTestId("radio-tab-my-stations");
    await fireEvent.click(myTab);

    const emptyBox = await screen.findByTestId("radio-saved-empty");
    expect(emptyBox).toBeInTheDocument();

    returnSaved = true;
    await radioStore.loadMyStations();

    await waitFor(() => {
      expect(screen.getByText("CBC Music")).toBeInTheDocument();
    });
  });
});
