import { i18n } from "../stores/i18n.svelte";

/**
 * Parses a MusicBrainz date string (`YYYY-MM-DD`, `YYYY-MM`, or `YYYY`) and
 * formats it for display using the specified locale.
 */
export function formatArtistDate(dateStr?: string | null, locale = "en"): string {
  if (!dateStr || !dateStr.trim()) return "";
  const trimmed = dateStr.trim();
  const parts = trimmed.split("-");

  const year = parseInt(parts[0], 10);
  if (isNaN(year)) return trimmed;

  if (parts.length === 3) {
    const month = parseInt(parts[1], 10);
    const day = parseInt(parts[2], 10);
    if (!isNaN(month) && !isNaN(day)) {
      const date = new Date(Date.UTC(year, month - 1, day));
      return new Intl.DateTimeFormat(locale, {
        year: "numeric",
        month: "long",
        day: "numeric",
        timeZone: "UTC",
      }).format(date);
    }
  } else if (parts.length === 2) {
    const month = parseInt(parts[1], 10);
    if (!isNaN(month)) {
      const date = new Date(Date.UTC(year, month - 1, 1));
      return new Intl.DateTimeFormat(locale, {
        year: "numeric",
        month: "long",
        timeZone: "UTC",
      }).format(date);
    }
  }

  return String(year);
}

/**
 * Computes how many full years have elapsed from a MusicBrainz date string
 * (`YYYY-MM-DD`, `YYYY-MM`, or `YYYY`) up to `referenceDate` (defaults to now).
 */
export function getYearsAgo(dateStr?: string | null, referenceDate: Date = new Date()): number | null {
  if (!dateStr || !dateStr.trim()) return null;
  const parts = dateStr.trim().split("-");
  const year = parseInt(parts[0], 10);
  if (isNaN(year)) return null;

  const currentYear = referenceDate.getUTCFullYear();
  let years = currentYear - year;
  if (years < 0) return 0;

  const currentMonth = referenceDate.getUTCMonth() + 1; // 1-12
  const currentDay = referenceDate.getUTCDate();

  if (parts.length >= 2) {
    const month = parseInt(parts[1], 10);
    if (!isNaN(month)) {
      if (parts.length >= 3) {
        const day = parseInt(parts[2], 10);
        if (!isNaN(day)) {
          if (currentMonth < month || (currentMonth === month && currentDay < day)) {
            years--;
          }
        }
      } else if (currentMonth < month) {
        years--;
      }
    }
  }

  return Math.max(0, years);
}

/**
 * Returns a relative "N years ago" string localized via existing playlist
 * relative date keys.
 */
export function formatRelativeYears(years: number): string {
  if (years <= 0) {
    return i18n.t("artistInfo.lessThanOneYearAgo", {}, "<1 year ago");
  }
  return years === 1
    ? i18n.t("playlists.relativeOneYearAgo", {}, "1 year ago")
    : i18n.t("playlists.relativeYearsAgo", { count: years }, `${years} years ago`);
}

/**
 * Combines an absolute date with its relative "N years ago" duration, e.g.:
 * "August 28, 1965 (61 years ago)" or "1987 (39 years ago)".
 */
export function formatArtistLifeEvent(
  dateStr?: string | null,
  locale = "en",
  referenceDate: Date = new Date()
): string {
  if (!dateStr || !dateStr.trim()) return "";
  const formatted = formatArtistDate(dateStr, locale);
  const years = getYearsAgo(dateStr, referenceDate);
  if (years == null) return formatted;
  const relative = formatRelativeYears(years);
  return `${formatted} (${relative})`;
}

/**
 * Returns true if the artist entity represents an individual person.
 */
export function isArtistPerson(artistType?: string | null, gender?: string | null): boolean {
  if (artistType?.trim().toLowerCase() === "person") return true;
  if (!artistType && gender && gender.trim().length > 0) return true;
  return false;
}

/**
 * Returns true if the artist entity represents a group or band.
 */
export function isArtistGroup(artistType?: string | null): boolean {
  return artistType?.trim().toLowerCase() === "group";
}

/**
 * Localizes common MusicBrainz gender strings.
 */
export function resolveGenderLabel(gender?: string | null): string {
  if (!gender || !gender.trim()) return "";
  const normalized = gender.trim().toLowerCase();
  switch (normalized) {
    case "female":
      return i18n.t("artistInfo.genderFemale", {}, "Female");
    case "male":
      return i18n.t("artistInfo.genderMale", {}, "Male");
    case "non-binary":
      return i18n.t("artistInfo.genderNonBinary", {}, "Non-binary");
    case "other":
      return i18n.t("artistInfo.genderOther", {}, "Other");
    default:
      return gender.charAt(0).toUpperCase() + gender.slice(1);
  }
}

export interface AreaLinkItem {
  name: string;
  url?: string;
}

/**
 * Resolves birth/formation place and containing country into a list of
 * displayable segments, deduplicating if identical and attaching MusicBrainz
 * area URLs when an area MBID is present (#1128).
 */
export function getArtistAreaLinks(
  beginAreaName?: string | null,
  beginAreaMbid?: string | null,
  areaName?: string | null,
  areaMbid?: string | null
): AreaLinkItem[] {
  const items: AreaLinkItem[] = [];
  const cleanBegin = beginAreaName?.trim() || "";
  const cleanArea = areaName?.trim() || "";

  if (cleanBegin && cleanArea && cleanBegin.toLowerCase() === cleanArea.toLowerCase()) {
    items.push({
      name: cleanBegin,
      url: beginAreaMbid ? `https://musicbrainz.org/area/${beginAreaMbid}` : undefined,
    });
    return items;
  }

  if (cleanBegin) {
    items.push({
      name: cleanBegin,
      url: beginAreaMbid ? `https://musicbrainz.org/area/${beginAreaMbid}` : undefined,
    });
  }

  if (cleanArea) {
    items.push({
      name: cleanArea,
      url: areaMbid ? `https://musicbrainz.org/area/${areaMbid}` : undefined,
    });
  }

  return items;
}
