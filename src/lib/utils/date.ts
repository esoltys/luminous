import { i18n } from "../stores/i18n.svelte";

export function formatRelativeDate(timestampSec: number | undefined | null): string {
  if (!timestampSec) return "";
  const now = new Date();
  const date = new Date(timestampSec * 1000);

  const nowStart = new Date(now.getFullYear(), now.getMonth(), now.getDate());
  const dateStart = new Date(date.getFullYear(), date.getMonth(), date.getDate());

  const diffMs = nowStart.getTime() - dateStart.getTime();
  const diffDays = Math.floor(diffMs / (1000 * 60 * 60 * 24));

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
