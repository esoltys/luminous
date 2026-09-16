import { describe, it, expect, beforeEach, vi, afterEach } from "vitest";
import { formatDateAdded } from "./date";
import { i18n } from "../stores/i18n.svelte";

describe("formatDateAdded", () => {
  beforeEach(() => {
    i18n.currentLocale = "en";
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
