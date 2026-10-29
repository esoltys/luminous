import { listen } from "@tauri-apps/api/event";
import type { ConnectedDevice, PlaylistSizeFit } from "../types";
import { i18n } from "../stores/i18n.svelte";
import { formatStorageSize } from "./formatters";

/** The backend's size-limit answer, only while `uuids` is the order it was
 * computed for; any other order would put the break in the wrong place. */
export function fitForOrder(fit: PlaylistSizeFit | null, uuids: string[]): PlaylistSizeFit | null {
  if (!fit || fit.order.length !== uuids.length) return null;
  return uuids.every((u, i) => u === fit.order[i]) ? fit : null;
}

/** The first of `shownUuids` past the limit — the row the break sits above —
 * or `null` when everything fits. Searching hides rows, not the break. */
export function breakBeforeUuid(fit: PlaylistSizeFit | null, shownUuids: Iterable<string>): string | null {
  if (!fit || fit.break_index >= fit.order.length) return null;
  const below = new Set(fit.order.slice(fit.break_index));
  for (const u of shownUuids) if (below.has(u)) return u;
  return null;
}

/** The break's label: the limit, how many songs above it fit, and how much is left below. */
export function sizeLimitBreakLabel(fit: PlaylistSizeFit): string {
  return i18n.plural("playlists.sizeLimitBreak", fit.fit_songs, {
    limit: formatStorageSize(fit.limit_bytes),
    below: formatStorageSize(fit.total_bytes - fit.fit_bytes),
  });
}

/** The header stat: how much of the limit the songs above the break use,
 * and on which device when the sizes are converted ones. */
export function sizeLimitStats(fit: PlaylistSizeFit): string {
  const values = { used: formatStorageSize(fit.fit_bytes), limit: formatStorageSize(fit.limit_bytes) };
  return fit.device
    ? i18n.t("playlists.sizeLimitStatsDevice", { ...values, device: fit.device.name })
    : i18n.t("playlists.sizeLimitStats", values);
}

export function deviceName(device: ConnectedDevice): string {
  return device.profile.nickname || device.defaultName;
}

/** Calls `onChange` when device `deviceId` connects, disconnects or has its
 * profile changed, which changes what its converted sizes are measured
 * with. Free-space updates, which arrive every few seconds during a copy,
 * don't count. Returns the unsubscribe function. */
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
