import type { PlaylistSizeFit } from "../types";
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

/** The header stat: how much of the limit the songs above the break use. */
export function sizeLimitStats(fit: PlaylistSizeFit): string {
  return i18n.t("playlists.sizeLimitStats", {
    used: formatStorageSize(fit.fit_bytes),
    limit: formatStorageSize(fit.limit_bytes),
  });
}
