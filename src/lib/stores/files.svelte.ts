import { invoke } from "@tauri-apps/api/core";
import { createEventScope } from "../utils/eventScope";
import type { ConnectedDevice, FileSource, FileSourceSummary, ScanProgress } from "../types";
import { devicesStore } from "./devices.svelte";
import { deviceName } from "../utils/devices";
import { collectionStore } from "./collection.svelte";
import { navigationStore } from "./navigation.svelte";

/** Whether `a` and `b` are the same source. Storages of one device count as one source. */
export function sameSource(a: FileSource, b: FileSource): boolean {
  return a.kind === b.kind && a.id === b.id;
}

/** A key unique to `source`, for keyed lists. */
export function sourceKey(source: FileSource): string {
  return `${source.kind}:${source.id}`;
}

/**
 * The sources the Files view browses (#1841): watched folders and media
 * servers with their song counts, plus the connected devices (owned by
 * `devicesStore`). Kept current from backend events; the sidebar and the
 * Files view only read it.
 */
class FilesStore {
  /** Folders, then Subsonic, then WebDAV servers, as the backend orders them. */
  sources = $state<FileSourceSummary[]>([]);
  /** True once the folders and servers have been read. */
  loaded = $state(false);
  /** Instant filter query for the folder currently open in the Files view (#1886). */
  filterQuery = $state("");
  /** Number of entries matching `filterQuery` in the currently open folder. */
  filterMatchCount = $state(0);
  /** Total entries in the currently open folder. */
  filterTotalCount = $state(0);
  /** Display name of the currently open folder, for the search placeholder. */
  activeFolderTitle = $state("");
  private lastLocationKey = "";
  get devices(): ConnectedDevice[] {
    return devicesStore.devices;
  }
  private refreshDebounce: ReturnType<typeof setTimeout> | undefined;
  private events = createEventScope(import.meta.hot);

  /**
   * True when the Files view is showing an open folder (including a device's
   * Files tab, but not the Files root overview or a device's Sync tab), so
   * the TopNavigation search box filters that folder (#1886).
   */
  get isFilteringFolder(): boolean {
    const loc = navigationStore.filesLocation;
    if (navigationStore.activeTab !== "files" || !loc) return false;
    if (loc.source.kind === "device" && loc.deviceTab === "sync") return false;
    return true;
  }

  clearFilter() {
    this.filterQuery = "";
  }

  constructor() {
    // Settings > Sources reloads these lists after every add, edit and
    // removal, so following them keeps names and the list itself current.
    $effect.root(() => {
      $effect(() => {
        void collectionStore.directories;
        void collectionStore.webdavServers;
        void collectionStore.subsonicServers;
        if (this.loaded) this.scheduleRefresh();
      });
      $effect(() => {
        void devicesStore.devices;
        this.leaveGoneSource();
      });
      $effect(() => {
        const loc = navigationStore.filesLocation;
        const key = loc
          ? `${sourceKey(loc.source)}:${loc.source.kind === "device" ? (loc.source.storage ?? "") : ""}:${JSON.stringify(loc.path)}`
          : "";
        if (key !== this.lastLocationKey) {
          this.lastLocationKey = key;
          this.filterQuery = "";
        }
      });
    });
    this.init();
  }

  private async init() {
    try {
      await this.events.listen("library-changed", () => this.scheduleRefresh());
      await this.events.listen<ScanProgress>("scan-progress", (event) => {
        if (event.payload.phase === "done") this.scheduleRefresh();
      });
      await this.refresh();
      this.loaded = true;
      this.leaveGoneSource();
    } catch (err) {
      console.error("Failed to initialize FilesStore:", err);
    }
  }

  private scheduleRefresh() {
    clearTimeout(this.refreshDebounce);
    this.refreshDebounce = setTimeout(() => {
      this.refresh().catch((err) => console.error("Failed to refresh file sources:", err));
    }, 300);
  }

  async refresh() {
    const sources = await invoke<FileSourceSummary[]>("list_file_sources");
    this.sources = Array.isArray(sources) ? sources : [];
    this.leaveGoneSource();
  }

  /**
   * Sends the Files view back to its overview when the source it shows was
   * removed or unplugged. Waits until both lists are in, since a location
   * restored at startup names a source that may not be listed yet.
   */
  private leaveGoneSource() {
    const location = navigationStore.filesLocation;
    const listed = this.loaded && devicesStore.loaded;
    if (listed && location && !this.exists(location.source)) navigationStore.filesLocation = null;
  }

  /** Whether `source` is still there: a listed folder or server, or a connected device. */
  exists(source: FileSource): boolean {
    if (source.kind === "device") return this.devices.some((d) => d.id === source.id);
    return this.sources.some((s) => sameSource(s.source, source));
  }

  /** What `source` is called, or "" when it's gone. */
  name(source: FileSource): string {
    if (source.kind === "device") {
      const device = this.devices.find((d) => d.id === source.id);
      return device ? deviceName(device) : "";
    }
    return this.sources.find((s) => sameSource(s.source, source))?.name ?? "";
  }

  /** Stops listening to backend events. */
  dispose() {
    this.events.dispose();
  }
}

export const filesStore = new FilesStore();
