import { i18n } from "../stores/i18n.svelte";
import { formatDate } from "./formatters";

function diffDaysFromNow(timestampSec: number): number {
  const now = new Date();
  const date = new Date(timestampSec * 1000);

  const nowStart = new Date(now.getFullYear(), now.getMonth(), now.getDate());
  const dateStart = new Date(date.getFullYear(), date.getMonth(), date.getDate());

  const diffMs = nowStart.getTime() - dateStart.getTime();
  return Math.floor(diffMs / (1000 * 60 * 60 * 24));
}

// Today / Yesterday / "N days ago" through 6 days, then falls back to the
// absolute date — used for the Date Added column, where older entries read
// better as a real date than as "3 weeks ago". Today's entries drill down
// further into minutes/hours ago rather than just reading "Today".
export function formatDateAdded(timestampSec: number | undefined | null): string {
  if (!timestampSec) return "—";
  const diffDays = diffDaysFromNow(timestampSec);

  if (diffDays <= 0) {
    const diffMinutes = Math.floor((Date.now() / 1000 - timestampSec) / 60);
    if (diffMinutes < 1) return i18n.t("playlists.relativeJustNow");
    if (diffMinutes < 60) {
      return diffMinutes === 1
        ? i18n.t("playlists.relativeOneMinuteAgo")
        : i18n.t("playlists.relativeMinutesAgo", { count: diffMinutes });
    }
    const diffHours = Math.floor(diffMinutes / 60);
    return diffHours === 1
      ? i18n.t("playlists.relativeOneHourAgo")
      : i18n.t("playlists.relativeHoursAgo", { count: diffHours });
  }
  if (diffDays === 1) return i18n.t("playlists.relativeYesterday");
  if (diffDays <= 6) return i18n.t("playlists.relativeDaysAgo", { count: diffDays });
  return formatDate(timestampSec);
}

export function formatRelativeDate(timestampSec: number | undefined | null): string {
  if (!timestampSec) return "";
  const diffDays = diffDaysFromNow(timestampSec);

  if (diffDays <= 0) return i18n.t("playlists.relativeToday");
  if (diffDays === 1) return i18n.t("playlists.relativeYesterday");
  if (diffDays < 7) return i18n.t("playlists.relativeDaysAgo", { count: diffDays });
  if (diffDays < 30) {
    const weeks = Math.floor(diffDays / 7);
    return weeks === 1
      ? i18n.t("playlists.relativeOneWeekAgo")
      : i18n.t("playlists.relativeWeeksAgo", { count: weeks });
  }
  if (diffDays < 365) {
    const months = Math.floor(diffDays / 30);
    return months === 1
      ? i18n.t("playlists.relativeOneMonthAgo")
      : i18n.t("playlists.relativeMonthsAgo", { count: months });
  }
  const years = Math.floor(diffDays / 365);
  return years === 1
    ? i18n.t("playlists.relativeOneYearAgo")
    : i18n.t("playlists.relativeYearsAgo", { count: years });
}
