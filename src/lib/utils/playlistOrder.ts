/**
 * The one definition of how playlists are ordered in the UI.
 *
 * The Playlists view shows custom/smart playlists and system auto-playlists on separate tabs,
 * each with a user-chosen sort persisted in localStorage. Any other surface that lists playlists
 * (the Artist Detail "Playlists featuring" section) must order them the same way, so the rules
 * and the persisted preference live here rather than inside one view.
 */
import type { Playlist } from "../types";
import { getPlaylistDisplayName, isSystemAutoPlaylist } from "./playlist";

export type PlaylistSortField = "name" | "track_count" | "updated";
export type PlaylistSortGroup = "auto" | "custom";

export interface PlaylistSortPref {
  field: PlaylistSortField;
  asc: boolean;
}

// Fixed intensity order for the BPM auto-playlist bucket names (mirrors
// src-tauri/src/playlist/tempo_buckets.rs's TEMPO_BUCKETS) — alphabetical sort would
// scramble Down-Tempo/Mid-Tempo/Uptempo/High Energy/Extreme.
export const BPM_BUCKET_ORDER = [
  "Very Slow BPM",
  "Down-Tempo BPM",
  "Mid-Tempo BPM",
  "Uptempo BPM",
  "High Energy BPM",
  "Extreme BPM",
];

function sortStorageKeys(group: PlaylistSortGroup) {
  return {
    field: `sort_${group}_playlist_field`,
    asc: `sort_${group}_playlist_asc`,
  };
}

/** Reads the persisted sort for a group, defaulting to name ascending. */
export function loadPlaylistSortPref(group: PlaylistSortGroup): PlaylistSortPref {
  const keys = sortStorageKeys(group);
  try {
    const field = localStorage.getItem(keys.field) as PlaylistSortField | null;
    return {
      field: field ?? "name",
      asc: localStorage.getItem(keys.asc) !== "false",
    };
  } catch {
    return { field: "name", asc: true };
  }
}

export function savePlaylistSortPref(group: PlaylistSortGroup, pref: PlaylistSortPref): void {
  const keys = sortStorageKeys(group);
  try {
    localStorage.setItem(keys.field, pref.field);
    localStorage.setItem(keys.asc, pref.asc.toString());
  } catch {
    // Storage unavailable: the sort simply isn't remembered.
  }
}

/** Sorts a copy of `items` by name / song count / updated time in the given direction. */
export function sortByPlaylistField<T>(
  items: readonly T[],
  { field, asc }: PlaylistSortPref,
  get: (item: T) => { label: string; trackCount: number; updated: number },
): T[] {
  return [...items].sort((a, b) => {
    const x = get(a);
    const y = get(b);
    if (field === "name")
      return asc ? x.label.localeCompare(y.label) : y.label.localeCompare(x.label);
    const valA = field === "track_count" ? x.trackCount : x.updated;
    const valB = field === "track_count" ? y.trackCount : y.updated;
    return asc ? valA - valB : valB - valA;
  });
}

// Singleton diagnostic auto-playlists are pinned ahead of the sorted per-value ones, in this order.
const PINNED_AUTO_SPECS = ["daypart:", "missingmeta", "missingmbid"];

function pinnedRank(p: Playlist): number {
  return PINNED_AUTO_SPECS.findIndex((s) => p.dynamic_spec?.startsWith(s));
}

// The Playlists view sorts the Custom tab by the raw `name` and the Auto tab by the display name
// (which carries population-mode suffixes and locale-specific labels); mirror that exactly.
const byName = (p: Playlist) => ({ label: p.name, trackCount: p.track_count, updated: p.updated });
const byDisplayName = (p: Playlist) => ({
  label: getPlaylistDisplayName(p),
  trackCount: p.track_count,
  updated: p.updated,
});

/**
 * Orders playlists as the Playlists view does, for surfaces that mix both kinds in one list:
 * the user's own playlists first (Custom tab order), then system auto-playlists (Auto tab order:
 * pinned singletons, then the sorted per-value ones, BPM buckets last in intensity order).
 */
export function orderPlaylists(playlists: readonly Playlist[]): Playlist[] {
  const custom = playlists.filter((p) => !isSystemAutoPlaylist(p));
  const auto = playlists.filter(isSystemAutoPlaylist);
  const isBpm = (p: Playlist) => p.dynamic_spec?.startsWith("bpmrange:") === true;

  const pinned = auto
    .filter((p) => pinnedRank(p) >= 0)
    .sort((a, b) => pinnedRank(a) - pinnedRank(b));
  const rest = auto.filter((p) => pinnedRank(p) < 0 && !isBpm(p));
  const bpm = auto
    .filter(isBpm)
    .sort((a, b) => BPM_BUCKET_ORDER.indexOf(a.name) - BPM_BUCKET_ORDER.indexOf(b.name));

  return [
    ...sortByPlaylistField(custom, loadPlaylistSortPref("custom"), byName),
    ...pinned,
    ...sortByPlaylistField(rest, loadPlaylistSortPref("auto"), byDisplayName),
    ...bpm,
  ];
}
