// @vitest-environment node
import fs from "fs";
import path from "path";
import { describe, expect, it } from "vitest";
// The MSIX bundler's own padding, so this checks what actually ships.
import { toFourPartVersion } from "@choochmeque/tauri-windows-bundle";
import { nextCalVer, parseCalVer } from "./calver";

const root = path.resolve(__dirname, "..");
const read = (rel: string) => fs.readFileSync(path.join(root, rel), "utf-8");

const utc = (iso: string) => new Date(iso);

describe("parseCalVer", () => {
  it("accepts YYYY.M.PATCH with or without a leading v", () => {
    expect(parseCalVer("2026.11.0")).toEqual({ year: 2026, month: 11, patch: 0 });
    expect(parseCalVer("v2027.1.12")).toEqual({ year: 2027, month: 1, patch: 12 });
  });

  it("rejects zero-padded months, month 0/13, leading-zero patches and SemVer", () => {
    for (const bad of [
      "2026.01.0",
      "2026.0.0",
      "2026.13.0",
      "2026.11.01",
      "4.1.0",
      "26.11.0",
      "2026.11",
    ]) {
      expect(parseCalVer(bad), bad).toBeNull();
    }
  });
});

describe("nextCalVer", () => {
  it("starts a month at PATCH 0", () => {
    expect(nextCalVer(utc("2026-12-03T10:00:00Z"), ["v2026.11.0", "v2026.11.1"], "2026.11.1")).toBe(
      "2026.12.0",
    );
  });

  it("increments past the highest tag this month", () => {
    expect(
      nextCalVer(
        utc("2026-11-28T00:00:00Z"),
        ["v2026.11.0", "v2026.11.2", "v2026.11.1"],
        "2026.11.2",
      ),
    ).toBe("2026.11.3");
  });

  it("counts the manifest version when it is ahead of the tags (4.x releases had no CalVer tag)", () => {
    expect(nextCalVer(utc("2026-11-28T00:00:00Z"), ["v4.0.0", "v4.1.0"], "2026.11.1")).toBe(
      "2026.11.2",
    );
  });

  it("reads the month in UTC, not local time", () => {
    // 23:30 on Nov 30 in UTC-5 is already December in UTC.
    expect(nextCalVer(utc("2026-12-01T04:30:00Z"), [], "2026.11.4")).toBe("2026.12.0");
  });

  it("does not confuse month 1 with months 10-12", () => {
    const tags = ["v2027.10.0", "v2027.11.3", "v2027.12.5"];
    expect(nextCalVer(utc("2027-01-15T00:00:00Z"), tags, "2027.12.5")).toBe("2027.1.0");
    expect(nextCalVer(utc("2027-10-15T00:00:00Z"), ["v2027.1.4"], "2027.1.4")).toBe("2027.10.0");
  });

  it("does not carry a patch over from the same month of another year", () => {
    expect(nextCalVer(utc("2027-11-02T00:00:00Z"), ["v2026.11.7"], "2026.11.7")).toBe("2027.11.0");
  });
});

describe("repo version manifests", () => {
  const pkgVersion: string = JSON.parse(read("package.json")).version;

  it("package.json holds a calendar version", () => {
    expect(parseCalVer(pkgVersion), pkgVersion).not.toBeNull();
  });

  it("tauri.conf.json, Cargo.toml and Cargo.lock match package.json", () => {
    expect(JSON.parse(read("src-tauri/tauri.conf.json")).version).toBe(pkgVersion);
    expect(read("src-tauri/Cargo.toml")).toMatch(new RegExp(`^version = "${pkgVersion}"`, "m"));
    expect(read("Cargo.lock")).toContain(`name = "luminous"\nversion = "${pkgVersion}"`);
  });

  it("pads to a four-part MSIX version with every part <= 65535", () => {
    const msix = toFourPartVersion(pkgVersion);
    expect(msix).toBe(`${pkgVersion}.0`);
    for (const part of msix.split(".")) expect(Number(part)).toBeLessThanOrEqual(65535);
  });
});
