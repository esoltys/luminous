import { describe, it, expect } from "vitest";
import { getDaypartBucket } from "./daypart";

describe("getDaypartBucket", () => {
  // Boundaries must match src-tauri/src/playlist/auto_sync.rs's
  // daypart_bucket_for_hour exactly (#223) — this is the frontend half of
  // that shared convention.
  it("buckets 05:00-11:59 as morning", () => {
    expect(getDaypartBucket(new Date(2026, 0, 1, 5, 0))).toBe("morning");
    expect(getDaypartBucket(new Date(2026, 0, 1, 11, 59))).toBe("morning");
  });

  it("buckets 12:00-16:59 as afternoon", () => {
    expect(getDaypartBucket(new Date(2026, 0, 1, 12, 0))).toBe("afternoon");
    expect(getDaypartBucket(new Date(2026, 0, 1, 16, 59))).toBe("afternoon");
  });

  it("buckets 17:00-20:59 as evening", () => {
    expect(getDaypartBucket(new Date(2026, 0, 1, 17, 0))).toBe("evening");
    expect(getDaypartBucket(new Date(2026, 0, 1, 20, 59))).toBe("evening");
  });

  it("buckets 21:00-04:59 as latenight, wrapping midnight", () => {
    expect(getDaypartBucket(new Date(2026, 0, 1, 21, 0))).toBe("latenight");
    expect(getDaypartBucket(new Date(2026, 0, 1, 23, 59))).toBe("latenight");
    expect(getDaypartBucket(new Date(2026, 0, 1, 0, 0))).toBe("latenight");
    expect(getDaypartBucket(new Date(2026, 0, 1, 4, 59))).toBe("latenight");
  });
});
