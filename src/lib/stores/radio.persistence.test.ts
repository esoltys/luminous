import { describe, it, expect, vi, beforeEach } from "vitest";
import type { RadioDirectoryFacets, RadioDirectoryStatus } from "../types";

const mockInvoke = vi.hoisted(() => vi.fn());
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => mockInvoke(...args),
}));
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(async () => () => {}),
}));

const status: RadioDirectoryStatus = {
  optedIn: true,
  stationCount: 3,
  playableStationCount: 3,
  lastSyncedAt: null,
  lastFullSyncAt: null,
  nextAutoSyncAt: null,
  syncInProgress: false,
  lastError: null,
  attributionUrl: "https://www.radio-browser.info",
};

function facets(languages: string[]): RadioDirectoryFacets {
  return {
    tags: [],
    countries: [{ name: "Canada", code: "CA", stationcount: 2 }],
    languages: languages.map((name) => ({ name, stationcount: 1 })),
    codecs: [{ name: "MP3", stationcount: 3 }],
  };
}

function mockBackend(languages: string[]) {
  mockInvoke.mockImplementation(async (cmd: string) => {
    if (cmd === "get_radio_directory_status") return status;
    if (cmd === "get_radio_directory_facets") return facets(languages);
    if (cmd === "query_radio_directory") return [];
    return [];
  });
}

/** A fresh store, as after an app restart (the module is re-evaluated). */
async function freshStore() {
  vi.resetModules();
  const mod = await import("./radio.svelte");
  return mod.radioStore;
}

function lastDiscoverQuery() {
  const calls = mockInvoke.mock.calls.filter(([cmd]) => cmd === "query_radio_directory");
  return calls.at(-1)?.[1]?.query;
}

describe("radio Discover filter persistence", () => {
  beforeEach(() => {
    localStorage.clear();
    mockInvoke.mockReset();
  });

  it("restores search, filters and sort after a restart and reruns the query with them", async () => {
    mockBackend(["english"]);
    const before = await freshStore();
    before.setFilter("country", "CA");
    before.setFilter("language", "english");
    before.setMinBitrate(128);
    before.setSearchQuery("jazz", true);
    before.orderBy = "bitrate";
    before.dispose();

    mockInvoke.mockClear();
    const after = await freshStore();
    expect(after.searchQuery).toBe("jazz");
    expect(after.selectedCountry).toBe("CA");
    expect(after.selectedLanguage).toBe("english");
    expect(after.selectedMinBitrate).toBe(128);
    expect(after.orderBy).toBe("bitrate");

    await after.refreshStatusAndData();
    expect(lastDiscoverQuery()).toMatchObject({
      search: "jazz",
      countrycode: "CA",
      language: "english",
      minBitrate: 128,
      orderBy: "bitrate",
    });

    after.clearFilters();
    const cleared = await freshStore();
    expect(cleared.selectedCountry).toBeNull();
    expect(cleared.searchQuery).toBe("");
  });

  it("drops a restored language the directory no longer has, keeping the other filters", async () => {
    mockBackend(["english", "klingon"]);
    const before = await freshStore();
    before.setFilter("country", "CA");
    before.setFilter("language", "klingon");

    mockBackend(["english"]);
    const after = await freshStore();
    await after.refreshStatusAndData();

    expect(after.selectedLanguage).toBeNull();
    expect(after.selectedCountry).toBe("CA");
    expect(lastDiscoverQuery()).toMatchObject({ countrycode: "CA", language: null });
    expect((await freshStore()).selectedLanguage).toBeNull();
  });
});
