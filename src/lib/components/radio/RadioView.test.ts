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

vi.mock("@tauri-apps/plugin-dialog", () => ({
  open: vi.fn(async () => null),
  save: vi.fn(async () => null),
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
import { playerStore } from "../../stores/player.svelte";

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
    reducedQuality: false,
    ...overrides,
  };
}

const defaultFacets: RadioDirectoryFacets = {
  tags: [{ name: "jazz", stationcount: 12 }],
  countries: [{ name: "Canada", code: "CA", stationcount: 5 }],
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
    radioStore.selectedCountry = null;
    radioStore.selectedLanguage = null;
    radioStore.selectedCodec = null;
    playerStore.state = "stopped";
    playerStore.currentSong = undefined;
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

  it("marks an HE-AAC station that plays at reduced quality", async () => {
    const heAac = makeStation({
      stationuuid: "uuid-aacp",
      name: "AAC+ Station",
      codec: "AAC+",
      bitrate: 48,
      reducedQuality: true,
    });
    mockInvoke.mockImplementation(async (cmd: string) => {
      if (cmd === "get_radio_directory_status")
        return {
          optedIn: true,
          stationCount: 1,
          playableStationCount: 1,
          lastSyncedAt: Math.floor(Date.now() / 1000),
          lastFullSyncAt: Math.floor(Date.now() / 1000),
          nextAutoSyncAt: null,
          syncInProgress: false,
          lastError: null,
          attributionUrl: "https://www.radio-browser.info",
        } satisfies RadioDirectoryStatus;
      if (cmd === "get_saved_radio_stations") return [];
      if (cmd === "get_recent_radio_stations") return [];
      if (cmd === "get_radio_playlist_ids") return [];
      if (cmd === "get_radio_directory_facets") return defaultFacets;
      if (cmd === "query_radio_directory") return [heAac];
      return null;
    });

    render(RadioView);

    expect(await screen.findByText("Reduced quality")).toBeInTheDocument();
    expect(screen.queryByText("Unsupported")).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Play AAC+ Station" })).toBeEnabled();
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

    // Select "All formats (incl. unsupported)" in Formats dropdown -> passes hideUnsupported: false.
    const codecSelect = screen.getByLabelText("Filter by format");
    await fireEvent.change(codecSelect, { target: { value: "__all__" } });

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

  it("opens StationListModal from My stations and creates a station list", async () => {
    const status: RadioDirectoryStatus = {
      optedIn: true,
      stationCount: 5,
      playableStationCount: 5,
      lastSyncedAt: Math.floor(Date.now() / 1000) - 3600,
      lastFullSyncAt: Math.floor(Date.now() / 1000) - 3600,
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
      if (cmd === "get_radio_directory_facets") return defaultFacets;
      if (cmd === "query_radio_directory") return [];
      if (cmd === "get_radio_station_list_tags") {
        return {
          tags: [{ name: "Folk Metal", stationcount: 14 }],
          suggestedFromLibrary: [{ name: "Folk Metal", stationcount: 14 }],
        };
      }
      if (cmd === "create_radio_station_list") {
        return {
          id: 99,
          name: "Folk Metal",
          track_count: 14,
          dynamic_enabled: true,
          dynamic_spec: "radio:tag=Folk Metal;order=clickcount;limit=100",
          is_system: false,
          is_queue: false,
          created: 1000,
          updated: 1000,
        };
      }
      if (cmd === "get_playlists") return [];
      return null;
    });

    render(RadioView);
    const myTab = await screen.findByTestId("radio-tab-my-stations");
    await fireEvent.click(myTab);

    const newBtn = await screen.findByTestId("new-station-list-btn");
    await fireEvent.click(newBtn);

    const modal = await screen.findByTestId("station-list-modal");
    expect(modal).toBeInTheDocument();

    const suggestions = await screen.findByTestId("station-list-suggestions");
    const folkBtn = suggestions.querySelector("button");
    expect(folkBtn).not.toBeNull();
    await fireEvent.click(folkBtn!);

    const saveBtn = screen.getByRole("button", { name: "Save" });
    await fireEvent.click(saveBtn);

    await waitFor(() => {
      expect(mockInvoke).toHaveBeenCalledWith("create_radio_station_list", {
        name: "Folk Metal",
        spec: {
          tags: ["Folk Metal"],
          country: null,
          lang: null,
          minBitrate: null,
          order: "clickcount",
          limit: 100,
        },
      });
    });
  });

  it("toggles play, pause, and resume when clicking a saved station card", async () => {
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

    mockInvoke.mockImplementation(async (cmd: string) => {
      if (cmd === "get_radio_directory_status") return status;
      if (cmd === "get_saved_radio_stations") return [saved];
      if (cmd === "get_recent_radio_stations") return [];
      if (cmd === "get_radio_playlist_ids") return [];
      if (cmd === "get_radio_directory_facets") return defaultFacets;
      if (cmd === "query_radio_directory") return [];
      if (cmd === "play_radio_station") {
        playerStore.currentSong = { id: 99, title: "CBC Music", path: "radio://uuid-saved" } as any;
        playerStore.state = "playing";
        return playerStore.currentSong;
      }
      if (cmd === "pause") {
        playerStore.state = "paused";
        return null;
      }
      if (cmd === "resume") {
        playerStore.state = "playing";
        return null;
      }
      return null;
    });

    radioStore.subView = "my_stations";
    render(RadioView);

    const playBtn = await screen.findByRole("button", { name: "Play CBC Music" });
    await fireEvent.click(playBtn);

    expect(mockInvoke).toHaveBeenCalledWith("play_radio_station", { stationuuid: "uuid-saved" });
    expect(radioStore.isPlayingStation("uuid-saved")).toBe(true);

    // Pause by clicking the card while playing — must not re-invoke play_radio_station.
    const pauseBtn = await screen.findByRole("button", { name: "Playing CBC Music" });
    await fireEvent.click(pauseBtn);

    expect(mockInvoke).toHaveBeenCalledWith("pause");
    expect(radioStore.isPlayingStation("uuid-saved")).toBe(false);

    // Resume by clicking the card while paused — must call resume, not play_radio_station.
    const resumeBtn = await screen.findByRole("button", { name: "Play CBC Music" });
    await fireEvent.click(resumeBtn);

    expect(mockInvoke).toHaveBeenCalledWith("resume");
    expect(radioStore.isPlayingStation("uuid-saved")).toBe(true);
    expect(mockInvoke.mock.calls.filter(([cmd]) => cmd === "play_radio_station")).toHaveLength(1);

    // If playback stops while the station is still currentSong, clicking restarts via play_radio_station.
    playerStore.state = "stopped";
    expect(radioStore.isPlayingStation("uuid-saved")).toBe(false);
    await fireEvent.click(await screen.findByRole("button", { name: "Play CBC Music" }));
    expect(mockInvoke.mock.calls.filter(([cmd]) => cmd === "play_radio_station")).toHaveLength(2);
  });

  it("opens AddStationModal from My stations and adds a custom station", async () => {
    const customStation = makeStation({
      stationuuid: "custom-123",
      name: "My Custom FM",
      urlResolved: "https://custom.example/live.mp3",
      tags: "indie",
    });
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
      if (cmd === "add_custom_radio_station") return customStation;
      return null;
    });

    render(RadioView);
    await fireEvent.click(screen.getByTestId("radio-tab-my-stations"));

    await fireEvent.click(await screen.findByTestId("radio-overflow-btn"));
    const addBtn = await screen.findByRole("menuitem", { name: "Add station" });
    await fireEvent.click(addBtn);

    const nameInput = await screen.findByTestId("add-station-name");
    const urlInput = screen.getByTestId("add-station-url");
    const tagsInput = screen.getByTestId("add-station-tags");
    await fireEvent.input(nameInput, { target: { value: "My Custom FM" } });
    await fireEvent.input(urlInput, { target: { value: "https://custom.example/live.pls" } });
    await fireEvent.input(tagsInput, { target: { value: "indie" } });

    await fireEvent.click(screen.getByTestId("add-station-submit"));

    await waitFor(() => {
      expect(mockInvoke).toHaveBeenCalledWith("add_custom_radio_station", {
        name: "My Custom FM",
        url: "https://custom.example/live.pls",
        tags: "indie",
      });
    });
    expect(await screen.findByText("My Custom FM")).toBeInTheDocument();
  });

  it("edits a custom station from its context menu", async () => {
    const customStation = makeStation({
      stationuuid: "custom-123",
      name: "My Custom FM",
      urlResolved: "https://custom.example/live.mp3",
      tags: "indie",
    });
    const updatedStation = makeStation({
      stationuuid: "custom-123",
      name: "Updated FM",
      urlResolved: "https://custom.example/hd.mp3",
      tags: "jazz",
    });
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
      if (cmd === "get_saved_radio_stations") return [customStation];
      if (cmd === "get_recent_radio_stations") return [];
      if (cmd === "get_radio_playlist_ids") return [];
      if (cmd === "update_custom_radio_station") return updatedStation;
      return null;
    });

    radioStore.subView = "my_stations";
    render(RadioView);

    const card = await screen.findByTestId("radio-station-card");
    await fireEvent.contextMenu(card);

    const editItem = await screen.findByRole("menuitem", { name: "Edit station" });
    await fireEvent.click(editItem);

    const nameInput = await screen.findByTestId("add-station-name");
    const urlInput = screen.getByTestId("add-station-url");
    const tagsInput = screen.getByTestId("add-station-tags");
    expect((nameInput as HTMLInputElement).value).toBe("My Custom FM");

    await fireEvent.input(nameInput, { target: { value: "Updated FM" } });
    await fireEvent.input(urlInput, { target: { value: "https://custom.example/hd.mp3" } });
    await fireEvent.input(tagsInput, { target: { value: "jazz" } });
    await fireEvent.click(screen.getByTestId("add-station-submit"));

    await waitFor(() => {
      expect(mockInvoke).toHaveBeenCalledWith("update_custom_radio_station", {
        stationuuid: "custom-123",
        name: "Updated FM",
        url: "https://custom.example/hd.mp3",
        tags: "jazz",
      });
    });
    expect(await screen.findByText("Updated FM")).toBeInTheDocument();
  });
});
