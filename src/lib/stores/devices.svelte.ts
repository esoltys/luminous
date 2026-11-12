import { invoke } from "@tauri-apps/api/core";
import type {
  ConnectedDevice,
  DeviceSyncState,
  SyncPlan,
  SyncProgress,
  SyncSettings,
  SyncSummary,
  SyncSource,
} from "../types";
import { deviceName } from "../utils/devices";
import { createEventScope } from "../utils/eventScope";
import { i18n } from "./i18n.svelte";
import { toastStore } from "./toast.svelte";

/**
 * Connected portable players and their syncs (#839): the device list
 * (`devices-changed`), each device's sync state and the progress of running
 * syncs (`device-sync-progress`, `device-sync-finished`). Whether a sync is
 * running is only ever learned from the backend, never assumed after a call.
 */
class DevicesStore {
  devices = $state<ConnectedDevice[]>([]);
  /** True once the device list has been read. */
  loaded = $state(false);
  /** Sync state by device id; absent until read. Its selection is the saved one: {@link planSync} is the only way to change it. */
  syncStates = $state<Record<string, DeviceSyncState>>({});
  /** Progress of the running sync by device id; absent before its first report. */
  progress = $state<Record<string, SyncProgress>>({});
  /** The progress drawer is hidden; the sidebar row keeps showing progress. */
  drawerHidden = $state(false);
  private events = createEventScope(import.meta.hot);

  constructor() {
    this.init();
  }

  private async init() {
    try {
      await this.events.listen<ConnectedDevice[]>("devices-changed", (event) =>
        this.setDevices(event.payload),
      );
      await this.events.listen<SyncProgress>("device-sync-progress", (event) => {
        const progress = event.payload;
        this.progress[progress.deviceId] = progress;
        const state = this.syncStates[progress.deviceId];
        if (state && !state.running)
          this.syncStates[progress.deviceId] = { ...state, running: true };
      });
      await this.events.listen<SyncSummary>("device-sync-finished", (event) =>
        this.finished(event.payload),
      );
      const devices = await invoke<ConnectedDevice[]>("get_connected_devices");
      this.setDevices(Array.isArray(devices) ? devices : []);
      this.loaded = true;
    } catch (err) {
      console.error("Failed to initialize DevicesStore:", err);
    }
  }

  private setDevices(devices: ConnectedDevice[]) {
    const known = new Set(this.devices.map((d) => d.id));
    this.devices = devices;
    for (const device of devices) {
      if (!known.has(device.id)) this.refreshSyncState(device.id);
    }
  }

  /** Reads a device's sync state again: its selection, last sync and whether one is running. */
  async refreshSyncState(deviceId: string): Promise<DeviceSyncState | null> {
    try {
      const state = await invoke<DeviceSyncState>("get_device_sync_state", { deviceId });
      if (state) this.syncStates[deviceId] = state;
      return state ?? null;
    } catch (err) {
      console.error("Failed to read device sync state:", err);
      return null;
    }
  }

  /**
   * Saves `settings` as the device's sync selection and plans syncing it.
   * Every change to a selection goes through here, so `syncStates` never
   * holds an older one that a later plan would save back over it.
   */
  async planSync(deviceId: string, settings: SyncSettings): Promise<SyncPlan> {
    const plan = await invoke<SyncPlan>("preview_device_sync", { deviceId, settings });
    const state = this.syncStates[deviceId];
    if (state) this.syncStates[deviceId] = { ...state, settings };
    else await this.refreshSyncState(deviceId);
    return plan;
  }

  /**
   * Identifies a source across syncs for comparison.
   */
  sourceKey(s: SyncSource): string {
    switch (s.kind) {
      case "library":
        return "library";
      case "playlist":
        return `playlist:${s.id}`;
      case "autoPlaylist":
        return `autoPlaylist:${s.autoType}`;
      case "song":
        return `song:${s.id}`;
      case "artist":
        return `artist:${s.name.toLowerCase()}`;
      case "album":
        return `album:${s.album.toLowerCase()}\u{1f}${(s.artist ?? "").toLowerCase()}`;
      case "folder":
        return `folder:${s.path.toLowerCase()}`;
    }
  }

  /**
   * Checks whether `source` is already included in `sources`.
   */
  hasSource(sources: SyncSource[], source: SyncSource): boolean {
    const targetKey = this.sourceKey(source);
    return sources.some((s) => this.sourceKey(s) === targetKey);
  }

  /**
   * Checks whether `source` is currently selected in `deviceId`'s sync settings.
   */
  isSourceInSync(deviceId: string, source: SyncSource): boolean {
    const state = this.syncStates[deviceId];
    if (!state) return false;
    return this.hasSource(state.settings.sources, source);
  }

  /**
   * Checks whether all songs in `songIds` are currently selected in `deviceId`'s sync settings.
   */
  areSongsInSync(deviceId: string, songIds: number[]): boolean {
    const state = this.syncStates[deviceId];
    if (!state || songIds.length === 0) return false;
    const existing = new Set(
      state.settings.sources
        .filter((s): s is { kind: "song"; id: number } => s.kind === "song")
        .map((s) => s.id),
    );
    return songIds.every((id) => existing.has(id));
  }

  /**
   * Adds `source` (playlist, auto-playlist, album, artist, song, or folder)
   * to the device's sync selection. False when it is already there.
   * Rejects when the device can't be planned.
   */
  async addSourceToSync(deviceId: string, source: SyncSource): Promise<boolean> {
    const state = await invoke<DeviceSyncState>("get_device_sync_state", { deviceId });
    if (this.hasSource(state.settings.sources, source)) return false;
    await this.planSync(deviceId, {
      ...state.settings,
      sources: [...state.settings.sources, source],
    });
    return true;
  }

  /**
   * Removes `source` from the device's sync selection. False when it wasn't there.
   */
  async removeSourceFromSync(deviceId: string, source: SyncSource): Promise<boolean> {
    const state = await invoke<DeviceSyncState>("get_device_sync_state", { deviceId });
    const targetKey = this.sourceKey(source);
    const filtered = state.settings.sources.filter((s) => this.sourceKey(s) !== targetKey);
    if (filtered.length === state.settings.sources.length) return false;
    await this.planSync(deviceId, {
      ...state.settings,
      sources: filtered,
    });
    return true;
  }

  /**
   * Adds `source` to `device`'s sync and displays a toast confirming addition,
   * reporting when it is already syncing, or reporting failure.
   */
  async addToDeviceSync(
    device: ConnectedDevice,
    source: SyncSource,
    name: string,
  ): Promise<boolean> {
    const deviceLabel = deviceName(device);
    try {
      if (!(await this.addSourceToSync(device.id, source))) {
        toastStore.show(i18n.t("devices.itemAlreadyInSync", { name, device: deviceLabel }), "info");
        return false;
      }
      toastStore.show(i18n.t("devices.addedItemToSync", { name, device: deviceLabel }), "success");
      return true;
    } catch (err) {
      console.error("Failed to add source to device sync:", err);
      toastStore.show(
        i18n.t("devices.addItemToSyncFailed", { name, device: deviceLabel }),
        "error",
      );
      return false;
    }
  }

  /**
   * Removes `source` from `device`'s sync and displays a toast confirming removal.
   */
  async removeFromDeviceSync(
    device: ConnectedDevice,
    source: SyncSource,
    name: string,
  ): Promise<boolean> {
    const deviceLabel = deviceName(device);
    try {
      if (!(await this.removeSourceFromSync(device.id, source))) {
        return false;
      }
      toastStore.show(
        i18n.t("devices.removedItemFromSync", { name, device: deviceLabel }),
        "success",
      );
      return true;
    } catch (err) {
      console.error("Failed to remove source from device sync:", err);
      toastStore.show(
        i18n.t("devices.removeItemFromSyncFailed", { name, device: deviceLabel }),
        "error",
      );
      return false;
    }
  }

  /**
   * Adds the folder at `path` to the device's sync selection. False when it
   * is already there. Rejects when the device can't be planned.
   */
  async addFolderToSync(deviceId: string, path: string): Promise<boolean> {
    return this.addSourceToSync(deviceId, { kind: "folder", path });
  }

  /**
   * Adds songs to the device's sync selection. Filters out songs that are already selected.
   */
  async addSongsToSync(
    deviceOrId: ConnectedDevice | string,
    songIds: number[],
    name?: string,
  ): Promise<boolean> {
    const deviceId = typeof deviceOrId === "string" ? deviceOrId : deviceOrId.id;
    const device = typeof deviceOrId === "string" ? this.device(deviceId) : deviceOrId;
    const deviceLabel = device ? deviceName(device) : "";
    const itemName = name ?? i18n.plural("playlists.songsCount", songIds.length);

    const state = await invoke<DeviceSyncState>("get_device_sync_state", { deviceId });
    const existing = new Set(
      state.settings.sources
        .filter((s): s is { kind: "song"; id: number } => s.kind === "song")
        .map((s) => s.id),
    );
    const newSources: SyncSource[] = songIds
      .filter((id) => !existing.has(id))
      .map((id) => ({ kind: "song", id }));

    if (newSources.length === 0) {
      toastStore.show(
        i18n.t("devices.itemAlreadyInSync", { name: itemName, device: deviceLabel }),
        "info",
      );
      return false;
    }

    await this.planSync(deviceId, {
      ...state.settings,
      sources: [...state.settings.sources, ...newSources],
    });
    toastStore.show(
      i18n.plural("devices.addedSongsToSync", newSources.length, {
        count: newSources.length,
        device: deviceLabel,
      }),
      "success",
    );
    return true;
  }

  /**
   * Removes songs from the device's sync selection.
   */
  async removeSongsFromSync(
    deviceOrId: ConnectedDevice | string,
    songIds: number[],
    name?: string,
  ): Promise<boolean> {
    const deviceId = typeof deviceOrId === "string" ? deviceOrId : deviceOrId.id;
    const device = typeof deviceOrId === "string" ? this.device(deviceId) : deviceOrId;
    const deviceLabel = device ? deviceName(device) : "";

    const state = await invoke<DeviceSyncState>("get_device_sync_state", { deviceId });
    const toRemove = new Set(songIds);
    const filtered = state.settings.sources.filter(
      (s) => !(s.kind === "song" && toRemove.has(s.id)),
    );
    const removedCount = state.settings.sources.length - filtered.length;
    if (removedCount === 0) return false;

    await this.planSync(deviceId, {
      ...state.settings,
      sources: filtered,
    });
    toastStore.show(
      i18n.plural("devices.removedSongsFromSync", removedCount, {
        count: removedCount,
        device: deviceLabel,
      }),
      "success",
    );
    return true;
  }

  /**
   * Toggles the sync state of a source or songs on `device`.
   * Removes if currently in sync, otherwise adds.
   */
  async toggleDeviceSync(
    device: ConnectedDevice,
    source?: SyncSource,
    sourceName?: string,
    songIds?: number[],
  ): Promise<boolean> {
    if (songIds && songIds.length > 0) {
      if (this.areSongsInSync(device.id, songIds)) {
        return this.removeSongsFromSync(device, songIds, sourceName);
      } else {
        return this.addSongsToSync(device, songIds, sourceName);
      }
    } else if (source) {
      if (this.isSourceInSync(device.id, source)) {
        return this.removeFromDeviceSync(device, source, sourceName ?? "");
      } else {
        return this.addToDeviceSync(device, source, sourceName ?? "");
      }
    }
    return false;
  }

  private finished(summary: SyncSummary) {
    delete this.progress[summary.deviceId];
    this.refreshSyncState(summary.deviceId);
    const device = this.device(summary.deviceId);
    const name = device ? deviceName(device) : "";
    // Stays until dismissed, like every toast that isn't given a duration.
    switch (summary.outcome.kind) {
      case "completed":
        toastStore.show(
          summary.failures.length > 0
            ? i18n.plural("devices.syncDoneWithFailures", summary.failures.length, { device: name })
            : i18n.t("devices.syncDone", { device: name }),
          summary.failures.length > 0 ? "warning" : "success",
        );
        break;
      case "stopped":
        toastStore.show(
          i18n.plural("devices.syncStopped", summary.songsWritten, { device: name }),
          "info",
        );
        break;
      case "failed":
        toastStore.show(i18n.t("devices.syncFailed", { device: name }), "error");
        break;
    }
  }

  device(id: string): ConnectedDevice | undefined {
    return this.devices.find((d) => d.id === id);
  }

  /** Whether a sync to the device is running, as the backend last said. */
  isSyncing(id: string): boolean {
    return !!this.syncStates[id]?.running || !!this.progress[id];
  }

  /** The device whose sync is running, if any (the drawer shows the first). */
  get syncingId(): string | null {
    return this.devices.find((d) => this.isSyncing(d.id))?.id ?? null;
  }

  /** Safely removes the device; a running sync to it is stopped first. Says why it can't when it can't. */
  async eject(id: string) {
    const device = this.device(id);
    const name = device ? deviceName(device) : "";
    try {
      await invoke("eject_device", { id });
      toastStore.show(i18n.t("devices.ejected", { device: name }), "success", 4000);
    } catch (err) {
      const code = String(err);
      toastStore.show(
        i18n.t(code === "device-in-use" ? "devices.ejectInUse" : "devices.ejectFailed", {
          device: name,
        }),
        "error",
      );
    }
  }

  /** Brings the progress drawer back after it was hidden. */
  showProgress() {
    this.drawerHidden = false;
  }

  /** Asks the device's sync to stop; `device-sync-finished` follows. */
  async stopSync(id: string) {
    try {
      await invoke("cancel_device_sync", { deviceId: id });
    } catch (err) {
      console.error("Failed to stop device sync:", err);
    }
  }

  /** Stops listening to the backend. */
  dispose() {
    this.events.dispose();
  }
}

export const devicesStore = new DevicesStore();
