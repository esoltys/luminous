// @vitest-environment node
import { describe, expect, it } from "vitest";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import {
  checkTauriVersions,
  formatMismatches,
  npmToCrateName,
  parseBunLock,
  parseCargoLock,
  parseMajorMinor,
} from "./check-tauri-versions.ts";

describe("npmToCrateName", () => {
  it("maps @tauri-apps/api to tauri", () => {
    expect(npmToCrateName("@tauri-apps/api")).toBe("tauri");
  });

  it("maps @tauri-apps/plugin-* to tauri-plugin-*", () => {
    expect(npmToCrateName("@tauri-apps/plugin-updater")).toBe("tauri-plugin-updater");
    expect(npmToCrateName("@tauri-apps/plugin-dialog")).toBe("tauri-plugin-dialog");
    expect(npmToCrateName("@tauri-apps/plugin-opener")).toBe("tauri-plugin-opener");
    expect(npmToCrateName("@tauri-apps/plugin-process")).toBe("tauri-plugin-process");
  });

  it("skips @tauri-apps/cli packages", () => {
    expect(npmToCrateName("@tauri-apps/cli")).toBeNull();
    expect(npmToCrateName("@tauri-apps/cli-win32-x64-msvc")).toBeNull();
    expect(npmToCrateName("@tauri-apps/cli-linux-arm64-gnu")).toBeNull();
  });

  it("returns null for unrelated packages", () => {
    expect(npmToCrateName("svelte")).toBeNull();
    expect(npmToCrateName("@tailwindcss/vite")).toBeNull();
  });
});

describe("parseMajorMinor", () => {
  it("extracts major and minor numbers", () => {
    expect(parseMajorMinor("2.12.0")).toEqual({ major: 2, minor: 12 });
    expect(parseMajorMinor("2.11.5")).toEqual({ major: 2, minor: 11 });
    expect(parseMajorMinor("0.1.0-alpha.1")).toEqual({ major: 0, minor: 1 });
  });

  it("returns null for non-semver strings", () => {
    expect(parseMajorMinor("latest")).toBeNull();
    expect(parseMajorMinor("")).toBeNull();
  });
});

describe("checkTauriVersions", () => {
  const sampleCargoLock = `
[[package]]
name = "tauri"
version = "2.11.5"

[[package]]
name = "tauri-plugin-updater"
version = "2.11.0"

[[package]]
name = "tauri-plugin-dialog"
version = "2.7.2"

[[package]]
name = "tauri-plugin-opener"
version = "2.5.4"

[[package]]
name = "tauri-plugin-process"
version = "2.3.1"

[[package]]
name = "tauri-plugin-fs"
version = "2.5.1"
`;

  it("passes when major and minor match (matching pairs)", () => {
    const bunLock = JSON.stringify({
      packages: {
        "@tauri-apps/api": ["@tauri-apps/api@2.11.5"],
        "@tauri-apps/plugin-process": ["@tauri-apps/plugin-process@2.3.1"],
      },
    });

    const result = checkTauriVersions(bunLock, sampleCargoLock);
    expect(result.mismatches).toHaveLength(0);
    expect(result.checked).toHaveLength(2);
    expect(result.checked).toEqual([
      {
        npmPackage: "@tauri-apps/api",
        npmVersion: "2.11.5",
        crateName: "tauri",
        crateVersion: "2.11.5",
      },
      {
        npmPackage: "@tauri-apps/plugin-process",
        npmVersion: "2.3.1",
        crateName: "tauri-plugin-process",
        crateVersion: "2.3.1",
      },
    ]);
  });

  it("tolerates patch-only differences", () => {
    // npm is 2.7.3 while crate is 2.7.2; npm api is 2.11.1 while crate is 2.11.5
    const bunLock = JSON.stringify({
      packages: {
        "@tauri-apps/api": ["@tauri-apps/api@2.11.1"],
        "@tauri-apps/plugin-dialog": ["@tauri-apps/plugin-dialog@2.7.3"],
        "@tauri-apps/plugin-opener": ["@tauri-apps/plugin-opener@2.5.5"],
      },
    });

    const result = checkTauriVersions(bunLock, sampleCargoLock);
    expect(result.mismatches).toHaveLength(0);
    expect(result.checked).toHaveLength(3);
  });

  it("fails when major or minor versions differ (minor mismatch)", () => {
    // npm updater is 2.12.0 while cargo crate is 2.11.0 (the v2.5.0 bug in #1194/#1207)
    const bunLock = JSON.stringify({
      packages: {
        "@tauri-apps/plugin-updater": ["@tauri-apps/plugin-updater@2.12.0"],
      },
    });

    const result = checkTauriVersions(bunLock, sampleCargoLock);
    expect(result.mismatches).toHaveLength(1);
    expect(result.mismatches[0]).toEqual({
      npmPackage: "@tauri-apps/plugin-updater",
      npmVersion: "2.12.0",
      crateName: "tauri-plugin-updater",
      crateVersion: "2.11.0",
      errorType: "version_mismatch",
    });

    const errorMsg = formatMismatches(result.mismatches);
    expect(errorMsg).toContain("@tauri-apps/plugin-updater (npm: 2.12.0) ↔ tauri-plugin-updater (crate: 2.11.0)");
    expect(errorMsg).toContain("cargo update -p tauri-plugin-updater --precise 2.12.0");
    expect(errorMsg).toContain("bun add @tauri-apps/plugin-updater@^2.11.0");
  });

  it("skips @tauri-apps/cli* packages with no crate counterparts", () => {
    const bunLock = JSON.stringify({
      packages: {
        "@tauri-apps/cli": ["@tauri-apps/cli@2.12.0"],
        "@tauri-apps/cli-win32-x64-msvc": ["@tauri-apps/cli-win32-x64-msvc@2.12.0"],
      },
    });

    const result = checkTauriVersions(bunLock, sampleCargoLock);
    expect(result.mismatches).toHaveLength(0);
    expect(result.checked).toHaveLength(0);
  });

  it("ignores Rust-only crates that have no npm package in bun.lock", () => {
    // sampleCargoLock has tauri-plugin-fs, but bunLock does not import it
    const bunLock = JSON.stringify({
      packages: {
        "@tauri-apps/api": ["@tauri-apps/api@2.11.0"],
      },
    });

    const result = checkTauriVersions(bunLock, sampleCargoLock);
    expect(result.mismatches).toHaveLength(0);
    expect(result.checked).toHaveLength(1);
    expect(result.checked[0].crateName).toBe("tauri");
  });

  it("flags missing crates when an npm plugin has no Cargo.lock counterpart", () => {
    const bunLock = JSON.stringify({
      packages: {
        "@tauri-apps/plugin-unregistered": ["@tauri-apps/plugin-unregistered@2.0.0"],
      },
    });

    const result = checkTauriVersions(bunLock, sampleCargoLock);
    expect(result.mismatches).toHaveLength(1);
    expect(result.mismatches[0]).toEqual({
      npmPackage: "@tauri-apps/plugin-unregistered",
      npmVersion: "2.0.0",
      crateName: "tauri-plugin-unregistered",
      errorType: "missing_crate",
    });

    const errorMsg = formatMismatches(result.mismatches);
    expect(errorMsg).toContain("has no matching 'tauri-plugin-unregistered' crate in Cargo.lock");
  });

  it("successfully validates the real repository lockfiles", () => {
    const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
    const bunLockContent = fs.readFileSync(path.join(root, "bun.lock"), "utf-8");
    const cargoLockContent = fs.readFileSync(path.join(root, "Cargo.lock"), "utf-8");

    const result = checkTauriVersions(bunLockContent, cargoLockContent);
    expect(result.mismatches).toEqual([]);
    expect(result.checked.length).toBeGreaterThanOrEqual(5);

    const checkedNpmNames = result.checked.map((c) => c.npmPackage);
    expect(checkedNpmNames).toContain("@tauri-apps/api");
    expect(checkedNpmNames).toContain("@tauri-apps/plugin-dialog");
    expect(checkedNpmNames).toContain("@tauri-apps/plugin-opener");
    expect(checkedNpmNames).toContain("@tauri-apps/plugin-process");
    expect(checkedNpmNames).toContain("@tauri-apps/plugin-updater");
  });
});
