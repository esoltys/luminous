import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type {
  RadioDirectoryFacets,
  RadioDirectoryOrder,
  RadioDirectoryStation,
  RadioDirectoryStatus,
  RadioDirectorySyncProgress,
  RadioDirectorySyncStats,
  Song,
} from "../types";
import { playerStore } from "./player.svelte";
import { toastStore } from "./toast.svelte";
import { i18n } from "./i18n.svelte";

type RadioSubView = "my_stations" | "discover";

const STORAGE_SUB_VIEW = "radio_subView";
const STORAGE_HIDE_UNSUPPORTED = "radio_hideUnsupported";
const STORAGE_SCROLL_MY = "radio_scrollTop_my_stations";
const STORAGE_SCROLL_DISCOVER = "radio_scrollTop_discover";
const STORAGE_ORDER_BY = "radio_orderBy";

const STALE_THRESHOLD_SECS = 24 * 3600;

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
  searchQuery = $state("");
  selectedTag = $state<string | null>(null);
  selectedCountry = $state<string | null>(null);
  selectedLanguage = $state<string | null>(null);
  selectedCodec = $state<string | null>(null);

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
  myStationsLoading = $state(false);

  private savedUuids = $derived(new Set(this.savedStations.map((s) => s.stationuuid)));
  private unlisteners: UnlistenFn[] = [];
  private initialized = false;

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

  isPlayingStation(stationuuid: string): boolean {
    return playerStore.currentSong?.path === `radio://${stationuuid}`;
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
        await Promise.all([this.loadFacets(), this.loadDiscover()]);
      }
    } catch (err) {
      this.syncError = err instanceof Error ? err.message : String(err);
    } finally {
      this.loadingStatus = false;
    }
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

  async loadFacets(): Promise<void> {
    try {
      const facets = await invoke<RadioDirectoryFacets>("get_radio_directory_facets", {
        hideUnsupported: this._hideUnsupported,
        tagLimit: 60,
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
        query: {
          search: this.searchQuery.trim() || null,
          tag: this.selectedTag,
          countrycode: this.selectedCountry,
          language: this.selectedLanguage,
          codec: this.selectedCodec,
          hideUnsupported: this._hideUnsupported,
          orderBy: this._orderBy,
          offset: 0,
          limit: 200,
        },
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
      const [saved, recent] = await Promise.all([
        invoke<RadioDirectoryStation[]>("get_saved_radio_stations", {
          hideUnsupported: this._hideUnsupported,
        }),
        invoke<RadioDirectoryStation[]>("get_recent_radio_stations", {
          limit: 20,
          hideUnsupported: this._hideUnsupported,
        }),
      ]);
      this.savedStations = saved ?? [];
      this.recentStations = recent ?? [];
    } catch (err) {
      console.error("Failed to load saved/recent radio stations:", err);
    } finally {
      this.myStationsLoading = false;
    }
  }

  async playStation(station: RadioDirectoryStation): Promise<void> {
    if (!station.playable) return;
    try {
      await invoke<Song>("play_radio_station", { stationuuid: station.stationuuid });
      void this.loadMyStations();
    } catch (err) {
      const msg = err instanceof Error ? err.message : String(err);
      toastStore.show(
        i18n.t(
          "playerBar.stationOfflineToast",
          { title: station.name },
          `"${station.name}" — ${msg}`,
        ),
        "error",
      );
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

  clearFilters() {
    this.searchQuery = "";
    this.selectedTag = null;
    this.selectedCountry = null;
    this.selectedLanguage = null;
    this.selectedCodec = null;
    void this.loadDiscover();
  }
}

export const radioStore = new RadioStore();
