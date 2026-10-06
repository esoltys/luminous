import { i18n } from "../stores/i18n.svelte";
import type { WeekStart } from "../stores/prefs.svelte";
import { formatDate } from "./formatters";

function diffDaysFromNow(timestampSec: number): number {
  const now = new Date();
  const date = new Date(timestampSec * 1000);

  const nowStart = new Date(now.getFullYear(), now.getMonth(), now.getDate());
  const dateStart = new Date(date.getFullYear(), date.getMonth(), date.getDate());

  const diffMs = nowStart.getTime() - dateStart.getTime();
  return Math.floor(diffMs / (1000 * 60 * 60 * 24));
}

// Anything this recent reads as minutes/hours ago even across midnight, so a song
// played at 23:50 shows "15 minutes ago" at 00:05 rather than "Yesterday" (#1299).
const ELAPSED_TIME_WINDOW_MINUTES = 6 * 60;

// Minutes/hours ago for today's (or the last few hours') entries, then Yesterday /
// "N days ago" through 6 days, then falls back to the absolute date — used for the
// Date Added and Last Played columns, where older entries read better as a real
// date than as "3 weeks ago".
export function formatDateAdded(timestampSec: number | undefined | null): string {
  if (!timestampSec) return "—";
  const diffDays = diffDaysFromNow(timestampSec);
  const diffMinutes = Math.floor((Date.now() / 1000 - timestampSec) / 60);

  if (diffDays <= 0 || diffMinutes < ELAPSED_TIME_WINDOW_MINUTES) {
    if (diffMinutes < 1) return i18n.t("playlists.relativeJustNow");
    if (diffMinutes < 60) {
      return i18n.plural("playlists.relativeMinutesAgo", diffMinutes);
    }
    const diffHours = Math.floor(diffMinutes / 60);
    return i18n.plural("playlists.relativeHoursAgo", diffHours);
  }
  if (diffDays === 1) return i18n.t("playlists.relativeYesterday");
  if (diffDays <= 6) return i18n.plural("playlists.relativeDaysAgo", diffDays);
  return formatDate(timestampSec);
}

export function formatRelativeDate(timestampSec: number | undefined | null): string {
  if (!timestampSec) return "";
  const diffDays = diffDaysFromNow(timestampSec);

  if (diffDays <= 0) return i18n.t("playlists.relativeToday");
  if (diffDays === 1) return i18n.t("playlists.relativeYesterday");
  if (diffDays < 7) return i18n.plural("playlists.relativeDaysAgo", diffDays);
  if (diffDays < 30) {
    const weeks = Math.floor(diffDays / 7);
    return i18n.plural("playlists.relativeWeeksAgo", weeks);
  }
  if (diffDays < 365) {
    const months = Math.floor(diffDays / 30);
    return i18n.plural("playlists.relativeMonthsAgo", months);
  }
  const years = Math.floor(diffDays / 365);
  return i18n.plural("playlists.relativeYearsAgo", years);
}

/** Formats a chart week (as computed by the backend's `chart_week`, #662) as a
 * compact date range, e.g. "Sep 21-27" or "Sep 27-Oct 3" when crossing a
 * monthly boundary. The backend already resolved the week on the user's local
 * calendar and encodes its first date as that date's UTC midnight, so this
 * renders with `timeZone: "UTC"` to read the date back unshifted. */
export function formatWeekRange(periodStartSec: number, locale: string): string {
  const start = new Date(periodStartSec * 1000);
  const end = new Date((periodStartSec + 6 * 86_400) * 1000);
  const fmtMonth = new Intl.DateTimeFormat(locale, { month: "short", timeZone: "UTC" });
  const startMonth = fmtMonth.format(start);
  const startDay = start.getUTCDate();
  const endDay = end.getUTCDate();

  if (start.getUTCMonth() === end.getUTCMonth()) {
    return `${startMonth} ${startDay}-${endDay}`;
  }
  const endMonth = fmtMonth.format(end);
  return `${startMonth} ${startDay}-${endMonth} ${endDay}`;
}
