import { afterEach, beforeEach, describe, expect, it } from "vitest";
import {
  DEFAULT_PRESET,
  getPreset,
  listPresets,
  registerPreset,
} from "../visualizers/registry.svelte";
import type { VisualizerPreset } from "../visualizers/types";
import { VECTORSCOPE_PRESET } from "../visualizers/vectorscope";
import { VOXEL_LOGO_PRESET } from "../visualizers/voxelLogo";
import { VisualizerStore } from "./visualizer.svelte";

function fakePreset(id: string): VisualizerPreset {
  return {
    id,
    labelKey: id,
    needs: ["spectrum"],
    create: () => ({ setTheme() {}, push() {}, step: () => false, render() {} }),
  };
}

/** Leaves only the voxel spectrum registered; the returned function restores the rest in order. */
function setOtherBuiltinsAside(): () => void {
  registerPreset(VOXEL_LOGO_PRESET)();
  registerPreset(VECTORSCOPE_PRESET)();
  return () => {
    registerPreset(VOXEL_LOGO_PRESET);
    registerPreset(VECTORSCOPE_PRESET);
  };
}

describe("visualizer registry (#2245)", () => {
  it("ships the voxel spectrum, voxel logo and vectorscope, spectrum first", () => {
    expect(listPresets().map((p) => p.id)).toEqual(["voxel", "voxel-logo", "vectorscope"]);
    expect(DEFAULT_PRESET.id).toBe("voxel");
  });

  it("returns the default preset for an unknown or missing id", () => {
    expect(getPreset("no-such-preset")).toBe(DEFAULT_PRESET);
    expect(getPreset(null)).toBe(DEFAULT_PRESET);
  });

  it("registers and unregisters a preset, replacing one with the same id", () => {
    const first = fakePreset("pack-a");
    const second = fakePreset("pack-a");
    const removeFirst = registerPreset(first);
    const removeSecond = registerPreset(second);
    expect(listPresets().filter((p) => p.id === "pack-a")).toEqual([second]);
    removeFirst();
    expect(getPreset("pack-a")).toBe(second);
    removeSecond();
    expect(getPreset("pack-a")).toBe(DEFAULT_PRESET);
  });
});

describe("VisualizerStore (#2245)", () => {
  const cleanups: (() => void)[] = [];

  beforeEach(() => {
    localStorage.clear();
  });

  afterEach(() => {
    while (cleanups.length) cleanups.pop()!();
    localStorage.clear();
  });

  it("has nothing to switch between with a single preset registered", () => {
    cleanups.push(setOtherBuiltinsAside());
    const store = new VisualizerStore();
    expect(store.preset.id).toBe("voxel");
    expect(store.canSwitch).toBe(false);
  });

  it("cycles both ways, wrapping at the ends, and counts each switch", () => {
    cleanups.push(setOtherBuiltinsAside());
    cleanups.push(registerPreset(fakePreset("b")), registerPreset(fakePreset("c")));
    const store = new VisualizerStore();
    expect(store.canSwitch).toBe(true);

    store.cycle(1);
    expect(store.preset.id).toBe("b");
    store.cycle(1);
    store.cycle(1);
    expect(store.preset.id).toBe("voxel");
    store.cycle(-1);
    expect(store.preset.id).toBe("c");
    expect(store.neighbour(1).id).toBe("voxel");
    expect(store.switchCount).toBe(4);
  });

  it("ignores selecting an unknown id or the current preset", () => {
    cleanups.push(registerPreset(fakePreset("b")));
    const store = new VisualizerStore();
    store.select("no-such-preset");
    store.select("voxel");
    expect(store.preset.id).toBe("voxel");
    expect(store.switchCount).toBe(0);
  });

  it("restores the saved choice on the next launch", () => {
    cleanups.push(registerPreset(fakePreset("b")));
    new VisualizerStore().select("b");
    expect(new VisualizerStore().preset.id).toBe("b");
  });

  it("falls back to voxel while a saved preset isn't registered, keeping the saved choice", () => {
    localStorage.setItem("visualizer_preset", "uninstalled-pack");
    expect(new VisualizerStore().preset.id).toBe("voxel");
    expect(localStorage.getItem("visualizer_preset")).toBe("uninstalled-pack");

    cleanups.push(registerPreset(fakePreset("uninstalled-pack")));
    expect(new VisualizerStore().preset.id).toBe("uninstalled-pack");
  });
});
