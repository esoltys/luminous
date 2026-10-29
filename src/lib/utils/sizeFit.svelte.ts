import { invoke } from "@tauri-apps/api/core";
import type { Playlist, PlaylistItem, PlaylistSizeFit } from "../types";
import { i18n } from "../stores/i18n.svelte";
import { formatStorageSize } from "./formatters";
import { watchDeviceProfile } from "./devices";

/** What a view lists: the playlist whose own rows they are (`undefined` when
 * they aren't one playlist's rows, e.g. the Queue or Favourites) and those
 * rows in the order the backend returned them (`null` while not loaded). */
export interface SizeLimitSource {
  playlist: Playlist | undefined;
  items: PlaylistItem[] | null;
}

/** The backend's answer to which of a playlist's songs fit its size limit,
 * for one view. Create it while a component initialises: it measures
 * whenever the rows, the limit or its device change, and again when that
 * device is plugged in, unplugged or re-profiled. */
export class PlaylistSizeLimit {
  #source: () => SizeLimitSource;
  #answer = $state<PlaylistSizeFit | null>(null);
  #requests = 0;

  constructor(source: () => SizeLimitSource) {
    this.#source = source;
    $effect(() => {
      const { playlist, items } = source();
      if (!playlist?.size_limit_bytes || items === null) {
        this.#requests++; // drop any answer still on its way
        this.#answer = null;
        return;
      }
      const { id, size_limit_device: device } = playlist;
      this.#measure(id);
      return device ? watchDeviceProfile(device, () => this.#measure(id)) : undefined;
    });
  }

  /** The answer, only while it answers what is listed now: the same limit,
   * device and row order. Otherwise `null` (e.g. right after an optimistic
   * drag, or while a new device's sizes are measured). */
  readonly fit = $derived.by((): PlaylistSizeFit | null => {
    const { playlist, items } = this.#source();
    const fit = this.#answer;
    if (!fit || !playlist || !items) return null;
    if (fit.limit_bytes !== playlist.size_limit_bytes) return null;
    if (fit.device_id !== (playlist.size_limit_device ?? null)) return null;
    if (fit.order.length !== items.length || items.some((item, i) => item.uuid !== fit.order[i])) return null;
    return fit;
  });

  /** The header stat: how much of the limit is used (and on which device),
   * "Measuring sizes…" until that's known, `null` with no limit. */
  readonly status = $derived.by((): string | null => {
    if (this.fit) return sizeLimitStats(this.fit);
    return this.#source().playlist?.size_limit_bytes ? i18n.t("playlists.sizeLimitMeasuring") : null;
  });

  /** The limit device's name from the latest answer, for showing it while unplugged. */
  readonly deviceName = $derived.by((): string | null => {
    const device = this.#source().playlist?.size_limit_device;
    return device && this.#answer?.device_id === device ? (this.#answer.device_name ?? null) : null;
  });

  /** The first of `shownUuids` past the limit — the row the break sits above —
   * or `null` when everything fits. Searching hides rows, not the break. */
  breakBefore(shownUuids: Iterable<string>): string | null {
    const fit = this.fit;
    if (!fit || fit.break_index >= fit.order.length) return null;
    const below = new Set(fit.order.slice(fit.break_index));
    for (const u of shownUuids) if (below.has(u)) return u;
    return null;
  }

  /** The break's label: the limit, how many songs above it fit, and how much is left below. */
  breakLabel(): string | null {
    const fit = this.fit;
    if (!fit) return null;
    return i18n.plural("playlists.sizeLimitBreak", fit.fit_songs, {
      limit: formatStorageSize(fit.limit_bytes),
      below: formatStorageSize(fit.total_bytes - fit.fit_bytes),
    });
  }

  /** Only the latest request's answer is kept: a slow measurement for a
   * device must not land over a newer one. */
  #measure(playlistId: number) {
    const request = ++this.#requests;
    invoke<PlaylistSizeFit | null>("get_playlist_size_fit", { playlistId })
      .then((fit) => {
        if (request === this.#requests) this.#answer = fit ?? null;
      })
      .catch((e) => console.error("Failed to measure the playlist's size limit:", e));
  }
}

function sizeLimitStats(fit: PlaylistSizeFit): string {
  const values = { used: formatStorageSize(fit.fit_bytes), limit: formatStorageSize(fit.limit_bytes) };
  return fit.device_name
    ? i18n.t("playlists.sizeLimitStatsDevice", { ...values, device: fit.device_name })
    : i18n.t("playlists.sizeLimitStats", values);
}
