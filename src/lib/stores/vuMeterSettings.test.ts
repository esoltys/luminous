import { beforeEach, describe, expect, it } from "vitest";
import { VU_REFERENCE_RANGE_DBFS } from "../visualizers/vuMeterEngine";
import { VuMeterSettingsStore } from "./vuMeterSettings.svelte";

describe("VuMeterSettingsStore", () => {
  beforeEach(() => localStorage.clear());

  it("calibrates itself until a level is set by hand", () => {
    expect(new VuMeterSettingsStore().referenceDbfs).toBeNull();
  });

  it("keeps a manual level across a relaunch", () => {
    new VuMeterSettingsStore().setManual(-12);
    expect(new VuMeterSettingsStore().referenceDbfs).toBe(-12);
  });

  it("returns to Auto, and stays there after a relaunch", () => {
    const store = new VuMeterSettingsStore();
    store.setManual(-12);
    store.setAuto();
    expect(store.referenceDbfs).toBeNull();
    expect(new VuMeterSettingsStore().referenceDbfs).toBeNull();
  });

  it("holds a manual level within the allowed range", () => {
    const store = new VuMeterSettingsStore();
    store.setManual(-90);
    expect(store.referenceDbfs).toBe(VU_REFERENCE_RANGE_DBFS.min);
    store.setManual(12);
    expect(store.referenceDbfs).toBe(VU_REFERENCE_RANGE_DBFS.max);
  });

  it("ignores a saved value that is not a number, and clamps one out of range", () => {
    localStorage.setItem("vu_meter_reference_dbfs", "loud");
    expect(new VuMeterSettingsStore().referenceDbfs).toBeNull();
    localStorage.setItem("vu_meter_reference_dbfs", "-99");
    expect(new VuMeterSettingsStore().referenceDbfs).toBe(VU_REFERENCE_RANGE_DBFS.min);
  });
});
