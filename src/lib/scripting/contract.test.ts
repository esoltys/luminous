import { describe, it, expect, vi } from "vitest";
import { readFileSync } from "node:fs";
import path from "node:path";
import { createScriptingApi } from "./index";
import { methodPaths } from "./contract";
import history from "./contract-history.json";
import {
  SCRIPT_API_VERSION,
  type ScriptAutoPlaylistKind,
  type ScriptPlaybackState,
  type ScriptSettingsTab,
  type ScriptSong,
  type ScriptSongColumn,
  type ScriptSubTab,
  type ScriptTab,
} from "./types";
import type { ActiveSubTab, ActiveTab, AutoPlaylistRef, SettingsTab } from "../stores/navigation.svelte";
import type { VisibleColumns } from "../stores/collection.svelte";
import type { PlaybackState, Song } from "../types";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn().mockResolvedValue(() => {}) }));

// types.ts copies these app types so it can be vendored without imports.
// Each line fails `bun run check` once a copy drifts from its original.
type Equal<A, B> = (<T>() => T extends A ? 1 : 2) extends <T>() => T extends B ? 1 : 2 ? true : false;
type Assert<T extends true> = T;
export type DriftChecks = [
  Assert<Equal<ScriptTab, ActiveTab>>,
  Assert<Equal<ScriptSubTab, ActiveSubTab>>,
  Assert<Equal<ScriptSettingsTab, SettingsTab>>,
  Assert<Equal<ScriptAutoPlaylistKind, AutoPlaylistRef["kind"]>>,
  Assert<Equal<ScriptSongColumn, keyof VisibleColumns>>,
  // The contract promises a subset of the real song and playback snapshot: every field
  // it names must still exist (a dropped optional field would stay assignable) and fit.
  Assert<Equal<Exclude<keyof ScriptSong, keyof Song>, never>>,
  Assert<Song extends ScriptSong ? true : false>,
  Assert<Equal<Exclude<keyof ScriptPlaybackState, keyof PlaybackState>, never>>,
  Assert<PlaybackState extends ScriptPlaybackState ? true : false>,
];

type Version = [number, number, number];
const parse = (v: string) => v.split(".").map(Number) as Version;

describe("Scripting contract", () => {
  describe("connect()", () => {
    it("resolves the version and every method path for a satisfied requirement", async () => {
      const api = createScriptingApi();
      const contract = await api.connect(SCRIPT_API_VERSION);
      expect(contract.version).toBe(SCRIPT_API_VERSION);
      expect(contract.capabilities).toEqual(methodPaths(api));
      expect(contract.capabilities).toContain("dialogs.welcome.show");
      expect(contract.capabilities).not.toContain("version");
    });

    it("accepts an older minor of the same major in any precision", async () => {
      const api = createScriptingApi();
      const [major] = parse(SCRIPT_API_VERSION);
      for (const requires of [`${major}`, `${major}.0`, `${major}.0.0`]) {
        await expect(api.connect(requires)).resolves.toMatchObject({ version: SCRIPT_API_VERSION });
      }
    });

    it("rejects a newer minor, naming the range it needed", async () => {
      const [major, minor] = parse(SCRIPT_API_VERSION);
      const requires = `${major}.${minor + 1}`;
      await expect(createScriptingApi().connect(requires)).rejects.toThrow(
        `Luminous scripting API ${SCRIPT_API_VERSION} does not satisfy ^${requires}.0 (needs ≥${requires}.0 <${major + 1}.0.0).`
      );
    });

    it("rejects another major, even an older one", async () => {
      const [major] = parse(SCRIPT_API_VERSION);
      await expect(createScriptingApi().connect(`${major + 1}`)).rejects.toThrow("does not satisfy");
      if (major > 0) await expect(createScriptingApi().connect(`${major - 1}`)).rejects.toThrow("does not satisfy");
    });

    it("names every missing capability alongside a version mismatch", async () => {
      const [major] = parse(SCRIPT_API_VERSION);
      await expect(createScriptingApi().connect(`${major + 1}`, ["wait.settled", "wait.nope", "x.y"])).rejects.toThrow(
        /does not satisfy .* and lacks wait\.nope, x\.y\.$/
      );
    });

    it("rejects a requirement that isn't a version", async () => {
      await expect(createScriptingApi().connect(">=1.0")).rejects.toThrow('not ">=1.0"');
    });
  });

  describe("Version ledger (contract-history.json)", () => {
    const versions = Object.keys(history) as (keyof typeof history)[];

    it("ends at the current version with exactly the live surface", () => {
      expect(versions.at(-1)).toBe(SCRIPT_API_VERSION);
      // If this fails, append a ledger entry for the new surface and bump
      // SCRIPT_API_VERSION in types.ts: minor for additions, major for removals.
      // Editing the newest entry in place instead of appending passes; review catches that.
      expect(history[SCRIPT_API_VERSION as keyof typeof history]).toEqual(methodPaths(createScriptingApi()));
    });

    it("bumps major for every removed method and at least minor for every added one", () => {
      for (let i = 1; i < versions.length; i++) {
        const [before, after] = [versions[i - 1], versions[i]];
        const [prev, next] = [parse(before), parse(after)];
        const removed = history[before].filter((m) => !(history[after] as string[]).includes(m));
        const added = history[after].filter((m) => !(history[before] as string[]).includes(m));
        const majorBump = next[0] > prev[0];
        const minorBump = majorBump || (next[0] === prev[0] && next[1] > prev[1]);
        expect({ step: `${before} → ${after}`, removedWithoutMajor: majorBump ? [] : removed }).toEqual({
          step: `${before} → ${after}`,
          removedWithoutMajor: [],
        });
        expect({ step: `${before} → ${after}`, addedWithoutMinor: minorBump ? [] : added }).toEqual({
          step: `${before} → ${after}`,
          addedWithoutMinor: [],
        });
        expect(prev[0] < next[0] || (prev[0] === next[0] && (prev[1] < next[1] || (prev[1] === next[1] && prev[2] < next[2])))).toBe(true);
      }
    });
  });

  it("keeps types.ts free of imports so it can be vendored", () => {
    const source = readFileSync(path.resolve(__dirname, "types.ts"), "utf8");
    expect(source).not.toMatch(/^\s*import\b|\bimport\(|^\s*export\s+(\*|\{[^}]*\})\s+from\b/m);
  });
});
