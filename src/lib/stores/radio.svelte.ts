import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type {
  Playlist,
  RadioBrowserFacetEntry,
  RadioDirectoryFacets,
  RadioDirectoryOrder,
  RadioDirectoryStation,
  RadioDirectoryStatus,
  RadioDirectorySyncProgress,
  RadioDirectorySyncStats,
  RadioStationListSpec,
  RadioStationListTags,
  Song,
} from "../types";
import { playerStore } from "./player.svelte";
import { playlistsStore } from "./playlists.svelte";
import { toastStore } from "./toast.svelte";
import { i18n } from "./i18n.svelte";

type RadioSubView = "my_stations" | "discover";

export function stationUuidFromPath(path?: string | null): string | null {
  if (!path?.startsWith("radio://")) return null;
  const uuid = path.slice("radio://".length).trim();
  return uuid || null;
}

export function formatUpdatedAgo(lastSyncedAt: number | null): string {
  if (!lastSyncedAt) return "";
  const diffSecs = Math.max(0, Math.floor(Date.now() / 1000) - lastSyncedAt);
  const hours = Math.floor(diffSecs / 3600);
  if (hours < 24) {
    return i18n.t("radio.updatedHoursAgo", { count: Math.max(1, hours) });
  }
  const days = Math.floor(hours / 24);
  return i18n.t("radio.updatedDaysAgo", { count: days });
}

const STORAGE_SUB_VIEW = "radio_subView";
const STORAGE_HIDE_UNSUPPORTED = "radio_hideUnsupported";
const STORAGE_SCROLL_MY = "radio_scrollTop_my_stations";
const STORAGE_SCROLL_DISCOVER = "radio_scrollTop_discover";
const STORAGE_ORDER_BY = "radio_orderBy";
const STORAGE_DISCOVER_FILTERS = "radio_discoverFilters";

/** Discover search and filters, persisted so the tab reopens as the user left it. */
interface DiscoverFilters {
  searchQuery: string;
  country: string | null;
  language: string | null;
  codec: string | null;
  minBitrate: number | null;
}

function readDiscoverFilters(): DiscoverFilters {
  const empty: DiscoverFilters = {
    searchQuery: "",
    country: null,
    language: null,
    codec: null,
    minBitrate: null,
  };
  const raw = readStorage(STORAGE_DISCOVER_FILTERS);
  if (!raw) return empty;
  try {
    const parsed = JSON.parse(raw) as Partial<DiscoverFilters>;
    const str = (v: unknown) => (typeof v === "string" && v.trim() ? v : null);
    return {
      searchQuery: typeof parsed.searchQuery === "string" ? parsed.searchQuery : "",
      country: str(parsed.country),
      language: str(parsed.language),
      codec: str(parsed.codec),
      minBitrate:
        typeof parsed.minBitrate === "number" && parsed.minBitrate > 0 ? parsed.minBitrate : null,
    };
  } catch {
    return empty;
  }
}

const STALE_THRESHOLD_SECS = 24 * 3600;

/** Directory rows requested per global search; the backend clamps to its own cap too. */
const GLOBAL_SEARCH_DIRECTORY_LIMIT = 10;

function stationMatchesQuery(station: RadioDirectoryStation, tokens: string[]): boolean {
  const haystack =
    `${station.name} ${station.tags} ${station.country} ${station.language}`.toLowerCase();
  return tokens.every((t) => haystack.includes(t));
}

/**
 * Global-search station suggestions (#2007): saved stations matching every query
 * word first (client-side), then directory results that aren't already saved,
 * capped at `limit`.
 */
export function mergeStationSuggestions(
  saved: RadioDirectoryStation[],
  directory: RadioDirectoryStation[],
  query: string,
  limit: number,
): RadioDirectoryStation[] {
  const tokens = query
    .toLowerCase()
    .split(/[^\p{L}\p{N}]+/u)
    .filter(Boolean);
  if (tokens.length === 0 || limit <= 0) return [];
  const savedMatches = saved.filter((s) => stationMatchesQuery(s, tokens));
  const seen = new Set(saved.map((s) => s.stationuuid));
  const rest = directory.filter((s) => !seen.has(s.stationuuid));
  return [...savedMatches, ...rest].slice(0, limit);
}

function readStorage(key: string): string | null {
  if (typeof window === "undefined") return null;
  try {
    return localStorage.getItem(key);
  } catch {
    return null;
  }
}

function writeStorage(key: string, value: string) {
  if (typeof window === "undefined") return;
  try {
    localStorage.setItem(key, value);
  } catch {
    // Ignore storage quota / private mode errors.
  }
}

class RadioStore {
  private _subView = $state<RadioSubView>(
    readStorage(STORAGE_SUB_VIEW) === "my_stations" ? "my_stations" : "discover",
  );
  private _hideUnsupported = $state<boolean>(
    readStorage(STORAGE_HIDE_UNSUPPORTED) === "false" ? false : true,
  );
  private _orderBy = $state<RadioDirectoryOrder>(
    (readStorage(STORAGE_ORDER_BY) as RadioDirectoryOrder) || "clickcount",
  );

  status = $state<RadioDirectoryStatus | null>(null);
  syncProgress = $state<RadioDirectorySyncProgress | null>(null);
  syncing = $state(false);
  syncError = $state<string | null>(null);
  loadingStatus = $state(false);

  // Discover state
  private restoredFilters = readDiscoverFilters();
  searchQuery = $state(this.restoredFilters.searchQuery);
  selectedCountry = $state<string | null>(this.restoredFilters.country);
  selectedLanguage = $state<string | null>(this.restoredFilters.language);
  selectedCodec = $state<string | null>(this.restoredFilters.codec);
  selectedMinBitrate = $state<number | null>(this.restoredFilters.minBitrate);
  // Restored selections are checked once against the directory's facets.
  private restoredFiltersChecked = false;

  discoverStations = $state<RadioDirectoryStation[]>([]);
  discoverLoading = $state(false);
  discoverError = $state<string | null>(null);
  facets = $state<RadioDirectoryFacets>({
    tags: [],
    countries: [],
    languages: [],
    codecs: [],
  });

  // My stations state
  savedStations = $state<RadioDirectoryStation[]>([]);
  recentStations = $state<RadioDirectoryStation[]>([]);
  radioPlaylistIds = $state<number[]>([]);
  editingStation = $state<RadioDirectoryStation | null>(null);
  myStationsLoading = $state(false);
  stationListTags = $state<RadioBrowserFacetEntry[]>([]);
  suggestedLibraryGenres = $state<RadioBrowserFacetEntry[]>([]);
  stationListTagsLoading = $state(false);

  // Global search (TopNavigation) directory matches, see `searchStationsGlobal`.
  globalSearchStations = $state<RadioDirectoryStation[]>([]);
  private globalSearchSeq = 0;

  private savedUuids = $derived(new Set(this.savedStations.map((s) => s.stationuuid)));
  private unlisteners: UnlistenFn[] = [];
  private searchTimer: ReturnType<typeof setTimeout> | null = null;
  private initialized = false;
  private savedLoaded = false;

  get subView(): RadioSubView {
    return this._subView;
  }
  set subView(val: RadioSubView) {
    this._subView = val;
    writeStorage(STORAGE_SUB_VIEW, val);
  }

  get hideUnsupported(): boolean {
    return this._hideUnsupported;
  }
  set hideUnsupported(val: boolean) {
    this._hideUnsupported = val;
    writeStorage(STORAGE_HIDE_UNSUPPORTED, String(val));
    void this.refreshAll();
  }

  get orderBy(): RadioDirectoryOrder {
    return this._orderBy;
  }
  set orderBy(val: RadioDirectoryOrder) {
    this._orderBy = val;
    writeStorage(STORAGE_ORDER_BY, val);
    void this.loadDiscover();
  }

  getScrollTop(view: RadioSubView): number {
    const raw = readStorage(view === "my_stations" ? STORAGE_SCROLL_MY : STORAGE_SCROLL_DISCOVER);
    const parsed = raw ? Number(raw) : 0;
    return Number.isFinite(parsed) && parsed > 0 ? parsed : 0;
  }

  setScrollTop(view: RadioSubView, value: number) {
    writeStorage(
      view === "my_stations" ? STORAGE_SCROLL_MY : STORAGE_SCROLL_DISCOVER,
      String(Math.max(0, Math.round(value))),
    );
  }

  get isStale(): boolean {
    const last = this.status?.lastSyncedAt;
    if (!this.status?.optedIn || last == null) return false;
    return Math.floor(Date.now() / 1000) - last > STALE_THRESHOLD_SECS;
  }

  isSaved(stationuuid: string): boolean {
    return this.savedUuids.has(stationuuid);
  }

  /** True only while this station is actively playing (not paused or stopped). */
  isPlayingStation(stationuuid: string): boolean {
    return (
      playerStore.state === "playing" && playerStore.currentSong?.path === `radio://${stationuuid}`
    );
  }

  async init(): Promise<void> {
    if (!this.initialized) {
      this.initialized = true;
      try {
        const unlistenProgress = await listen<RadioDirectorySyncProgress>(
          "radio-directory-sync-progress",
          (event) => {
            this.syncProgress = event.payload;
            this.syncing = !event.payload.done;
          },
        );
        const unlistenChanged = await listen("radio-directory-changed", () => {
          void this.refreshStatusAndData();
        });
        this.unlisteners.push(unlistenProgress, unlistenChanged);
      } catch {
        // Non-Tauri test environments may not support event listeners.
      }
    }
    await this.refreshStatusAndData();
  }

  dispose() {
    if (this.searchTimer) {
      clearTimeout(this.searchTimer);
      this.searchTimer = null;
    }
    for (const unlisten of this.unlisteners) {
      unlisten();
    }
    this.unlisteners = [];
    this.initialized = false;
  }

  async refreshStatusAndData(): Promise<void> {
    this.loadingStatus = true;
    try {
      const status = await invoke<RadioDirectoryStatus>("get_radio_directory_status");
      this.status = status;
      this.syncing = status.syncInProgress;
      this.syncError = status.lastError ?? null;
      await this.loadMyStations();
      if (status.optedIn && status.stationCount > 0) {
        await this.dropMissingRestoredFilters();
        await Promise.all([this.loadFacets(), this.loadDiscover()]);
      }
    } catch (err) {
      this.syncError = err instanceof Error ? err.message : String(err);
    } finally {
      this.loadingStatus = false;
    }
  }

  /**
   * A restored country, language or format may have left the directory since the
   * last session (a re-sync dropped it). Clear such a selection rather than keep
   * filtering on a value the dropdown can no longer show.
   */
  private async dropMissingRestoredFilters(): Promise<void> {
    if (this.restoredFiltersChecked) return;
    this.restoredFiltersChecked = true;
    if (!this.selectedCountry && !this.selectedLanguage && !this.selectedCodec) return;
    try {
      const facets = await invoke<RadioDirectoryFacets>("get_radio_directory_facets", {
        hideUnsupported: this._hideUnsupported,
        tagLimit: 1,
      });
      if (!facets) return;
      const has = (list: { name: string; code?: string | null }[], value: string) =>
        list.some((e) => e.name === value || e.code === value);
      if (this.selectedCountry && !has(facets.countries, this.selectedCountry)) {
        this.selectedCountry = null;
      }
      if (this.selectedLanguage && !has(facets.languages, this.selectedLanguage)) {
        this.selectedLanguage = null;
      }
      if (this.selectedCodec && !has(facets.codecs, this.selectedCodec)) {
        this.selectedCodec = null;
      }
      this.persistFilters();
    } catch {
      // Keep the restored selection; the query still runs with it.
    }
  }

  private persistFilters() {
    const filters: DiscoverFilters = {
      searchQuery: this.searchQuery,
      country: this.selectedCountry,
      language: this.selectedLanguage,
      codec: this.selectedCodec,
      minBitrate: this.selectedMinBitrate,
    };
    writeStorage(STORAGE_DISCOVER_FILTERS, JSON.stringify(filters));
  }

  async refreshAll(): Promise<void> {
    await this.loadMyStations();
    if (this.status?.optedIn && this.status.stationCount > 0) {
      await Promise.all([this.loadFacets(), this.loadDiscover()]);
    }
  }

  async syncDirectory(): Promise<void> {
    this.syncing = true;
    this.syncError = null;
    this.syncProgress = { mode: "full", fetchedStations: 0, page: 1, done: false };
    try {
      await invoke<RadioDirectorySyncStats>("sync_radio_directory");
      await this.refreshStatusAndData();
    } catch (err) {
      const message = err instanceof Error ? err.message : String(err);
      this.syncError = message;
    } finally {
      this.syncing = false;
    }
  }

  private buildQuery() {
    return {
      search: this.searchQuery.trim() || null,
      tag: null,
      countrycode: this.selectedCountry,
      language: this.selectedLanguage,
      codec: this.selectedCodec,
      minBitrate: this.selectedMinBitrate,
      hideUnsupported: this._hideUnsupported,
      orderBy: this._orderBy,
      offset: 0,
      limit: 200,
    };
  }

  async loadFacets(): Promise<void> {
    try {
      const facets = await invoke<RadioDirectoryFacets>("get_radio_directory_facets", {
        hideUnsupported: this._hideUnsupported,
        tagLimit: 60,
        query: this.buildQuery(),
      });
      if (facets) this.facets = facets;
    } catch (err) {
      console.error("Failed to load radio facets:", err);
    }
  }

  async loadDiscover(): Promise<void> {
    this.discoverLoading = true;
    this.discoverError = null;
    try {
      const stations = await invoke<RadioDirectoryStation[]>("query_radio_directory", {
        query: this.buildQuery(),
      });
      this.discoverStations = stations ?? [];
    } catch (err) {
      this.discoverError = err instanceof Error ? err.message : String(err);
      this.discoverStations = [];
    } finally {
      this.discoverLoading = false;
    }
  }

  async loadMyStations(): Promise<void> {
    this.myStationsLoading = true;
    try {
      const [saved, recent, playlistIds] = await Promise.all([
        invoke<RadioDirectoryStation[]>("get_saved_radio_stations", {
          hideUnsupported: this._hideUnsupported,
        }),
        invoke<RadioDirectoryStation[]>("get_recent_radio_stations", {
          limit: 10,
          hideUnsupported: this._hideUnsupported,
        }),
        invoke<number[]>("get_radio_playlist_ids"),
      ]);
      this.savedStations = saved ?? [];
      this.recentStations = recent ?? [];
      this.radioPlaylistIds = playlistIds ?? [];
      this.savedLoaded = true;
    } catch (err) {
      console.error("Failed to load saved/recent radio stations:", err);
    } finally {
      this.myStationsLoading = false;
    }
  }

  /** Plays a station, or toggles pause/resume if it is already loaded. */
  async playStation(station: RadioDirectoryStation): Promise<void> {
    if (!station.playable) return;
    await this.playStationByUuid(station.stationuuid, station.name);
  }

  /**
   * Looks up directory stations for the global search box. Stale responses are
   * dropped; failures (or no directory) leave an empty list rather than an error.
   */
  async searchStationsGlobal(query: string): Promise<void> {
    const seq = ++this.globalSearchSeq;
    const q = query.trim();
    if (!q) {
      this.globalSearchStations = [];
      return;
    }
    void this.ensureSavedLoaded();
    const results = await invoke<RadioDirectoryStation[]>("search_stations", {
      query: q,
      limit: GLOBAL_SEARCH_DIRECTORY_LIMIT,
    }).catch(() => [] as RadioDirectoryStation[]);
    if (seq === this.globalSearchSeq) {
      this.globalSearchStations = Array.isArray(results) ? results : [];
    }
  }

  clearGlobalSearchStations() {
    this.globalSearchSeq++;
    this.globalSearchStations = [];
  }

  /** Plays a station known only by uuid (e.g. from a recent search), or toggles pause/resume if loaded. */
  async playStationByUuid(stationuuid: string, name: string): Promise<void> {
    const isLoaded =
      (playerStore.state === "playing" || playerStore.state === "paused") &&
      playerStore.currentSong?.path === `radio://${stationuuid}`;
    if (isLoaded) {
      await playerStore.togglePlayPause();
      return;
    }
    try {
      await invoke<Song>("play_radio_station", { stationuuid });
      void this.loadMyStations();
    } catch (err) {
      const msg = err instanceof Error ? err.message : String(err);
      toastStore.show(
        i18n.t("playerBar.stationOfflineToast", { title: name }, `"${name}" — ${msg}`),
        "error",
      );
    }
  }

  async ensureSavedLoaded(): Promise<void> {
    if (this.savedLoaded) return;
    await this.loadMyStations();
  }

  async toggleSaveByUuid(stationuuid: string): Promise<void> {
    const currentlySaved = this.isSaved(stationuuid);
    try {
      if (currentlySaved) {
        await invoke("unsave_radio_station", { stationuuid });
        this.savedStations = this.savedStations.filter((s) => s.stationuuid !== stationuuid);
      } else {
        await invoke("save_radio_station", { stationuuid });
        await this.loadMyStations();
      }
    } catch (err) {
      const msg = err instanceof Error ? err.message : String(err);
      toastStore.show(msg, "error");
    }
  }

  async toggleSaveStation(station: RadioDirectoryStation): Promise<void> {
    const currentlySaved = this.isSaved(station.stationuuid);
    try {
      if (currentlySaved) {
        await invoke("unsave_radio_station", { stationuuid: station.stationuuid });
        this.savedStations = this.savedStations.filter(
          (s) => s.stationuuid !== station.stationuuid,
        );
      } else {
        await invoke("save_radio_station", { stationuuid: station.stationuuid });
        this.savedStations = [station, ...this.savedStations];
      }
    } catch (err) {
      const msg = err instanceof Error ? err.message : String(err);
      toastStore.show(msg, "error");
    }
  }

  async reorderSavedStations(order: string[]): Promise<void> {
    const byUuid = new Map(this.savedStations.map((s) => [s.stationuuid, s]));
    const reordered: RadioDirectoryStation[] = [];
    const seen = new Set<string>();
    for (const uuid of order) {
      const station = byUuid.get(uuid);
      if (station && !seen.has(uuid)) {
        seen.add(uuid);
        reordered.push(station);
      }
    }
    for (const station of this.savedStations) {
      if (!seen.has(station.stationuuid)) {
        reordered.push(station);
      }
    }
    const previous = this.savedStations;
    this.savedStations = reordered;
    try {
      await invoke("reorder_saved_stations", { order: reordered.map((s) => s.stationuuid) });
    } catch (err) {
      this.savedStations = previous;
      const msg = err instanceof Error ? err.message : String(err);
      toastStore.show(msg, "error");
    }
  }

  async addCustomStation(name: string, url: string, tags: string): Promise<RadioDirectoryStation> {
    const station = await invoke<RadioDirectoryStation>("add_custom_radio_station", {
      name,
      url,
      tags,
    });
    const rest = this.savedStations.filter((s) => s.stationuuid !== station.stationuuid);
    this.savedStations = [station, ...rest];
    toastStore.show(i18n.t("radio.customStationAdded", { name: station.name }), "success");
    return station;
  }

  async updateCustomStation(
    stationuuid: string,
    name: string,
    url: string,
    tags: string,
  ): Promise<RadioDirectoryStation> {
    const station = await invoke<RadioDirectoryStation>("update_custom_radio_station", {
      stationuuid,
      name,
      url,
      tags,
    });
    this.savedStations = this.savedStations.map((s) =>
      s.stationuuid === station.stationuuid ? station : s,
    );
    this.recentStations = this.recentStations.map((s) =>
      s.stationuuid === station.stationuuid ? station : s,
    );
    toastStore.show(i18n.t("radio.customStationUpdated", { name: station.name }), "success");
    return station;
  }

  async importCustomStations(filePath: string): Promise<RadioDirectoryStation[]> {
    const added = await invoke<RadioDirectoryStation[]>("import_custom_radio_stations", {
      filePath,
    });
    await this.loadMyStations();
    toastStore.show(i18n.plural("radio.customStationsImported", added.length), "success");
    return added;
  }

  async exportSavedStations(exportPath: string): Promise<number> {
    const count = await invoke<number>("export_saved_radio_stations", { exportPath });
    toastStore.show(i18n.plural("radio.stationsExported", count), "success");
    return count;
  }

  private refreshDiscoverAndFacets() {
    this.persistFilters();
    void Promise.all([this.loadDiscover(), this.loadFacets()]);
  }

  setSearchQuery(query: string, immediate = false) {
    this.searchQuery = query;
    this.persistFilters();
    if (this.searchTimer) {
      clearTimeout(this.searchTimer);
      this.searchTimer = null;
    }
    if (immediate) {
      this.refreshDiscoverAndFacets();
      return;
    }
    this.searchTimer = setTimeout(() => {
      this.searchTimer = null;
      this.refreshDiscoverAndFacets();
    }, 180);
  }

  setMinBitrate(value: number | null) {
    this.selectedMinBitrate = value && value > 0 ? value : null;
    this.refreshDiscoverAndFacets();
  }

  setFilter(field: "country" | "language" | "codec", value: string | null) {
    if (field === "country") this.selectedCountry = value;
    else if (field === "language") this.selectedLanguage = value;
    else this.selectedCodec = value;
    this.refreshDiscoverAndFacets();
  }

  setCodecFilter(codec: string | null, hideUnsupported: boolean) {
    const hideChanged = this._hideUnsupported !== hideUnsupported;
    this.selectedCodec = codec;
    this._hideUnsupported = hideUnsupported;
    writeStorage(STORAGE_HIDE_UNSUPPORTED, String(hideUnsupported));
    this.persistFilters();
    if (hideChanged) {
      void this.refreshAll();
    } else {
      this.refreshDiscoverAndFacets();
    }
  }

  clearFilters() {
    this.searchQuery = "";
    this.selectedCountry = null;
    this.selectedLanguage = null;
    this.selectedCodec = null;
    this.selectedMinBitrate = null;
    this.refreshDiscoverAndFacets();
  }

  async loadStationListTags(): Promise<void> {
    this.stationListTagsLoading = true;
    try {
      const res = await invoke<RadioStationListTags>("get_radio_station_list_tags", {
        hideUnsupported: true,
        limit: 200,
      });
      this.stationListTags = res?.tags ?? [];
      this.suggestedLibraryGenres = res?.suggestedFromLibrary ?? [];
    } catch (err) {
      console.error("Failed to load station list tags:", err);
      this.stationListTags = [];
      this.suggestedLibraryGenres = [];
    } finally {
      this.stationListTagsLoading = false;
    }
  }

  async createStationList(name: string, spec: RadioStationListSpec): Promise<Playlist> {
    const playlist = await invoke<Playlist>("create_radio_station_list", { name, spec });
    await Promise.all([playlistsStore.refreshPlaylists(), this.loadMyStations()]);
    return playlist;
  }

  async updateStationList(
    playlistId: number,
    name: string,
    spec: RadioStationListSpec,
  ): Promise<Playlist> {
    const playlist = await invoke<Playlist>("update_radio_station_list", {
      playlistId,
      name,
      spec,
    });
    await Promise.all([playlistsStore.refreshPlaylists(), this.loadMyStations()]);
    return playlist;
  }

  async deleteStationList(playlistId: number): Promise<void> {
    await playlistsStore.deletePlaylist(playlistId);
    await this.loadMyStations();
  }
}

export const radioStore = new RadioStore();
