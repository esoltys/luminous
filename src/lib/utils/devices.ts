import { listen } from "@tauri-apps/api/event";
import type { ConnectedDevice } from "../types";

/** The name a connected device is shown by: its nickname, else the name it reports. */
export function deviceName(device: ConnectedDevice): string {
  return device.profile.nickname || device.defaultName;
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
