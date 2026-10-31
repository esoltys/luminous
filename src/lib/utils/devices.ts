import { listen } from "@tauri-apps/api/event";
import type { ConnectedDevice, DeviceStorage, SyncSettings, TargetFormat } from "../types";

/** The name a connected device is shown by: its nickname, else the name it reports. */
export function deviceName(device: ConnectedDevice): string {
  return device.profile.nickname || device.defaultName;
}

/** What a conversion format is called ("AAC"). */
export const FORMAT_LABELS: Record<TargetFormat, string> = { aac: "AAC", mp3: "MP3", opus: "Opus" };

/** A conversion as sync shows it: "AAC 256". */
export function conversionLabel(format: TargetFormat, bitrateKbps: number): string {
  return `${FORMAT_LABELS[format]} ${bitrateKbps}`;
}

/** The device's storage `storageId` names, else its first (as the backend resolves an absent or stale id). */
export function deviceStorage(device: ConnectedDevice, storageId: string | null | undefined): DeviceStorage | undefined {
  return device.storages.find((s) => s.id === storageId) ?? device.storages[0];
}

/** The storage a device's sync writes to. */
export function syncStorage(device: ConnectedDevice, settings: SyncSettings | undefined): DeviceStorage | undefined {
  return deviceStorage(device, settings?.storageId);
}

/** What a storage is called: its label, else "Storage 2" by position. */
export function storageName(device: ConnectedDevice, storage: DeviceStorage, unnamed: (n: number) => string): string {
  return storage.label?.trim() || unnamed(device.storages.indexOf(storage) + 1);
}

/** Calls `onChange` when device `deviceId` connects, disconnects or has its
 * profile changed. Free-space updates, which arrive every few seconds during
 * a copy, don't count. Returns the unsubscribe function. */
export function watchDeviceProfile(deviceId: string, onChange: () => void): () => void {
  let last: string | undefined;
  let disposed = false;
  let unlisten: (() => void) | undefined;
  listen<ConnectedDevice[]>("devices-changed", (event) => {
    const device = event.payload.find((d) => d.id === deviceId);
    const key = device ? JSON.stringify(device.profile) : "";
    if (last !== undefined && key !== last) onChange();
    last = key;
  }).then((fn) => {
    if (disposed) fn();
    else unlisten = fn;
  });
  return () => {
    disposed = true;
    unlisten?.();
  };
}
