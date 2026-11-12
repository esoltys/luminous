import { describe, expect, it } from "vitest";
import {
  applyDateOpChoice,
  dateOpChoice,
  formatRelative,
  legacyTimestampToDay,
  parseRelative,
} from "./dateRule";

describe("parseRelative / formatRelative", () => {
  it("round-trips an offset", () => {
    expect(parseRelative("-30d")).toEqual({ count: 30, unit: "d" });
    expect(parseRelative("-2Y")).toEqual({ count: 2, unit: "y" });
    expect(formatRelative(2, "w")).toBe("-2w");
  });

  it("rejects anything that is not an offset", () => {
    for (const v of ["", "30d", "-d", "-3x", "2026-01-31", "-5", "1700000000"]) {
      expect(parseRelative(v)).toBeNull();
    }
  });
});

describe("dateOpChoice", () => {
  it("reads >= / < with an offset as the relative windows", () => {
    expect(dateOpChoice(">=", "-30d")).toBe("within");
    expect(dateOpChoice("<", "-1y")).toBe("outside");
  });

  it("reads the same operators with a day as plain comparisons", () => {
    expect(dateOpChoice(">=", "2026-01-31")).toBe(">=");
    expect(dateOpChoice("<", "")).toBe("<");
    expect(dateOpChoice("=", "2026-01-31")).toBe("=");
  });

  it("falls back to 'is' for an operator a date field doesn't offer", () => {
    expect(dateOpChoice("contains", "")).toBe("=");
  });
});

describe("applyDateOpChoice", () => {
  it("seeds a window when switching from a day, so the rule stays valid", () => {
    expect(applyDateOpChoice("within", { op: ">=", value: "2026-01-31" })).toEqual({
      op: ">=",
      value: "-30d",
    });
    expect(applyDateOpChoice("outside", { op: "=", value: "" })).toEqual({
      op: "<",
      value: "-30d",
    });
  });

  it("keeps the window when flipping between in / not in the last", () => {
    expect(applyDateOpChoice("outside", { op: ">=", value: "-2w" })).toEqual({
      op: "<",
      value: "-2w",
    });
  });

  it("clears an offset when switching to a day comparison, but keeps a day", () => {
    expect(applyDateOpChoice("<=", { op: ">=", value: "-2w" })).toEqual({ op: "<=", value: "" });
    expect(applyDateOpChoice("<=", { op: ">=", value: "2026-01-31" })).toEqual({
      op: "<=",
      value: "2026-01-31",
    });
  });
});

describe("legacyTimestampToDay", () => {
  it("shows an epoch-seconds value as its local day", () => {
    const noon = new Date(2026, 0, 31, 12, 0, 0).getTime() / 1000;
    expect(legacyTimestampToDay(String(noon))).toBe("2026-01-31");
  });

  it("leaves days, offsets and empty values alone", () => {
    for (const v of ["2026-01-31", "-30d", ""]) expect(legacyTimestampToDay(v)).toBe(v);
  });
});
