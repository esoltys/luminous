import { describe, it, expect, beforeEach, vi, afterEach } from "vitest";
import { formatDateAdded, formatWeekRange } from "./date";
import { i18n } from "../stores/i18n.svelte";

describe("formatDateAdded", () => {
  beforeEach(() => {
    i18n.currentLocale = "en-CA";
    vi.useFakeTimers();
    // Fixed reference time: 2026-09-13T12:00:00Z (midday)
    vi.setSystemTime(new Date("2026-09-13T12:00:00Z"));
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it("returns an em-dash for undefined, null, or 0", () => {
    expect(formatDateAdded(undefined)).toBe("—");
    expect(formatDateAdded(null)).toBe("—");
    expect(formatDateAdded(0)).toBe("—");
  });

  it("returns 'Just now' for timestamps within 1 minute", () => {
    const nowSec = Math.floor(Date.now() / 1000);
    expect(formatDateAdded(nowSec)).toBe("Just now");
    expect(formatDateAdded(nowSec - 30)).toBe("Just now");
  });

  it("returns '1 minute ago' and '{n} minutes ago'", () => {
    const nowSec = Math.floor(Date.now() / 1000);
    expect(formatDateAdded(nowSec - 60)).toBe("1 minute ago");
    expect(formatDateAdded(nowSec - 15 * 60)).toBe("15 minutes ago");
    expect(formatDateAdded(nowSec - 59 * 60)).toBe("59 minutes ago");
  });

  it("returns '1 hour ago' and '{n} hours ago'", () => {
    const nowSec = Math.floor(Date.now() / 1000);
    expect(formatDateAdded(nowSec - 3600)).toBe("1 hour ago");
    expect(formatDateAdded(nowSec - 5 * 3600)).toBe("5 hours ago");
  });

  it("counts minutes and hours across midnight instead of reading 'Yesterday' (#1299)", () => {
    vi.setSystemTime(new Date(2026, 8, 13, 0, 5));
    const nowSec = Math.floor(Date.now() / 1000);
    expect(formatDateAdded(nowSec - 15 * 60)).toBe("15 minutes ago");
    expect(formatDateAdded(nowSec - 5 * 3600)).toBe("5 hours ago");
    expect(formatDateAdded(nowSec - 6 * 3600)).toBe("Yesterday");
  });

  it("returns 'Yesterday' for 1 calendar day ago", () => {
    const yesterday = new Date("2026-09-12T12:00:00Z");
    const yesterdaySec = Math.floor(yesterday.getTime() / 1000);
    expect(formatDateAdded(yesterdaySec)).toBe("Yesterday");
  });

  it("returns '{n} days ago' for 2 to 6 calendar days ago", () => {
    const threeDaysAgo = new Date("2026-09-10T12:00:00Z");
    const threeDaysAgoSec = Math.floor(threeDaysAgo.getTime() / 1000);
    expect(formatDateAdded(threeDaysAgoSec)).toBe("3 days ago");

    const sixDaysAgo = new Date("2026-09-07T12:00:00Z");
    const sixDaysAgoSec = Math.floor(sixDaysAgo.getTime() / 1000);
    expect(formatDateAdded(sixDaysAgoSec)).toBe("6 days ago");
  });

  it("falls back to absolute date string after 6 days", () => {
    const tenDaysAgo = new Date("2026-09-03T12:00:00Z");
    const tenDaysAgoSec = Math.floor(tenDaysAgo.getTime() / 1000);
    const expected = new Date(tenDaysAgoSec * 1000).toLocaleDateString();
    expect(formatDateAdded(tenDaysAgoSec)).toBe(expected);
  });
});

describe("formatWeekRange", () => {
  // The separator is an en dash, with or without spaces or thin spaces depending on the locale
  // and ICU version, so these match the shape rather than the exact whitespace.
  const DASH = "\\s*[–-]\\s*";
  const weekOf = (y: number, m: number, d: number) => Math.floor(Date.UTC(y, m, d) / 1000);

  it("formats a week inside one month with the month once", () => {
    expect(formatWeekRange(weekOf(2026, 8, 21), "en")).toMatch(new RegExp(`^Sep 21${DASH}27$`));
  });

  it("names both months when the week crosses a month", () => {
    expect(formatWeekRange(weekOf(2026, 8, 27), "en")).toMatch(new RegExp(`^Sep 27${DASH}Oct 3$`));
  });

  it("names both months when the week crosses a year", () => {
    expect(formatWeekRange(weekOf(2026, 11, 28), "en")).toMatch(new RegExp(`^Dec 28${DASH}Jan 3$`));
  });

  it("puts the day first in French", () => {
    expect(formatWeekRange(weekOf(2026, 10, 1), "fr")).toMatch(new RegExp(`^1${DASH}7 nov\\.$`));
    expect(formatWeekRange(weekOf(2026, 8, 27), "fr")).toMatch(
      new RegExp(`^27 sept\\.${DASH}3 oct\\.$`),
    );
  });

  it("uses German ordinal days with the month after them", () => {
    expect(formatWeekRange(weekOf(2026, 10, 1), "de")).toMatch(
      new RegExp(`^1\\.${DASH}7\\. Nov\\.$`),
    );
  });

  it("reads the date back unshifted whatever the local time zone", () => {
    // 2026-11-01T00:00:00Z is still Oct 31 in the Americas; the range must start on the 1st.
    // Pin a zone behind UTC so this fails on a UTC runner too if the date ever shifts.
    const originalTz = process.env.TZ;
    process.env.TZ = "America/Los_Angeles";
    try {
      expect(formatWeekRange(weekOf(2026, 10, 1), "en-CA")).toMatch(/^Nov 1/);
    } finally {
      if (originalTz === undefined) delete process.env.TZ;
      else process.env.TZ = originalTz;
    }
  });
});
