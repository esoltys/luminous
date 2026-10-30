import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type { ConnectedDevice, FileSource, FileSourceSummary, ScanProgress } from "../types";
import { deviceName } from "../utils/devices";
import { collectionStore } from "./collection.svelte";
import { navigationStore } from "./navigation.svelte";

/** Whether `a` and `b` are the same source. */
export function sameSource(a: FileSource, b: FileSource): boolean {
  return a.kind === b.kind && a.id === b.id;
}

/** A key unique to `source`, for keyed lists. */
export function sourceKey(source: FileSource): string {
  return `${source.kind}:${source.id}`;
}

/**
 * The sources the Files view browses (#1841): watched folders and media
 * servers with their song counts, plus the connected devices. Kept current
 * from backend events; the sidebar and the Files view only read it.
 */
class FilesStore {
  /** Folders, then Subsonic, then WebDAV servers, as the backend orders them. */
  sources = $state<FileSourceSummary[]>([]);
  devices = $state<ConnectedDevice[]>([]);
  /** True once both lists have been read, so a missing source means it's gone. */
  loaded = $state(false);
  private refreshDebounce: ReturnType<typeof setTimeout> | undefined;

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
    });
    this.init();
  }

  private async init() {
    try {
      await listen("library-changed", () => this.scheduleRefresh());
      await listen<ScanProgress>("scan-progress", (event) => {
        if (event.payload.phase === "done") this.scheduleRefresh();
      });
      await listen<ConnectedDevice[]>("devices-changed", (event) => {
        this.devices = event.payload;
        this.leaveGoneSource();
      });
      const [, devices] = await Promise.all([
        this.refresh(),
        invoke<ConnectedDevice[]>("get_connected_devices"),
      ]);
      this.devices = Array.isArray(devices) ? devices : [];
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
    if (this.loaded && location && !this.exists(location.source)) navigationStore.filesLocation = null;
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
}

export const filesStore = new FilesStore();
